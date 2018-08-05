extern crate crate_db;
extern crate kitchen_sink;
extern crate failure;
extern crate rayon;
use kitchen_sink::KitchenSink;
use kitchen_sink::RichCrateVersion;
use kitchen_sink::Crate;
use std::sync::{Arc, Mutex};
use std::collections::HashSet;

fn main() {
    let crates = Arc::new(match kitchen_sink::KitchenSink::new_default() {
        Ok(a) => a,
        e => {
            print_res(e);
            std::process::exit(1);
        },
    });

    let seen_repos = &Mutex::new(HashSet::new());

    rayon::scope(move |s1| {
        for k in crates.all_crates() {
            let crates = Arc::clone(&crates);
            s1.spawn(move |s2| {
                match index_crate(&crates, &k) {
                    Ok(v) => s2.spawn(move |_| {
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
                    }),
                    err => print_res(err),
                }
            });
        }
    });
}

fn index_crate(crates: &KitchenSink, c: &Crate) -> Result<RichCrateVersion, failure::Error> {
    let k = crates.rich_crate(c)?;
    let v = crates.rich_crate_version(c)?;
    crates.index_crate(&k)?;
    crates.index_crate_latest_version(&v)?;
    Ok(v)
}

fn print_res<T>(res: Result<T, failure::Error>) {
    if let Err(e) = res {
        eprintln!("••• Error: {}", e);
        for c in e.causes().into_iter().skip(1) {
            eprintln!("•   error: -- {}", c);
        }
    }
}
