use either::*;
use failure;
use kitchen_sink::RichCrate;
use kitchen_sink::{self, stopped, CrateData, Include, KitchenSink, MaintenanceStatus, Origin, RichCrateVersion};
use parking_lot::Mutex;
use rand::{seq::SliceRandom, thread_rng};
use ranking::CrateTemporalInputs;
use ranking::CrateVersionInputs;
use rayon;
use render_readme::Renderer;
use search_index::*;
use std::collections::HashSet;
use std::sync::mpsc;
use std::sync::Arc;
use udedokei::LanguageExt;

fn main() {
    let crates = Arc::new(match kitchen_sink::KitchenSink::new_default() {
        Ok(a) => a,
        e => {
            print_res(e);
            std::process::exit(1);
        },
    });
    let renderer = Arc::new(Renderer::new(None));

    let everything = std::env::args().nth(1).map_or(false, |a| a == "--all");
    let repos = !everything;

    let mut indexer = Indexer::new(CrateSearchIndex::new(crates.main_cache_dir()).expect("init search")).expect("init search indexer");

    let (tx, rx) = mpsc::sync_channel(64);
    let index_thread = std::thread::spawn({
        let renderer = renderer.clone();
        move || -> Result<(), failure::Error> {
            let mut n = 0;
            let mut next_n = 100;
            while let Ok((ver, downloads_per_month, score)) = rx.recv() {
                if stopped() {break;}
                index_search(&mut indexer, &renderer, &ver, downloads_per_month, score)?;
                n += 1;
                if n == next_n {
                    next_n *= 2;
                    println!("savepoint…");
                    indexer.commit()?;
                }
            }
            indexer.commit()?;
            let _ = indexer.bye()?;
            Ok(())
        }
    });

    let seen_repos = &Mutex::new(HashSet::new());
    rayon::scope(move |s1| {
        let c = if everything {
            let mut c: Vec<_> = crates.all_crates().cloned().collect::<Vec<_>>();
            c.shuffle(&mut thread_rng());
            Either::Left(c)
        } else {
            Either::Right(crates.all_new_crates().unwrap().map(|c| c.origin().clone()))
        };
        for (i, k) in c.into_iter().enumerate() {
            if stopped() {
                return;
            }
            let crates = Arc::clone(&crates);
            let renderer = Arc::clone(&renderer);
            let tx = tx.clone();
            s1.spawn(move |s2| {
                if stopped() {
                    return;
                }
                print!("{} ", i);
                match index_crate(&crates, &k, &renderer, &tx) {
                    Ok(v) => {
                        if repos {
                            s2.spawn(move |_| {
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
                                    print_res(crates.index_repo(repo, v.version()));
                                }
                            })
                        }
                    },
                    err => print_res(err),
                }
            });
        }
        drop(tx);
    });

    index_thread.join().unwrap().unwrap();
}

fn index_crate(crates: &KitchenSink, c: &Origin, renderer: &Renderer, search_sender: &mpsc::SyncSender<(RichCrateVersion, usize, f64)>) -> Result<RichCrateVersion, failure::Error> {
    let v = crates.rich_crate_version(c, CrateData::FullNoDerived)?;
    let k = crates.rich_crate(c)?;
    let contrib_info = crates.all_contributors(&v).map_err(|e| eprintln!("{}", e)).ok();
    let contributors_count = if let Some((authors, _owner_only, _, extra_contributors)) = &contrib_info {
        (authors.len() + extra_contributors) as u32
    } else {
        k.owners().len() as u32
    };

    let (downloads_per_month, score) = crate_overall_score(crates, &k, &v, renderer, contributors_count);
    crates.index_crate_highest_version(&v, score)?;
    crates.index_crate(&k, score)?;
    search_sender.send((v.clone(), downloads_per_month, score))?;
    Ok(v)
}

fn index_search(indexer: &mut Indexer, renderer: &Renderer, k: &RichCrateVersion, downloads_per_month: usize, score: f64) -> Result<(), failure::Error> {
    let keywords: Vec<_> = k.keywords(Include::Cleaned).collect();

    let mut lib_tmp = None;
    let readme = k.readme().map(|readme| &readme.markup).or_else(|| {
        lib_tmp = k.lib_file_markdown();
        lib_tmp.as_ref()
    }).map(|markup| {
        renderer.visible_text(markup)
    });
    let version = k.version();

    indexer.add(k.short_name(), version, k.description().unwrap_or(""), &keywords, readme.as_ref().map(|s| s.as_str()), downloads_per_month as u64, score);
    Ok(())
}



fn crate_overall_score(crates: &KitchenSink, all: &RichCrate, k: &RichCrateVersion, renderer: &Renderer, contributors_count: u32) -> (usize, f64) {
    let readme = k.readme().map(|readme| {
        renderer.page_node(&readme.markup, None, false, None)
    });
    let langs = k.language_stats();
    let (rust_code_lines, rust_comment_lines) = langs.langs.get(&udedokei::Language::Rust).map(|rs| (rs.code, rs.comments)).unwrap_or_default();
    let total_code_lines = langs.langs.iter().filter(|(k,_)| k.is_code()).map(|(_,l)| l.code).sum::<u32>();
    let base_score = ranking::crate_score_version(&CrateVersionInputs {
        versions: all.versions(),
        description: k.description().unwrap_or(""),
        readme: readme.as_ref(),
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

    let downloads_per_month = crates.downloads_per_month_or_equivalent(all.origin()).expect("dl numbers").unwrap_or(0) as u32;
    let dependency_freshness = if let Ok((runtime, _, build)) = k.direct_dependencies() {
        // outdated dev deps don't matter
        runtime.iter().chain(&build).filter_map(|richdep| {
            richdep.dep.req().parse().ok().map(|req| (richdep.is_optional(), crates.version_popularity(&richdep.package, &req).expect("verpop")))
        })
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
        has_docs_rs: crates.has_docs_rs(k.short_name(), k.version()),
        is_nightly: k.is_nightly(),
        downloads_per_month,
        downloads_per_month_minus_most_downloaded_user: downloads_per_month,
        number_of_direct_reverse_deps: 0,
        number_of_indirect_reverse_deps: 0,
        number_of_indirect_reverse_optional_deps: 0,
        dependency_freshness,
    };

    let mut direct_rev_deps = 0;
    let mut indirect_reverse_optional_deps = 0;
    if let Some(deps) = crates.dependents_stats_of_crates_io_crate(k.short_name()) {
        direct_rev_deps = deps.direct as u32;
        indirect_reverse_optional_deps = (deps.runtime.def as u32 + deps.runtime.opt as u32)
            .max(deps.dev as u32)
            .max(deps.build.def as u32 + deps.build.opt as u32);

        temp_inp.number_of_direct_reverse_deps = direct_rev_deps;
        temp_inp.number_of_indirect_reverse_deps = deps.runtime.def.max(deps.build.def).into();
        temp_inp.number_of_indirect_reverse_optional_deps = indirect_reverse_optional_deps;
        let biggest = deps.rev_dep_names.iter()
            .filter_map(|name| crates.downloads_per_month(&Origin::from_crates_io_name(name)).ok().and_then(|x| x))
            .max().unwrap_or(0);
        temp_inp.downloads_per_month_minus_most_downloaded_user = downloads_per_month.saturating_sub(biggest as u32);
    }

    let removals_divisor = if let Some(removals_weighed) = crates.crate_removals(k.origin()) {
        // count some indirect/optional deps in case removals have been due to moving the crate behind another facade
        // +20 is a fudge factor to smooth out nosiy data for rarely used crates.
        // We don't care about small amount of removals, only mass exodus from big dead crates.
        let effective_rev_deps = 20. + (direct_rev_deps as f64).max(indirect_reverse_optional_deps as f64 / 5.);
        let removals_ratio = removals_weighed / (effective_rev_deps * 3.);
        // if it's used more than removed, ratio < 1 is fine.
        removals_ratio.max(1.).min(3.)
    } else {
        1.
    };

    let temp_score = ranking::crate_score_temporal(&temp_inp);
    let temp_score = temp_score.total();

    let mut score = (base_score + temp_score) * 0.5 / removals_divisor;

    // there's usually a non-macro/non-sys sibling
    if k.is_proc_macro() || k.is_sys() {
        score *= 0.9;
    }
    if is_sub_component(crates, k) {
        score *= 0.9;
    }

    if is_autopublished(&k) {
        score *= 0.8;
    }

    if is_deprecated(&k) {
        score *= 0.2;
    }

    // k bye
    if k.is_yanked() {
        score *= 0.001;
    }

    (downloads_per_month as usize, score)
}

/// Crates are spilt into foo and foo-core. The core is usually uninteresting/duplicate.
fn is_sub_component(crates: &KitchenSink, k: &RichCrateVersion) -> bool {
    let name = k.short_name();
    if let Some(pos) = name.rfind(|c: char| c == '-' || c == '_') {
        match name.get(pos+1..) {
            Some("core") | Some("shared") | Some("utils") |
            Some("fork") | Some("unofficial") => {
                if let Some(parent_name) = name.get(..pos-1) {
                    if crates.crate_exists(&Origin::from_crates_io_name(parent_name)) {
                        // TODO: check if owners overlap?
                        return true;
                    }
                }
                if crates.parent_crate(k).is_some() {
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

fn is_deprecated(k: &RichCrateVersion) -> bool {
    if k.version().contains("deprecated") || kitchen_sink::is_deprecated(k.short_name()) {
        return true;
    }
    if k.maintenance() == MaintenanceStatus::Deprecated {
        return true;
    }
    if let Some(desc) = k.description() {
        let desc = desc.trim_matches(|c: char| !c.is_ascii_alphabetic()).to_ascii_lowercase();
        return desc.starts_with("deprecated") || desc.starts_with("unsafe and deprecated") ||
            desc.starts_with("crate is abandoned") ||
            desc.contains("this crate is abandoned") ||
            desc.contains("this crate has been abandoned") ||
            desc.contains("do not use") ||
            desc.contains("this crate is a placeholder") ||
            desc.contains("this is a dummy package") ||
            desc == "reserved" ||
            desc.starts_with("reserved for ") ||
            desc.starts_with("reserved name") ||
            desc.starts_with("discontinued") ||
            desc.starts_with("renamed to ") ||
            desc.starts_with("crate renamed to ") ||
            desc.starts_with("temporary fork") ||
            desc.contains("no longer maintained") ||
            desc.contains("this tool is abandoned") ||
            desc.ends_with("deprecated") || desc.contains("deprecated in favor") || desc.contains("project is deprecated");
    }
    false
}

fn print_res<T>(res: Result<T, failure::Error>) {
    if let Err(e) = res {
        eprintln!("••• Error: {}", e);
        for c in e.iter_chain().skip(1) {
            eprintln!("•   error: -- {}", c);
        }
    }
}
