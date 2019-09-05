use std::sync::Arc;
use crate_db::builddb::*;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::hash_map::Entry;


fn main() {
    let crates = kitchen_sink::KitchenSink::new_default().unwrap();

    let db = BuildDb::new(crates.main_cache_dir().join("builds.db")).unwrap();
    let mut outputs = BTreeSet::new();

    println!("9.9.9 gcc <=0.3.0");
    println!("9.9.9 backtrace <=0.1.8");
    println!("9.9.9 backtrace =0.2.2");
    println!("9.9.9 backtrace =0.2.3");
    println!("9.9.9 lazy_static <=0.1.0");
    println!("9.9.9 libc ^0.1.0");
    println!("9.9.9 mio <=0.3.7");
    println!("9.9.9 mio =0.6.0");
    println!("9.9.9 nix =0.5.0");
    println!("9.9.9 num <=0.1.25");
    println!("9.9.9 pkg-config <=0.3.2");
    println!("1.24.0 pkg-config >=0.3.15");
    println!("1.24.0 regex-syntax >=0.6.9");
    println!("1.24.0 rustc-demangle >=0.1.16");
    println!("9.9.9 rand <=0.3.8");
    println!("9.9.9 rustc-serialize <=0.3.21");
    println!("9.9.9 semver <=0.1.5");
    println!("9.9.9 tokio-io <=0.1.2");
    println!("9.9.9 tokio-reactor <=0.1.0");
    println!("9.9.9 variants =0.0.1");
    println!("9.9.9 void <=0.0.4");
    println!("9.9.9 winapi <=0.1.17");

    for c in db.get_all_compat().unwrap() {
        if !c.origin.is_crates_io() || c.origin.short_crate_name() != "getopts" {
            continue;
        }

        let mut combined = HashMap::with_capacity(c.rustc_versions.len());
        if c.rustc_versions.iter().all(|(_, compat)| compat.crate_newest_ok.is_none()) {
            // Can't build it at all, so it's probably our env that's broken, not the crate
            continue;
        }
        for (rust_ver, compat) in c.rustc_versions {
            if let Some(bork) = compat.crate_oldest_bad {
                if let Some(n) = compat.crate_newest_ok {
                    if n > bork {
                        // invalid data (old version didn't build, but a newer does build)
                        continue;
                    }
                }
                match combined.entry(bork.clone()) {
                    Entry::Vacant(e) => {
                        e.insert(rust_ver);
                    },
                    Entry::Occupied(mut e) => {
                        if e.get() < &rust_ver {
                            e.insert(rust_ver);
                        }
                    },
                }
            }
        }
        // sort
        let name: Arc<str> = c.origin.short_crate_name().into();
        for (bork, rust_ver) in combined {
            outputs.insert((rust_ver, name.clone(), bork));
        }
        if let Some(oldbork) = c.old_crates_broken_up_to {
            println!("9.9.9 {} <={}", name, oldbork);
        }
    }
    for (rust_ver, name, bork) in outputs {
        println!("{} {} >={}", rust_ver, name, bork);
    }
}
