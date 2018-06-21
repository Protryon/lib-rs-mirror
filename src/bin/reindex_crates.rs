extern crate crate_db;
extern crate kitchen_sink;
extern crate failure;
extern crate rayon;
use std::sync::Arc;

fn main() {
    let crates = Arc::new(match kitchen_sink::KitchenSink::new_default() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        },
    });

    rayon::scope(move |s1| {
        for k in crates.all_crates() {
            let crates = Arc::clone(&crates);
            s1.spawn(move |_| {
                let res = crates.index_crate(&k);
                if let Err(e) = res {
                    eprintln!("••• error: {}", e);
                    for c in e.causes() {
                        eprintln!("•   error: -- {}", c);
                    }
                }
            });
        }
    });
}
