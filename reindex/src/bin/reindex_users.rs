use futures::future::FutureExt;
use futures::stream::StreamExt;
use kitchen_sink::{stopped, KitchenSink};
use std::{
    collections::HashSet,
    sync::{mpsc, Arc},
};

#[tokio::main]
async fn main() {
    let handle = Arc::new(tokio::runtime::Handle::current());
    handle.clone().spawn(async move {
        let crates = Arc::new(match KitchenSink::new_default().await {
            Ok(a) => a,
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            },
        });
        let crates2 = crates.clone();
        let (tx, rx) = mpsc::sync_channel(64);
        let tx1 = tx.clone();

        let t = std::thread::spawn(move || {
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
        });

        let all_crates = crates.all_crates();
        let waiting = futures::stream::FuturesUnordered::new();
        let concurrency = Arc::new(tokio::sync::Semaphore::new(16));
        for o in all_crates {
            if stopped() {
                eprintln!("STOPPING");
                break;
            }
            let crates = Arc::clone(&crates);
            let concurrency = Arc::clone(&concurrency);
            let tx = tx1.clone();
            waiting.push(handle.spawn(async move {
                let _f = concurrency.acquire().await;
                println!("{}…", o.short_crate_name());
                let r1 = crates.rich_crate_version_async(&o).await;
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
            }).map(drop));
        }
        let _ = waiting.collect::<()>().await;
        eprintln!("Finished sending");
        tx.send(None).unwrap();
        t.join().unwrap();
    }).await.unwrap();
}
