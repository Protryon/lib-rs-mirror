extern crate failure;
extern crate github_info;
extern crate kitchen_sink;
extern crate rayon;
extern crate repo_url;
extern crate user_db;
use kitchen_sink::stopped;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::collections::HashSet;
use kitchen_sink::{KitchenSink, CrateData};

fn main() {
    let crates = Arc::new(match KitchenSink::new_default() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        },
    });
    let crates2 = crates.clone();
    let (tx, rx) = mpsc::sync_channel(64);

    thread::spawn(move || {
        let tx1 = tx.clone();
        let all_crates = crates.all_crates();
        let crates = Arc::clone(&crates);
        rayon::scope(move |s1| {
            for (o, k) in all_crates {
                if stopped() {
                    eprintln!("STOPPING");
                    break;
                }
                let crates = Arc::clone(&crates);
                let tx = tx1.clone();
                s1.spawn(move |_| {
                    println!("{:?}", k.name());
                    let r1 = crates.rich_crate_version(o, CrateData::Minimal);
                    let res = r1.and_then(|c| {
                        for a in c.authors().iter().filter(|a| a.email.is_some()) {
                            if let Some(email) = a.email.as_ref() {
                                tx.send(Some((email.to_string(), a.name.clone())))?;
                            }
                        }
                        Ok(())
                    });
                    if let Err(e) = res {
                        eprintln!("••• error: {}", e);
                        for c in e.iter_chain() {
                            eprintln!("•   error: -- {}", c);
                        }
                    }
                });
            }
        });
        tx.send(None).unwrap();
    });

    let mut seen = HashSet::new();
    while let Some((email, name)) = rx.recv().unwrap() {
        let email: String = email;
        let name: Option<String> = name;

        if seen.contains(&email) {
            continue;
        }
        seen.insert(email.clone());

        if let Err(err) = crates2.index_email(&email, name.as_ref().map(|s| s.as_str())) {
            eprintln!("•• {}: {}", email, err);
        }
    }
}
