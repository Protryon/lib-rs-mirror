extern crate kitchen_sink;
extern crate failure;
extern crate rayon;
extern crate search_index;
extern crate rand;
use rand::prelude::SliceRandom;
use kitchen_sink::RichCrate;
use search_index::*;
use rayon::prelude::*;
use rand::thread_rng;
use kitchen_sink::{KitchenSink, RichCrateVersion, Markup, CrateData, Include};
use kitchen_sink::stopped;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;

fn main() {
    if let Err(e) = run() {
        print_err(e);
        std::process::exit(1);
    }
}

fn run() -> Result<(), failure::Error> {
    println!("start");
    let crates = Arc::new(KitchenSink::new_default()?);
    let crates2 = crates.clone();
    let mut indexer = Indexer::new(CrateSearchIndex::new(crates.main_cache_dir())?)?;
    let (tx, rx) = mpsc::sync_channel(64);

    let t = thread::spawn(move || {
        let c = crates.clone();
        let mut c: Vec<_> = c.all_crates().collect::<Vec<_>>();
        c.shuffle(&mut thread_rng());
        c.into_par_iter()
        .for_each(|k| {
            if stopped() {return;}
            let res = crates.rich_crate(&k).and_then(|all| {
                crates.rich_crate_version(&k, CrateData::Full)
                .map(|ver| (all, ver))
            })
            .and_then(|res| Ok(tx.send(res)?));
            if let Err(e) = res {
                print_err(e);
            }
        });
        Ok(())
    });

    let mut n = 0;
    let mut next_n = 100;
    while let Ok((all, ver)) = rx.recv() {
        index(&mut indexer, &all, &ver, crates2.downloads_per_month_or_equivalent(all.origin())?.unwrap_or(0))?;
        if stopped() {break;}
        n += 1;
        if n == next_n {
            next_n *= 2;
            println!("savepoint…");
            indexer.commit()?;
        }
    }
    indexer.commit()?;
    let _ = indexer.bye()?;

    t.join().unwrap()
}

fn index(indexer: &mut Indexer, all: &RichCrate, k: &RichCrateVersion, popularity: usize) -> Result<(), failure::Error> {

    let keywords: Vec<_> = k.keywords(Include::Cleaned).collect();
    let readme = match k.readme() {
        Ok(Some(r)) => Some(match r.markup {
            Markup::Markdown(ref s) | Markup::Rst(ref s) => s.as_str(),
        }),
        _ => None,
    };
    let version = k.version();

    // Base score is from donwloads per month.
    // apps have it harder to get download numbers
    let mut score = ((popularity+10) as f64).log2() / (if k.is_app() {7.0} else {14.0});

    // Try to get rid of junk crates
    if !version.starts_with("0.0.") && !version.starts_with("0.1.0") {
        score += 1.;
    }
    let releases = all.versions().count().min(10);
    if releases > 1 {
        score += releases as f64 / 10.0;
    }

    // bus factor
    if k.authors().len() > 1 {
        score += 0.1;
    }

    // Prefer stable crates
    if version.starts_with("0.") {
        score *= 0.9;
    }

    // long descriptions are better
    if k.description().map_or(false, |d| d.len() > 50) {
        score += 0.1;
    }

    // there's usually a non-macro sibling
    if k.is_proc_macro() {
        score *= 0.9;
    }

    // k bye
    if k.is_yanked() {
        score *= 0.001;
    }

    score = (score / 4.0).min(1.0); // keep it in the range

    println!("{:0.3} {}: {}", score, k.short_name(), k.description().unwrap_or(""));

    indexer.add(k.short_name(), version, k.description().unwrap_or(""), &keywords, readme, popularity as u64, score);
    Ok(())
}


fn print_err(e: failure::Error) {
    eprintln!("••• Error: {}", e);
    for c in e.iter_chain().skip(1) {
        eprintln!("•   error: -- {}", c);
    }
}
