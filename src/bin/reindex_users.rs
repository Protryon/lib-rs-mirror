extern crate failure;
extern crate github_info;
extern crate kitchen_sink;
extern crate rayon;
extern crate repo_url;
extern crate user_db;
use repo_url::Repo;
use repo_url::RepoHost;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::collections::HashSet;

fn main() {
    let crates = Arc::new(match kitchen_sink::KitchenSink::new_default() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        },
    });
    let crates2 = crates.clone();
    let (tx, rx) = mpsc::channel();
    let mut seen = HashSet::new();

    thread::spawn(move || {
        let tx1 = tx.clone();
        rayon::scope(move |s1| {
            for k in crates.all_crates() {
                let crates = Arc::clone(&crates);
                println!("{:?}", k.name());
                let tx = tx1.clone();
                s1.spawn(move |_| {
                    let r1 = crates.rich_crate_version(&k);
                    let res = r1.and_then(|c| {
                        if let Some(Repo{host:RepoHost::GitHub(repo),..}) = c.repository() {
                            if let Ok(commits) = crates.repo_commits(&repo) {
                                for c in commits {
                                    crates.index_user(&c.author, &c.commit.author)?;
                                    crates.index_user(&c.committer, &c.commit.committer)?;
                                }
                            }
                        }

                        for a in c.authors().iter().filter(|a| a.email.is_some()) {
                            if let Some(email) = a.email.as_ref() {
                                tx.send(Some((email.to_string(), a.name.clone())))?;
                            }
                        }
                        Ok(())
                    });
                    if let Err(e) = res {
                        eprintln!("••• error: {}", e);
                        for c in e.causes() {
                            eprintln!("•   error: -- {}", c);
                        }
                    }
                });
            }
        });
        tx.send(None).unwrap();
    });

    while let Some((email, name)) = rx.recv().unwrap() {
        let email: String = email;
        let name: Option<String> = name;

        if seen.contains(&email) {
            continue;
        }
        seen.insert(email.clone());

        crates2.index_email(&email, name.as_ref().map(|s| s.as_str())).unwrap();
    }
}
