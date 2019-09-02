use std::sync::Arc;
use crate_db::builddb::*;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::hash_map::Entry;


fn main() {
    let crates = kitchen_sink::KitchenSink::new_default().unwrap();

    let db = BuildDb::new(crates.main_cache_dir().join("builds.db")).unwrap();
    let mut outputs = BTreeSet::new();

    for (origin, rows) in db.get_all_compat().unwrap() {
        if !origin.is_crates_io() {
            continue;
        }

        let mut combined = HashMap::with_capacity(rows.len());
        if rows.iter().all(|(_, compat)| compat.newest_ok.is_none()) {
            // Can't build it at all, so it's probably our env that's broken, not the crate
            continue;
        }
        for (rust_ver, compat) in rows {
            if let Some(bork) = compat.oldest_bad {
                if compat.newest_ok.map_or(false, |n| n > bork) {
                    // invalid data (old version didn't build, but a newer does build)
                    continue;
                }
                match combined.entry(bork) {
                    Entry::Vacant(e) => {
                        e.insert(rust_ver);
                    },
                    Entry::Occupied(mut e) => {
                        if e.get() > &rust_ver {
                            e.insert(rust_ver);
                        }
                    },
                }
            }
        }
        // sort
        let name: Arc<str> = origin.short_crate_name().into();
        for (bork, rust_ver) in combined {
            outputs.insert((rust_ver, name.clone(), bork));
        }
    }
    for (rust_ver, name, bork) in outputs {
        println!("{} {} {}", rust_ver, name, bork);
    }
}
