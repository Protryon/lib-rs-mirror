use futures::future::FutureExt;
use futures::stream::StreamExt;
use kitchen_sink::{stopped, KitchenSink};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
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

        let all_crates = crates.all_crates();
        let waiting = futures::stream::FuturesUnordered::new();
        let concurrency = Arc::new(tokio::sync::Semaphore::new(16));
        let seen = Arc::new(Mutex::new(HashSet::new()));
        for o in all_crates {
            if stopped() {
                eprintln!("STOPPING");
                break;
            }
            let crates = Arc::clone(&crates);
            let concurrency = Arc::clone(&concurrency);
            let seen = Arc::clone(&seen);
            waiting.push(handle.spawn(async move {
                let _f = concurrency.acquire().await;
                println!("{}…", o.short_crate_name());
                let c = crates.rich_crate_version_async(&o).await?;
                if stopped() {failure::bail!("stop");}
                for a in c.authors().iter().filter(|a| a.email.is_some()) {
                    if let Some(email) = a.email.as_ref() {
                        {
                            let mut seen = seen.lock().unwrap();
                            if seen.contains(email) {
                                continue;
                            }
                            seen.insert(email.clone());
                        }

                        if let Err(err) = crates.index_email(email, a.name.as_deref()).await {
                            eprintln!("•• {}: {}", email, err);
                        }
                    }
                }
                Ok(())
            }.then(|res| async move {
                if let Err(e) = res {
                    eprintln!("••• error: {}", e);
                    for c in e.iter_chain() {
                        eprintln!("•   error: -- {}", c);
                    }
                }
            })).map(drop));
        }
        if stopped() {return;}
        waiting.collect::<()>().await;
        eprintln!("Finished");
    }).await.unwrap();
}
