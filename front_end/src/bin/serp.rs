use front_end;
use failure;
use std::fs::File;
use std::io::BufWriter;
use std::sync::Arc;
use render_readme::{Highlighter, ImageOptimAPIFilter, Renderer};

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {}", e);
        for c in e.iter_chain() {
            eprintln!("error: -- {}", c);
        }
        std::process::exit(1);
    }
}

fn run() -> Result<(), failure::Error> {
    let index = search_index::CrateSearchIndex::new("../data")?;
    let query = std::env::args().nth(1).expect("search for something");
    let results = index.search(&query, 50)?;
    for r in &results {
        println!("[{}] {}: {}", r.score, r.crate_name, r.description);
    }

    let mut f = BufWriter::new(File::create("public/search.html")?);
    println!("public/search.html");
    println!("http://localhost:3000/search");

    let image_filter = Arc::new(ImageOptimAPIFilter::new("czjpqfbdkz", "../data/images.db")?);
    let markup = Renderer::new_filter(Some(Highlighter::new()), image_filter);

    front_end::render_serp_page(&mut f, &query, &results, &markup)?;
    Ok(())
}
