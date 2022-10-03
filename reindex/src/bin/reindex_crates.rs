use ahash::HashSetExt;
use kitchen_sink::CrateOwners;
use kitchen_sink::SemVer;
use anyhow::anyhow;
use debcargo_list::DebcargoList;
use feat_extractor::*;
use futures::future::join_all;
use futures::Future;
use futures::stream::StreamExt;
use kitchen_sink::ArcRichCrateVersion;
use kitchen_sink::RichCrate;
use kitchen_sink::VersionPopularity;
use kitchen_sink::{self, stop, stopped, KitchenSink, Origin, RichCrateVersion};
use log::debug;
use parking_lot::Mutex;
use rand::{seq::SliceRandom, thread_rng};
use ranking::CrateTemporalInputs;
use ranking::CrateVersionInputs;
use ranking::OverallScoreInputs;
use render_readme::Links;
use render_readme::Renderer;
use search_index::*;
use simple_cache::TempCache;
use ahash::HashSet;
use std::convert::TryInto;
use std::time::Duration;
use tokio::sync::mpsc;
use triomphe::Arc;
use udedokei::LanguageExt;

struct Reindexer {
    crates: KitchenSink,
    deblist: DebcargoList,
}

fn main() {
    let everything = std::env::args().nth(1).map_or(false, |a| a == "--all");
    let search_only = std::env::args().nth(1).map_or(false, |a| a == "--search");
    let specific: Vec<_> = if !everything && !search_only {
        std::env::args().skip(1).map(|name| {
            Origin::try_from_crates_io_name(&name).unwrap_or_else(|| Origin::from_str(name))
        }).collect()
    } else {
        Vec::new()
    };
    let repos = !everything && !search_only;

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("reindex")
        .build()
        .unwrap();


    let crates = rt.block_on(kitchen_sink::KitchenSink::new_default()).unwrap();
    let deblist = DebcargoList::new(crates.main_cache_dir()).expect("deblist");
    if everything {
        deblist.update().expect("debcargo"); // it needs to be updated sometime, but not frequently
    }
    let r = Arc::new(Reindexer { crates, deblist });
    let mut indexer = Indexer::new(CrateSearchIndex::new(r.crates.main_cache_dir()).expect("init search")).expect("init search indexer");
    let lines = TempCache::new(r.crates.main_cache_dir().join("search-uniq-lines.dat"), Duration::ZERO).expect("init lines cache");
    let (tx, mut rx) = mpsc::channel::<(Arc<_>, _, _)>(64);

    let r2 = r.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(1000));
        if stopped() {
            eprintln!("Flushing caches");
            r2.crates.cleanup();
            return;
        }
    });

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
                    Ok::<_, anyhow::Error>(())
                })?;
            }
            tokio::task::block_in_place(|| indexer.commit())?;
            let _ = indexer.bye()?;
            Ok::<_, anyhow::Error>(())
        }
    });

    rt.block_on(r.crates.prewarm());

    let c: Box<dyn Iterator<Item = Origin> + Send> = if everything || search_only {
        let mut c: Vec<_> = r.crates.all_crates().collect::<Vec<_>>();
        c.shuffle(&mut thread_rng());
        Box::new(c.into_iter())
    } else if !specific.is_empty() {
        Box::new(specific.into_iter())
    } else {
        Box::new(rt.block_on(r.crates.crates_to_reindex()).unwrap().into_iter().map(|c| c.origin().clone()))
    };

    rt.block_on(rt.spawn(async move {
        main_indexing_loop(r, c, tx, repos, search_only, everything).await;
        if stopped() {return;}
        index_thread.await.unwrap().unwrap();
    })).unwrap();
}

async fn main_indexing_loop(ref r: Arc<Reindexer>, crate_origins: Box<dyn Iterator<Item=Origin> + Send>, tx: mpsc::Sender<(Arc<RichCrateVersion>, usize, f64)>, repos: bool, search_only: bool, reindexing_all_crates: bool) {
    let renderer = &Renderer::new(None);
    let seen_repos = &Mutex::new(HashSet::new());
    let repo_concurrency = &tokio::sync::Semaphore::new(4);
    futures::stream::iter(crate_origins.enumerate()).map(move |(i, origin)| {
        let mut tx = tx.clone();
        async move {
        if stopped() {return;}
        println!("{}…", i);
        if !search_only {
            match run_timeout("hv", 62, r.crates.index_crate_highest_version(&origin, reindexing_all_crates)).await {
                Ok(()) => {},
                err => {
                    print_res(err);
                    if reindexing_all_crates { return; }
                    // on hourly updates, try to continue and update score, search, etc.
                },
            }
        }
        if stopped() {return;}
        match run_timeout("rc", 70, r.index_crate(&origin, renderer, &mut tx)).await {
            Ok(v) => {
                if repos {
                    if let Some(repo) = v.repository() {
                        {
                            let mut s = seen_repos.lock();
                            let url = repo.canonical_git_url();
                            if s.contains(&*url) {
                                return;
                            }
                            println!("Indexing repo of {}: {}", v.short_name(), url);
                            s.insert(url.to_string());
                        }
                        let _finished = repo_concurrency.acquire().await;
                        if stopped() {return;}
                        print_res(run_timeout("rep", 500, r.crates.index_repo(repo, v.version())).await);
                    }
                }
            },
            err => print_res(err),
        }
    }})
    .buffer_unordered(16)
    .collect::<()>().await;
}

impl Reindexer {
async fn index_crate(&self, origin: &Origin, renderer: &Renderer, search_sender: &mut mpsc::Sender<(ArcRichCrateVersion, usize, f64)>) -> Result<ArcRichCrateVersion, anyhow::Error> {
    let crates = &self.crates;
    let (k, v) = futures::try_join!(
        run_timeout("rca", 31, crates.rich_crate_async(origin)),
        run_timeout("rcv", 45, crates.rich_crate_version_async(origin)),
    )?;

    let (downloads_per_month, score) = run_timeout("score", 41, self.crate_overall_score(&k, &v, renderer)).await?;
    debug!("{origin:?} has score {score} and {downloads_per_month}dl/mo");
    let (_, index_res) = futures::join!(
        run_timeout("ssend", 10, async {
            search_sender.send((v.clone(), downloads_per_month, score)).await
                .map_err(|e| {stop();e})
        }),
        run_timeout("ic", 50, crates.index_crate(&k, score))
    );
    index_res?;
    Ok(v)
}
}

fn index_search(indexer: &mut Indexer, lines: &TempCache<(String, f64), [u8; 16]>, renderer: &Renderer, k: &RichCrateVersion, downloads_per_month: usize, score: f64) -> Result<(), anyhow::Error> {
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
    variables.sort_unstable_by_key(|a| !a.len()); // longest first

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

    indexer.add(k.origin(), k.short_name(), version, k.description().unwrap_or(""), &keywords, Some(unique_text.as_str()).filter(|s| !s.trim_start().is_empty()), downloads_per_month as u64, score)?;
    Ok(())
}

impl Reindexer {
async fn crate_overall_score(&self, all: &RichCrate, k: &RichCrateVersion, renderer: &Renderer) -> Result<(usize, f64), anyhow::Error> {
    let crates = &self.crates;

    let (traction_stats, downloads_per_month, has_docs_rs, has_verified_repository_link, is_on_shitlist, deps_stats) = futures::join!(
        run_timeout("ts", 10, crates.traction_stats(all.origin())),
        run_timeout("dl", 10, crates.downloads_per_month_or_equivalent(all.origin())),
        crates.has_docs_rs(k.origin(), k.short_name(), k.version()),
        crates.has_verified_repository_link(k),
        crates.is_crate_on_shitlist(all),
        run_timeout("depstats", 20, crates.crates_io_dependents_stats_of(k.origin())),
    );
    let traction_stats = traction_stats?;
    let deps_stats = deps_stats?;
    let downloads_per_month = downloads_per_month?.unwrap_or(0) as u32;

    let advisories = crates.advisories_for_crate(k.origin());
    let semver: SemVer = k.version().parse()?;
    let is_unmaintained = advisories.iter()
        .filter(|a| !a.withdrawn() && a.versions.is_vulnerable(&semver))
        .filter_map(|a| a.metadata.informational.as_ref())
        .any(|info| info.is_unmaintained());

    let mut is_repo_archived = false;
    if let Some(repo) = k.repository() {
        if let Some(github_repo) = run_timeout("gh", 22, crates.github_repo(repo)).await.map_err(|e| log::error!("gh repo{}", e)).ok().and_then(|x| x) {
            is_repo_archived = github_repo.archived;
        }
    }

    let contrib_info = run_timeout("contr", 20, crates.all_contributors(k)).await.map_err(|e| log::error!("contrib {}", e)).ok();
    let owners = run_timeout("own", 20, crates.crate_owners(k.origin(), CrateOwners::All)).await?;
    let contributors_count = if let Some((authors, _owner_only, _, extra_contributors)) = &contrib_info {
        (authors.len() + extra_contributors) as u32
    } else {
        1
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
        owners: &owners,
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
        has_verified_repository_link,
        has_keywords: k.has_own_keywords(),
        has_own_categories: k.has_own_categories(),
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

    let (runtime, _, build) = k.direct_dependencies();
    let dependency_freshness = {
        // outdated dev deps don't matter
        join_all(runtime.iter().chain(&build).filter_map(|richdep| {
            if !richdep.dep.is_crates_io() {
                return None;
            }
            let req = richdep.dep.req().parse().ok()?;
            Some(async move {
                let pop = run_timeout("depf", 15, crates.version_popularity(&richdep.package, &req)).await
                    .map_err(|e| log::error!("ver1pop {}", e)).unwrap_or(None);
                (richdep.is_optional(), pop.unwrap_or(VersionPopularity { matches_latest: true, pop: 0., lost_popularity: false}))
            })
        }))
        .await
        .into_iter()
        .map(|(is_optional, pop)| {
            if pop.matches_latest {1.0} // don't penalize pioneers
            else if is_optional {0.8 + pop.pop * 0.2} // not a big deal when it's off
            else {pop.pop}
        })
        .collect()
    };

    let is_in_debian = self.deblist.has(k.short_name()).map_err(|e| log::error!("debcargo check: {}", e)).unwrap_or(false);

    let mut temp_inp = CrateTemporalInputs {
        traction_stats,
        versions: all.versions(),
        is_app: k.is_app(),
        has_docs_rs,
        is_nightly: k.is_nightly(),
        downloads_per_month,
        downloads_per_month_minus_most_downloaded_user: downloads_per_month,
        number_of_direct_reverse_deps: 0,
        number_of_indirect_reverse_deps: 0,
        number_of_indirect_reverse_optional_deps: 0,
        dependency_freshness,
        is_in_debian,
    };

    let mut has_many_direct_rev_deps = false;
    if let Some(deps) = deps_stats {
        let direct_rev_deps = deps.direct.all();
        if direct_rev_deps > 50 {
            has_many_direct_rev_deps = true;
        }
        let indirect_reverse_optional_deps = (deps.runtime.def as u32 + deps.runtime.opt as u32)
            .max(deps.dev as u32)
            .max(deps.build.def as u32 + deps.build.opt as u32);

        temp_inp.number_of_direct_reverse_deps = direct_rev_deps;
        temp_inp.number_of_indirect_reverse_deps = deps.runtime.def.max(deps.build.def).into();
        temp_inp.number_of_indirect_reverse_optional_deps = indirect_reverse_optional_deps;
        let tmp = futures::future::join_all(
            deps.rev_dep_names_default.iter().map(|n| (n, false)).chain(
            deps.rev_dep_names_optional.iter().map(|n| (n, true)))
            .filter_map(|(name, opt)| Some((Origin::try_from_crates_io_name(name)?, opt)))
            .map(|(o, opt)| async move {
                let dl = run_timeout("dlpm", 5, crates.downloads_per_month(&o)).await.unwrap_or_default().unwrap_or_default();
                if opt { dl / 2 } else { dl } // hyper-tls vs reqwest
            })).await;
        let biggest = tmp.into_iter().max().unwrap_or(0);
        temp_inp.downloads_per_month_minus_most_downloaded_user = downloads_per_month.saturating_sub(biggest as u32);
    }

    let temp_score = ranking::crate_score_temporal(&temp_inp);
    let overall = OverallScoreInputs {
        former_glory: traction_stats.map_or(1., |t| t.former_glory),
        is_proc_macro: k.is_proc_macro(),
        is_sys: k.is_sys(),
        is_sub_component: crates.is_sub_component(k).await,
        is_internal: crates.is_internal_crate(k) && !has_many_direct_rev_deps,
        is_autopublished: is_autopublished(k),
        is_deprecated: is_deprecated(k) || is_repo_archived || is_unmaintained,
        is_crates_io_published: k.origin().is_crates_io(),
        is_yanked: k.is_yanked(),
        is_squatspam: is_squatspam(k) || is_on_shitlist,
        is_unwanted_category: k.category_slugs().iter().any(|c| &**c == "cryptography::cryptocurrencies"),
    };

    debug!("score {base_score:?} {temp_score:?} {overall:?}");
    let score = ranking::combined_score(base_score, temp_score, &overall);

    Ok((downloads_per_month as usize, score))
}
}

fn print_res<T>(res: Result<T, anyhow::Error>) {
    if let Err(e) = res {
        let s = e.to_string();
        if s.starts_with("Too many open files") {
            stop();
            panic!("{}", s);
        }
        log::error!("••• Error: {}", s);
        for c in e.chain().skip(1) {
            let s = c.to_string();
            log::error!("•   error: -- {}", s);
            if s.starts_with("Too many open files") {
                stop();
                panic!("{}", s);
            }
        }
    }
}

async fn run_timeout<'a, T, E>(label: &'static str, secs: u64, fut: impl Future<Output = Result<T, E>> + 'a) -> Result<T, anyhow::Error>
where anyhow::Error: From<E> {
    let res = tokio::time::timeout(Duration::from_secs(secs), fut).await.map_err(move |_| anyhow!("{label} timed out {secs}"))?;
    res.map_err(From::from)
}
