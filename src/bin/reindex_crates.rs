extern crate crate_db;
extern crate kitchen_sink;
extern crate failure;
extern crate rayon;
use kitchen_sink::{KitchenSink, CrateData};
use kitchen_sink::RichCrateVersion;
use kitchen_sink::Origin;
use std::sync::{Arc, Mutex};
use std::collections::HashSet;
extern crate rand;

use rand::{thread_rng, Rng};

fn main() {
    let crates = Arc::new(match kitchen_sink::KitchenSink::new_default() {
        Ok(a) => a,
        e => {
            print_res(e);
            std::process::exit(1);
        },
    });

    let seen_repos = &Mutex::new(HashSet::new());
    let repos = false;

    rayon::scope(move |s1| {
        let mut c = crates.all_crates().map(|c| c.name().to_string()).collect::<Vec<_>>();
        thread_rng().shuffle(&mut c);
        let total = c.len();
        for (i, k) in c.into_iter().enumerate() {
            let crates = Arc::clone(&crates);
            s1.spawn(move |s2| {
                print!("{} ", i*100/total);
                match index_crate(&crates, &Origin::from_crates_io_name(&k)) {
                    Ok(v) => if repos {s2.spawn(move |_| {
                        if let Some(ref repo) = v.repository() {
                            {
                                let mut s = seen_repos.lock().unwrap();
                                let url = repo.canonical_git_url().to_string();
                                if s.contains(&url) {
                                    return;
                                }
                                s.insert(url);
                            }
                            print_res(crates.index_repo(repo, v.short_name()));
                        }
                    })},
                    err => print_res(err),
                }
            });
        }
    });
}

fn index_crate(crates: &KitchenSink, c: &Origin) -> Result<RichCrateVersion, failure::Error> {
    let v = crates.rich_crate_version(c, CrateData::FullNoDerived)?;
    crates.index_crate_latest_version(&v)?;
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
