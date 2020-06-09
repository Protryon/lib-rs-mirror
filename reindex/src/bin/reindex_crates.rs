use std::convert::TryInto;
use simple_cache::TempCache;
use futures::Future;
use failure;
use futures::future::FutureExt;
use futures::future::join_all;
use futures::stream::StreamExt;
use kitchen_sink::ArcRichCrateVersion;
use kitchen_sink::RichCrate;
use kitchen_sink::{self, stop, stopped, KitchenSink, Origin, RichCrateVersion};
use parking_lot::Mutex;
use rand::{seq::SliceRandom, thread_rng};
use ranking::CrateTemporalInputs;
use ranking::CrateVersionInputs;
use ranking::OverallScoreInputs;
use render_readme::Links;
use render_readme::Renderer;
use search_index::*;
use std::collections::HashSet;
use std::time::Duration;
use tokio::sync::mpsc;
use triomphe::Arc;
use udedokei::LanguageExt;
use feat_extractor::*;

fn main() {
    let everything = std::env::args().nth(1).map_or(false, |a| a == "--all");
    let specific: Vec<_> = if !everything {
        std::env::args().skip(1).map(|name| {
            Origin::try_from_crates_io_name(&name).unwrap_or_else(|| Origin::from_str(name))
        }).collect()
    } else {
        Vec::new()
    };
    let repos = !everything;

    let mut rt = tokio::runtime::Builder::new()
        .threaded_scheduler()
        .enable_all()
        .thread_name("reindex")
        .build()
        .unwrap();

    let crates = rt.block_on(kitchen_sink::KitchenSink::new_default());
    let crates = Arc::new(crates.unwrap());
    let mut indexer = Indexer::new(CrateSearchIndex::new(crates.main_cache_dir()).expect("init search")).expect("init search indexer");
    let lines = TempCache::new(crates.main_cache_dir().join("search-uniq-lines.dat")).expect("init lines cache");
    let (tx, mut rx) = mpsc::channel::<(Arc<_>, _, _)>(64);

    let index_thread = rt.spawn({
        async move {
            let renderer = Arc::new(Renderer::new(None));
            let mut n = 0usize;
            let mut next_n = 100usize;
            while let Some((ver, downloads_per_month, score)) = rx.recv().await {
                if stopped() {break;}
                tokio::task::block_in_place(|| {
                    index_search(&mut indexer, &lines, &renderer, &ver, downloads_per_month, score)?;
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

    let c: Box<dyn Iterator<Item=Origin> + Send> = if everything {
        let mut c: Vec<_> = crates.all_crates().collect::<Vec<_>>();
        c.shuffle(&mut thread_rng());
        Box::new(c.into_iter())
    } else if !specific.is_empty() {
        Box::new(specific.into_iter())
    } else {
        Box::new(rt.block_on(crates.crates_to_reindex()).unwrap().into_iter().map(|c| c.origin().clone()))
    };

    rt.block_on(rt.spawn(async move {
        main_indexing_loop(crates, c, tx, repos).await;
        if stopped() {return;}
        index_thread.await.unwrap().unwrap();
    })).unwrap();
}

async fn main_indexing_loop(crates: Arc<KitchenSink>, c: Box<dyn Iterator<Item=Origin> + Send>, tx: mpsc::Sender<(Arc<RichCrateVersion>, usize, f64)>, repos: bool) {
    let renderer = Arc::new(Renderer::new(None));
    let waiting = futures::stream::FuturesUnordered::new();
    let handle = Arc::new(tokio::runtime::Handle::current());
    let seen_repos = Arc::new(Mutex::new(HashSet::new()));
    let concurrency = Arc::new(tokio::sync::Semaphore::new(16));
    let repo_concurrency = Arc::new(tokio::sync::Semaphore::new(4));
    for (i, origin) in c.enumerate() {
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
            match run_timeout(62, crates.index_crate_highest_version(&origin)).await {
                Ok(()) => {},
                err => {
                    print_res(err);
                    return;
                },
            }
            if stopped() {return;}
            match run_timeout(70, index_crate(&crates, &origin, &renderer, &mut tx)).await {
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
                            print_res(run_timeout(600, crates.index_repo(repo, v.version())).await);
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
}

async fn index_crate(crates: &KitchenSink, origin: &Origin, renderer: &Renderer, search_sender: &mut mpsc::Sender<(ArcRichCrateVersion, usize, f64)>) -> Result<ArcRichCrateVersion, failure::Error> {
    let (k, v) = futures::try_join!(crates.rich_crate_async(origin), run_timeout(45, crates.rich_crate_version_async(origin)))?;

    let (downloads_per_month, score) = crate_overall_score(crates, &k, &v, renderer).await;
    let (_, index_res) = futures::join!(
        async {
            search_sender.send((v.clone(), downloads_per_month, score)).await
                .map_err(|e| {stop();e}).expect("closed channel?");
        },
        run_timeout(60, crates.index_crate(&k, score))
    );
    index_res?;
    Ok(v)
}

fn index_search(indexer: &mut Indexer, lines: &TempCache<(String, f64), [u8; 16]>, renderer: &Renderer, k: &RichCrateVersion, downloads_per_month: usize, score: f64) -> Result<(), failure::Error> {
    let keywords: Vec<_> = k.keywords().iter().map(|s| s.as_str()).collect();
    let version = k.version();

    let mut variables = Vec::with_capacity(15);
    variables.push(k.short_name());
    if let Some(r) = k.repository() {
        if let Some(name) = r.repo_name() {
            variables.push(name);
        }
        variables.push(r.url.as_str());
    }
    for name in k.authors().iter().filter_map(|a| a.name.as_deref()) {
        variables.push(name);
    }
    variables.sort_by_key(|a| !a.len()); // longest first

    let mut dupe_sentences = HashSet::new();
    let mut unique_text = String::with_capacity(4000);
    let mut cb = |key: &str, orig: &str| {
        let key: [u8; 16] = blake3::hash(key.as_bytes()).as_bytes()[..16].try_into().unwrap();
        if dupe_sentences.insert(key) {
            let (matches, wins) = if let Ok(Some((their_crate_name, their_score))) = lines.get(&key) {
                (k.short_name() == their_crate_name, score > their_score)
            } else {
                (true, true)
            };
            if wins {
                lines.set(key, (k.short_name().to_string(), score)).expect("upd lines");
            }
            if matches || wins {
                unique_text.push_str(orig);
                unique_text.push('\n');
            }
        }
    };

    let mut dedup = wlita::WLITA::new(variables.iter(), &mut cb);
    if let Some(markup) = k.readme().map(|readme| &readme.markup) {
        dedup.add_text(&renderer.visible_text(markup));
    }
    if let Some(markup) = k.lib_file_markdown() {
        dedup.add_text(&renderer.visible_text(&markup));
    }

    indexer.add(k.origin(), k.short_name(), version, k.description().unwrap_or(""), &keywords, Some(unique_text.as_str()).filter(|s| !s.trim_start().is_empty()), downloads_per_month as u64, score);
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
    });

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

    let score = ranking::combined_score(base_score, temp_score, &OverallScoreInputs {
        former_glory: crates.former_glory(all.origin()).await.expect("former_glory").map(|(f,_)| f),
        is_proc_macro: k.is_proc_macro(),
        is_sys: k.is_sys(),
        is_sub_component: crates.is_sub_component(k).await,
        is_autopublished: is_autopublished(&k),
        is_deprecated: is_deprecated(&k),
        is_crates_io_published: if let Origin::CratesIo(_) = k.origin() {true} else {false},
        is_yanked: k.is_yanked(),
        is_squatspam: is_squatspam(&k),
    });

    (downloads_per_month as usize, score)
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

fn run_timeout<'a, T>(secs: u64, fut: impl Future<Output=Result<T, failure::Error>> + 'a) -> impl Future<Output=Result<T, failure::Error>> + 'a {
    tokio::time::timeout(Duration::from_secs(secs), fut).map(move |r| r.map_err(|_| failure::format_err!("timed out {}", secs)).and_then(|x| x))
}
