use kitchen_sink::ArcRichCrateVersion;
use either::*;
use failure;
use futures::future::join_all;
use futures::future::FutureExt;
use futures::stream::StreamExt;
use kitchen_sink::RichCrate;
use kitchen_sink::{self, stop, stopped, KitchenSink, MaintenanceStatus, Origin, RichCrateVersion};
use parking_lot::Mutex;
use rand::{seq::SliceRandom, thread_rng};
use ranking::CrateTemporalInputs;
use ranking::CrateVersionInputs;
use render_readme::Links;
use render_readme::Renderer;
use search_index::*;
use std::collections::HashSet;
use tokio::sync::mpsc;
use triomphe::Arc;
use udedokei::LanguageExt;

#[tokio::main]
async fn main() {
    let handle = Arc::new(tokio::runtime::Handle::current());
    handle.clone().spawn(async move {

    let crates = Arc::new(match kitchen_sink::KitchenSink::new_default().await {
        Ok(a) => a,
        e => {
            print_res(e);
            std::process::exit(1);
        },
    });
    let renderer = Arc::new(Renderer::new(None));
    let pre = handle.spawn({
        let c = Arc::clone(&crates);
        async move { c.prewarm().await }
    });

    let everything = std::env::args().nth(1).map_or(false, |a| a == "--all");
    let specific: Vec<_> = if !everything {
        std::env::args().skip(1).map(|name| {
            Origin::try_from_crates_io_name(&name).unwrap_or_else(|| Origin::from_str(name))
        }).collect()
    } else {
        Vec::new()
    };
    let repos = !everything;

    let mut indexer = Indexer::new(CrateSearchIndex::new(crates.main_cache_dir()).expect("init search")).expect("init search indexer");

    let (tx, mut rx) = mpsc::channel::<(Arc<_>, _, _)>(64);
    let index_thread = handle.spawn({
        let renderer = renderer.clone();
        async move {
            let mut n = 0usize;
            let mut next_n = 100usize;
            while let Some((ver, downloads_per_month, score)) = rx.recv().await {
                if stopped() {break;}
                tokio::task::block_in_place(|| {
                    index_search(&mut indexer, &renderer, &ver, downloads_per_month, score)?;
                    n += 1;
                    if n == next_n {
                        next_n *= 2;
                        println!("savepoint…");
                        indexer.commit()?;
                    }
                    Ok::<_, failure::Error>(())
                })?;
            }
            tokio::task::block_in_place(|| indexer.commit())?;
            let _ = indexer.bye()?;
            Ok::<_, failure::Error>(())
        }
    });

        let handle = Arc::new(tokio::runtime::Handle::current());
        let seen_repos = Arc::new(Mutex::new(HashSet::new()));
        let concurrency = Arc::new(tokio::sync::Semaphore::new(16));
        let repo_concurrency = Arc::new(tokio::sync::Semaphore::new(4));
        let _ = pre.await;
        let waiting = futures::stream::FuturesUnordered::new();
        let c = if everything {
            let mut c: Vec<_> = crates.all_crates().collect::<Vec<_>>();
            c.shuffle(&mut thread_rng());
            Either::Left(c)
        } else if !specific.is_empty() {
            Either::Left(specific)
        } else {
            Either::Right(handle.enter(|| futures::executor::block_on(crates.crates_to_reindex())).unwrap().into_iter().map(|c| c.origin().clone()))
        };
        for (i, origin) in c.into_iter().enumerate() {
            if stopped() {
                return;
            }
            let crates = Arc::clone(&crates);
            let handle = Arc::clone(&handle);
            let concurrency = Arc::clone(&concurrency);
            let repo_concurrency = Arc::clone(&repo_concurrency);
            let renderer = Arc::clone(&renderer);
            let seen_repos = Arc::clone(&seen_repos);
            let mut tx = tx.clone();
            waiting.push(handle.clone().spawn(async move {
                let index_finished = concurrency.acquire().await;
                if stopped() {return;}
                println!("{}…", i);
                match crates.index_crate_highest_version(&origin).await {
                    Ok(()) => {},
                    err => {
                        print_res(err);
                        return;
                    },
                }
                if stopped() {return;}
                match index_crate(&crates, &origin, &renderer, &mut tx).await {
                    Ok(v) => {
                        drop(index_finished);
                        if repos {
                            if let Some(ref repo) = v.repository() {
                                {
                                    let mut s = seen_repos.lock();
                                    let url = repo.canonical_git_url().to_string();
                                    if s.contains(&url) {
                                        return;
                                    }
                                    println!("Indexing {}", url);
                                    s.insert(url);
                                }
                                let _finished = repo_concurrency.acquire().await;
                                if stopped() {return;}
                                print_res(crates.index_repo(repo, v.version()).await);
                            }
                        }
                    },
                    err => print_res(err),
                }
            }).map(drop));
        }
        drop(tx);
        if stopped() {return;}
        waiting.collect::<()>().await;
        if stopped() {return;}
        index_thread.await.unwrap().unwrap();
    }).await.unwrap();
}

async fn index_crate(crates: &KitchenSink, origin: &Origin, renderer: &Renderer, search_sender: &mut mpsc::Sender<(ArcRichCrateVersion, usize, f64)>) -> Result<ArcRichCrateVersion, failure::Error> {
    let (k, v) = futures::try_join!(crates.rich_crate_async(origin), crates.rich_crate_version_async(origin))?;

    let (downloads_per_month, score) = crate_overall_score(crates, &k, &v, renderer).await;
    let (_, index_res) = futures::join!(
        async {
            search_sender.send((v.clone(), downloads_per_month, score)).await
                .map_err(|e| {stop();e}).expect("closed channel?");
        },
        crates.index_crate(&k, score)
    );
    index_res?;
    Ok(v)
}

fn index_search(indexer: &mut Indexer, renderer: &Renderer, k: &RichCrateVersion, downloads_per_month: usize, score: f64) -> Result<(), failure::Error> {
    let keywords: Vec<_> = k.keywords().iter().map(|s| s.as_str()).collect();

    let mut lib_tmp = None;
    let readme = k.readme().map(|readme| &readme.markup).or_else(|| {
        lib_tmp = k.lib_file_markdown();
        lib_tmp.as_ref()
    }).map(|markup| {
        renderer.visible_text(markup)
    });
    let version = k.version();

    indexer.add(k.origin(), k.short_name(), version, k.description().unwrap_or(""), &keywords, readme.as_ref().map(|s| s.as_str()), downloads_per_month as u64, score);
    Ok(())
}

async fn crate_overall_score(crates: &KitchenSink, all: &RichCrate, k: &RichCrateVersion, renderer: &Renderer) -> (usize, f64) {
    let contrib_info = crates.all_contributors(&k).await.map_err(|e| eprintln!("{}", e)).ok();
    let contributors_count = if let Some((authors, _owner_only, _, extra_contributors)) = &contrib_info {
        (authors.len() + extra_contributors) as u32
    } else {
        all.owners().len() as u32
    };

    let langs = k.language_stats();
    let (rust_code_lines, rust_comment_lines) = langs.langs.get(&udedokei::Language::Rust).map(|rs| (rs.code, rs.comments)).unwrap_or_default();
    let total_code_lines = langs.langs.iter().filter(|(k, _)| k.is_code()).map(|(_, l)| l.code).sum::<u32>();
    let base_score = ranking::crate_score_version(&CrateVersionInputs {
        versions: all.versions(),
        description: k.description().unwrap_or(""),
        readme: k.readme().map(|readme| {
            renderer.page_node(&readme.markup, None, Links::Ugc, None)
        }).as_ref(),
        owners: all.owners(),
        authors: k.authors(),
        contributors: Some(contributors_count),
        edition: k.edition(),
        total_code_lines,
        rust_code_lines,
        rust_comment_lines,
        is_app: k.is_app(),
        has_build_rs: k.has_buildrs(),
        has_links: k.links().is_some(),
        has_documentation_link: k.documentation().is_some(),
        has_homepage_link: k.homepage().is_some(),
        has_repository_link: k.repository().is_some(),
        has_keywords: k.has_own_keywords(),
        has_categories: k.has_own_categories(),
        has_features: !k.features().is_empty(),
        has_examples: k.has_examples(),
        has_code_of_conduct: k.has_code_of_conduct(),
        has_benches: k.has_benches(),
        has_tests: k.has_tests(),
        // has_lockfile: k.has_lockfile(),
        // has_changelog: k.has_changelog(),
        license: k.license().unwrap_or(""),
        has_badges: k.has_badges(),
        maintenance: k.maintenance(),
        is_nightly: k.is_nightly(),
    })
    .total();

    let downloads_per_month = crates.downloads_per_month_or_equivalent(all.origin()).await.expect("dl numbers").unwrap_or(0) as u32;
    let dependency_freshness = if let Ok((runtime, _, build)) = k.direct_dependencies() {
        // outdated dev deps don't matter
        join_all(runtime.iter().chain(&build).filter_map(|richdep| {
            if !richdep.dep.is_crates_io() {
                return None;
            }
            let req = richdep.dep.req().parse().ok()?;
            Some(async move {
                (richdep.is_optional(), crates.version_popularity(&richdep.package, &req).await.expect("ver1pop").expect("ver2pop"))
            })
        }))
        .await
        .into_iter()
        .map(|(is_optional, (is_latest, popularity))| {
            if is_latest {1.0} // don't penalize pioneers
            else if is_optional {0.8 + popularity * 0.2} // not a big deal when it's off
            else {popularity}
        })
        .collect()
    } else {
        Vec::new()
    };

    let mut temp_inp = CrateTemporalInputs {
        versions: all.versions(),
        is_app: k.is_app(),
        has_docs_rs: crates.has_docs_rs(k.origin(), k.short_name(), k.version()).await,
        is_nightly: k.is_nightly(),
        downloads_per_month,
        downloads_per_month_minus_most_downloaded_user: downloads_per_month,
        number_of_direct_reverse_deps: 0,
        number_of_indirect_reverse_deps: 0,
        number_of_indirect_reverse_optional_deps: 0,
        dependency_freshness,
    };

    if let Some(deps) = crates.crates_io_dependents_stats_of(k.origin()).await.expect("depsstats") {
        let direct_rev_deps = deps.direct.all();
        let indirect_reverse_optional_deps = (deps.runtime.def as u32 + deps.runtime.opt as u32)
            .max(deps.dev as u32)
            .max(deps.build.def as u32 + deps.build.opt as u32);

        temp_inp.number_of_direct_reverse_deps = direct_rev_deps;
        temp_inp.number_of_indirect_reverse_deps = deps.runtime.def.max(deps.build.def).into();
        temp_inp.number_of_indirect_reverse_optional_deps = indirect_reverse_optional_deps;
        let tmp = futures::future::join_all(deps.rev_dep_names.iter()
            .filter_map(|name| Origin::try_from_crates_io_name(name))
            .map(|o| async move {
                crates.downloads_per_month(&o).await
            })).await;
        let biggest = tmp.into_iter().filter_map(|x| x.ok().and_then(|x| x)).max().unwrap_or(0);
        temp_inp.downloads_per_month_minus_most_downloaded_user = downloads_per_month.saturating_sub(biggest as u32);
    }

    let temp_score = ranking::crate_score_temporal(&temp_inp);
    let temp_score = temp_score.total();

    let mut score = (base_score + temp_score) * 0.5;
    if let Some((former_glory, _)) = crates.former_glory(all.origin()).await.expect("former_glory") {
        score *= former_glory;
    }

    // there's usually a non-macro/non-sys sibling
    if k.is_proc_macro() || k.is_sys() {
        score *= 0.9;
    }
    if is_sub_component(crates, k).await {
        score *= 0.9;
    }

    if is_autopublished(&k) {
        score *= 0.8;
    }

    if is_deprecated(&k) {
        score *= 0.2;
    }

    match k.origin() {
        Origin::CratesIo(_) => {},
        // installation and usage of other crate sources is more limited
        _ => score *= 0.75,
    }

    // k bye
    if k.is_yanked() || is_squatspam(&k) {
        score *= 0.001;
    }

    (downloads_per_month as usize, score)
}

/// Crates are spilt into foo and foo-core. The core is usually uninteresting/duplicate.
async fn is_sub_component(crates: &KitchenSink, k: &RichCrateVersion) -> bool {
    let name = k.short_name();
    if let Some(pos) = name.rfind(|c: char| c == '-' || c == '_') {
        match name.get(pos+1..) {
            Some("core") | Some("shared") | Some("utils") | Some("common") |
            Some("fork") | Some("unofficial") => {
                if let Some(parent_name) = name.get(..pos-1) {
                    if Origin::try_from_crates_io_name(parent_name).map_or(false, |name| crates.crate_exists(&name)) {
                        // TODO: check if owners overlap?
                        return true;
                    }
                }
                if crates.parent_crate(k).await.is_some() {
                    return true;
                }
            },
            _ => {},
        }
    }
    false
}

fn is_autopublished(k: &RichCrateVersion) -> bool {
    k.description().map_or(false, |d| d.starts_with("Automatically published "))
}

fn is_squatspam(k: &RichCrateVersion) -> bool {
    if k.version().contains("reserved") || k.version().contains("placeholder") {
        return true;
    }
    if let Some(desc) = k.description() {
        let desc = desc.trim_matches(|c: char| !c.is_ascii_alphabetic()).to_ascii_lowercase();
        return desc.contains("this crate is a placeholder") ||
            desc.contains("reserving this crate") ||
            desc.contains("want to use this name") ||
            desc.contains("this is a dummy package") ||
            desc == "reserved" ||
            desc.starts_with("placeholder") ||
            desc.ends_with(" placeholder") ||
            desc.starts_with("a placeholder") ||
            desc.starts_with("reserved for ") ||
            desc.starts_with("empty crate") ||
            desc.starts_with("stub to squat") ||
            desc.starts_with("claiming it before someone") ||
            desc.starts_with("reserved name") ||
            desc.starts_with("reserved package") ||
            desc.starts_with("an empty crate") ||
            desc.starts_with("Empty crate,") ||
            desc.starts_with("reserve the name");
    }
    false
}

fn is_deprecated(k: &RichCrateVersion) -> bool {
    if k.version().contains("deprecated") || k.version() == "0.0.0" || k.version() == "0.0.1" {
        return true;
    }
    if k.maintenance() == MaintenanceStatus::Deprecated {
        return true;
    }
    if let Some(orig_desc) = k.description() {
        let orig_desc = orig_desc.trim_matches(|c: char| !c.is_ascii_alphabetic());
        let desc = orig_desc.to_ascii_lowercase();
        return orig_desc.starts_with("WIP") || orig_desc.ends_with("WIP") ||
            desc.starts_with("deprecated") ||
            desc.starts_with("unfinished") ||
            desc.starts_with("an unfinished") ||
            desc.starts_with("unsafe and deprecated") ||
            desc.starts_with("crate is abandoned") ||
            desc.starts_with("abandoned") ||
            desc.contains("this crate is abandoned") ||
            desc.contains("this crate has been abandoned") ||
            desc.contains("do not use") ||
            desc.contains("this crate is a placeholder") ||
            desc.contains("this is a dummy package") ||
            desc.starts_with("an empty crate") ||
            desc.starts_with("discontinued") ||
            desc.starts_with("wip. ") ||
            desc.starts_with("very early wip") ||
            desc.starts_with("renamed to ") ||
            desc.starts_with("crate renamed to ") ||
            desc.starts_with("temporary fork") ||
            desc.contains("no longer maintained") ||
            desc.contains("this tool is abandoned") ||
            desc.ends_with("deprecated") || desc.contains("deprecated in favor") || desc.contains("project is deprecated");
    }
    if let Ok(req) = k.version().parse() {
        if kitchen_sink::is_deprecated(k.short_name(), &req) {
            return true;
        }
    }
    false
}

fn print_res<T>(res: Result<T, failure::Error>) {
    if let Err(e) = res {
        let s = e.to_string();
        if s.starts_with("Too many open files") {
            stop();
            panic!(s);
        }
        eprintln!("••• Error: {}", s);
        for c in e.iter_chain().skip(1) {
            let s = c.to_string();
            eprintln!("•   error: -- {}", s);
            if s.starts_with("Too many open files") {
                stop();
                panic!(s);
            }
        }
    }
}
