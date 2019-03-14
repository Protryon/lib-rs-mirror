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
use ranking::CrateVersionInputs;
use render_readme::Renderer;

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
    let renderer = Renderer::new(None);
    while let Ok((all, ver)) = rx.recv() {
        index(&mut indexer, &renderer, &all, &ver, crates2.downloads_per_month_or_equivalent(all.origin())?.unwrap_or(0))?;
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


fn crate_base_score(all: &RichCrate, k: &RichCrateVersion, renderer: &Renderer) -> f64 {
    let readme = k.readme().ok().and_then(|r| r).map(|readme| {
        renderer.page_node(&readme.markup, None, false)
    });
    let mut score = ranking::crate_score_version(&CrateVersionInputs {
        versions: all.versions(),
        description: k.description().unwrap_or(""),
        readme: readme.as_ref(),
        owners: all.owners(),
        authors: k.authors(),
        edition: k.edition(),
        is_app: k.is_app(),
        has_build_rs: k.has_buildrs(),
        has_links: k.links().is_some(),
        has_documentation_link: k.documentation().is_some(),
        has_homepage_link: k.homepage().is_some(),
        has_repository_link: k.repository().is_some(),
        has_keywords: k.has_own_keywords(),
        has_categories: k.has_own_categories(),
        has_features: !k.features().is_empty(),
        has_examples: k.has_examples(),
        has_benches: k.has_benches(),
        has_tests: k.has_tests(),
        // has_lockfile: k.has_lockfile(),
        // has_changelog: k.has_changelog(),
        license: k.license().unwrap_or(""),
        has_badges: k.has_badges(),
        maintenance: k.maintenance(),
        is_nightly: k.is_nightly(),
    }).total();


    // there's usually a non-macro/non-sys sibling
    if k.is_proc_macro() || k.is_sys() {
        score *= 0.9;
    }

    // k bye
    if k.is_yanked() {
        score *= 0.001;
    }

    score
}

fn index(indexer: &mut Indexer, renderer: &Renderer, all: &RichCrate, k: &RichCrateVersion, popularity: usize) -> Result<(), failure::Error> {

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
    let pop_score = ((popularity+10) as f64).log2() / (if k.is_app() {7.0} else {14.0});

    // based on crate's own content and metadata
    let base_score = crate_base_score(all, k, renderer);

    let score = ((0.5 + pop_score) * base_score).min(1.0);

    println!("{:0.3} {:0.3} {}: {}", score, base_score, k.short_name(), k.description().unwrap_or(""));

    indexer.add(k.short_name(), version, k.description().unwrap_or(""), &keywords, readme, popularity as u64, score);
    Ok(())
}


fn print_err(e: failure::Error) {
    eprintln!("••• Error: {}", e);
    for c in e.iter_chain().skip(1) {
        eprintln!("•   error: -- {}", c);
    }
}
