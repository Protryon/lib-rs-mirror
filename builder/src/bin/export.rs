use crate_db::builddb::*;
use std::collections::hash_map::Entry;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let crates = kitchen_sink::KitchenSink::new_default().await.unwrap();

    let db = BuildDb::new(crates.main_cache_dir().join("builds.db")).unwrap();
    let mut outputs = BTreeSet::new();

    println!("pkg-config 1.24.1 >=0.3.15");
    println!("regex-syntax 1.24.1 >=0.6.9");
    println!("rustc-demangle 1.24.1 >=0.1.16");
    println!("proc-macro2 1.30.1 >=1.0.0");
    println!("lazy_static 1.24.1 >=1.4.0");
    println!("backtrace 1.24.1 >=0.3.30");
    println!("backtrace 9.9.9 <=0.1.8");
    println!("backtrace 9.9.9 =0.2.2");
    println!("backtrace 9.9.9 =0.2.3");
    println!("gcc 9.9.9 <=0.3.0");
    println!("lazy_static 9.9.9 <=0.1.0");
    println!("libc 9.9.9 ^0.1.0");
    println!("mio 9.9.9 <=0.3.7");
    println!("mio 9.9.9 =0.6.0");
    println!("nix 9.9.9 =0.5.0");
    println!("num 9.9.9 <=0.1.25");
    println!("pkg 9.9.9-config <=0.3.2");
    println!("rand 9.9.9 <=0.3.8");
    println!("rustc 9.9.9-serialize <=0.3.21");
    println!("semver 9.9.9 <=0.1.5");
    println!("tokio 9.9.9-io <=0.1.2");
    println!("tokio 9.9.9-reactor <=0.1.0");
    println!("variants 9.9.9 =0.0.1");
    println!("void 9.9.9 <=0.0.4");
    println!("winapi 9.9.9 <=0.1.17");

    for c in db.get_all_compat().unwrap() {
        if !c.origin.is_crates_io() {
            continue;
        }
        let mut combined = HashMap::with_capacity(c.rustc_versions.len());
        if c.rustc_versions.iter().all(|(_, compat)| compat.crate_newest_ok.is_none()) {
            // Can't build it at all, so it's probably our env that's broken, not the crate
            continue;
        }
        for (rust_ver, compat) in c.rustc_versions {
            if let Some(bork_old) = compat.crate_oldest_bad {
                let newest_ok = compat.crate_newest_ok.filter(|n| n > &bork_old);
                match combined.entry((bork_old, newest_ok)) {
                    Entry::Vacant(e) => {
                        e.insert(rust_ver);
                    },
                    Entry::Occupied(mut e) => {
                        if e.get() < &rust_ver {
                            *e.get_mut() = rust_ver;
                        }
                    },
                }
            }
        }
        // sort
        let name: Arc<str> = c.origin.short_crate_name().into();
        for (bork, rust_ver) in combined {
            outputs.insert((name.clone(), rust_ver, bork));
        }
        if let Some(oldbork) = c.old_crates_broken_up_to {
            println!("{} 9.9.9 <={}", name, oldbork);
        }
    }
    for (name, rust_ver, (oldest_bad, newest_ok)) in outputs {
        if let Some(newest_ok) = newest_ok {
            println!("{} {} >{}", name, rust_ver, newest_ok);
        } else {
            println!("{} {} >={}", name, rust_ver, oldest_bad);
        }
    }
}
