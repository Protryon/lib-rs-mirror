#![allow(unused)]

use kitchen_sink::{KitchenSink, CrateData};

fn main() {
    let mut crates = KitchenSink::new_default().unwrap();
    // crates.cache_only(true);

    let (authors, deps) = rayon::join(
        || ranking::do_author_pr(&crates).unwrap(),
        || crates_by_rev_dep(&crates));

    let mut top: Vec<_> = authors.iter().collect();
    top.sort_by(|a,b| b.1.partial_cmp(&a.1).unwrap());
    top.truncate(100);
    for (author, score) in top {
        println!("{}: {:0.4}", author, score);
    }

    let mut by_risk: Vec<_> = deps.into_iter().filter(|&(_, rev_deps, _)| rev_deps > 5).map(|(name, rev_deps, owners)| {
        // most trusted finds most risky crates by unvetted authors.
        // least trusted would find crates with weakest links
        // (which is useful too, but too soon to address when we have almost no reviews for anything yet)
        let most_trusted = owners.into_iter().filter_map(|o| authors.get(&*o).cloned()).fold(0., |a:f64,b:f64| a.max(b));
        let risk = (rev_deps as f64) / (0.000001 + most_trusted);
        (name, risk)
    }).collect();

    by_risk.sort_by(|a,b| b.1.partial_cmp(&a.1).unwrap());
    by_risk.truncate(200);
    for (s, a) in by_risk {
        println!("{} {}", s, a);
    }
}

fn crates_by_rev_dep(crates: &KitchenSink) -> Vec<(&str, u32, Vec<Box<str>>)> {
    let mut res = Vec::new();
    for k in crates.all_crates().values() {
        let name = k.name();
        if let Some(rev) = crates.dependents_stats_of_crates_io_crate(name) {
            if let Ok(owners) = crates.crates_io_crate_owners(name, k.latest_version().version()) {
                let rev_dep_count = rev.runtime.0 as u32 * 2 + rev.runtime.1 as u32 + rev.build.0 as u32 * 2 + rev.build.1 as u32 + rev.dev as u32 / 2;
                let owners = owners.into_iter()
                    .filter_map(|o| o.github_login().map(|l| l.to_ascii_lowercase().into_boxed_str()))
                    .collect();
                res.push((name, rev_dep_count, owners));
            }
        }
    }
    res
}
