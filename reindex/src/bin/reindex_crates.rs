use ranking::CrateVersionInputs;
use kitchen_sink::RichCrate;
use render_readme::Renderer;
use either::*;
use failure;
use kitchen_sink::{self, stopped, CrateData, KitchenSink, Origin, RichCrateVersion};
use rand::{seq::SliceRandom, thread_rng};
use rayon;
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

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
            s1.spawn(move |s2| {
                if stopped() {
                    return;
                }
                print!("{} ", i);
                match index_crate(&crates, &k, &renderer) {
                    Ok(v) => {
                        if repos {
                            s2.spawn(move |_| {
                                if let Some(ref repo) = v.repository() {
                                    {
                                        let mut s = seen_repos.lock().unwrap();
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
    });
}

fn index_crate(crates: &KitchenSink, c: &Origin, renderer: &Renderer) -> Result<RichCrateVersion, failure::Error> {
    let v = crates.rich_crate_version(c, CrateData::FullNoDerived)?;
    let k = crates.rich_crate(c)?;
    let score = crate_base_score(&k, &v, renderer);
    crates.index_crate_highest_version(&v, score)?;
    crates.index_crate(&k)?;
    Ok(v)
}


fn crate_base_score(all: &RichCrate, k: &RichCrateVersion, renderer: &Renderer) -> f64 {
    let readme = k.readme().ok().and_then(|r| r).map(|readme| {
        renderer.page_node(&readme.markup, None, false)
    });
    let mut score = ranking::crate_score_version(&CrateVersionInputs {
        versions: all.versions(),
        description: k.description().unwrap_or(""),
        readme: readme.as_ref(),
        owners: all.owners(),
        authors: k.authors(),
        edition: k.edition(),
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
        has_benches: k.has_benches(),
        has_tests: k.has_tests(),
        // has_lockfile: k.has_lockfile(),
        // has_changelog: k.has_changelog(),
        license: k.license().unwrap_or(""),
        has_badges: k.has_badges(),
        maintenance: k.maintenance(),
        is_nightly: k.is_nightly(),
    }).total();


    // there's usually a non-macro/non-sys sibling
    if k.is_proc_macro() || k.is_sys() {
        score *= 0.9;
    }

    // k bye
    if k.is_yanked() {
        score *= 0.001;
    }

    score
}


fn print_res<T>(res: Result<T, failure::Error>) {
    if let Err(e) = res {
        eprintln!("••• Error: {}", e);
        for c in e.iter_chain().skip(1) {
            eprintln!("•   error: -- {}", c);
        }
    }
}
