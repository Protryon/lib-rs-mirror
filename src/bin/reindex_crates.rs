use kitchen_sink;
use failure;
use rayon;
use rand::seq::SliceRandom;
use rand::thread_rng;
use kitchen_sink::{KitchenSink, CrateData};
use kitchen_sink::RichCrateVersion;
use kitchen_sink::Origin;
use kitchen_sink::stopped;
use std::sync::{Arc, Mutex};
use std::collections::HashSet;
use either::*;

fn main() {
    let crates = Arc::new(match kitchen_sink::KitchenSink::new_default() {
        Ok(a) => a,
        e => {
            print_res(e);
            std::process::exit(1);
        },
    });

    let everything = false;
    let repos = !everything;

    let seen_repos = &Mutex::new(HashSet::new());
    rayon::scope(move |s1| {
        let c = if everything {
            let mut c: Vec<_> = crates.all_crates().keys().cloned().collect::<Vec<_>>();
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
            s1.spawn(move |s2| {
                if stopped() {
                    return;
                }
                print!("{} ", i);
                match index_crate(&crates, &k) {
                    Ok(v) => if repos {
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
                    },
                    err => print_res(err),
                }
            });
        }
    });
}

fn index_crate(crates: &KitchenSink, c: &Origin) -> Result<RichCrateVersion, failure::Error> {
    let v = crates.rich_crate_version(c, CrateData::FullNoDerived)?;
    crates.index_crate_highest_version(&v)?;
    let k = crates.rich_crate(c)?;
    crates.index_crate(&k)?;
    Ok(v)
}

fn print_res<T>(res: Result<T, failure::Error>) {
    if let Err(e) = res {
        eprintln!("••• Error: {}", e);
        for c in e.iter_chain().skip(1) {
            eprintln!("•   error: -- {}", c);
        }
    }
}
