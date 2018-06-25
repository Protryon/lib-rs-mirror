//! Render the whole website - homepage, category pages, and crate pages linked there

extern crate front_end;
extern crate kitchen_sink;
extern crate failure;
extern crate rayon;
extern crate render_readme;
extern crate categories;

use kitchen_sink::Crate;
use rayon::prelude::*;
use std::path::Path;
use std::path::PathBuf;
use kitchen_sink::KitchenSink;
use render_readme::ImageOptimAPIFilter;
use categories::CategoryMap;
use std::fs;
use std::fs::File;
use std::sync::{Arc, Mutex};
use std::collections::HashSet;

///
/// See home_page.rs for interesting bits
///
fn main() -> Result<(), failure::Error> {
    let mut out = File::create("public/index.html").unwrap();
    let crates = KitchenSink::new_default().unwrap();
    let done_pages = Mutex::new(HashSet::new());
    let image_filter = Arc::new(render_readme::ImageOptimAPIFilter::new("czjpqfbdkz", crates.main_cache_path())?);

    println!("Generating homepage…");
    let res = front_end::render_homepage(&mut out, &crates)
        .and_then(|_| {
            println!("Generating category pages…");
            render_categories(&categories::CATEGORIES.root, Path::new("public"), &crates, &done_pages, image_filter.clone())
        });

    if let Err(e) = res {
        eprintln!("Website generation failed: {}", e);
        for c in e.causes() {
            eprintln!("error: -- {}", c);
        }
        std::process::exit(1);
    }

    println!("http://localhost:3000/");
    Ok(())
}

fn render_categories(cats: &CategoryMap, base: &Path, crates: &KitchenSink, done_pages: &Mutex<HashSet<String>>, image_filter: Arc<ImageOptimAPIFilter>) -> Result<(), failure::Error> {
    cats.par_iter().map(|(slug, cat)| {
        if !cat.sub.is_empty() {
            let new_base = base.join(slug);
            let _ = fs::create_dir(&new_base);
            render_categories(&cat.sub, &new_base, crates, done_pages, image_filter.clone())?;
        }
        let render_crate = |c: Crate| {
            {
                let mut s = done_pages.lock().unwrap();
                if s.get(c.name()).is_some() {
                    return Ok(());
                }
                s.insert(c.name().to_owned());
            }
            let allver = crates.rich_crate(&c)?;
            let ver = crates.rich_crate_version(&c)?;
            let path = PathBuf::from(format!("public/crates/{}.html", c.name()));
            println!("{}", path.display());
            let mut outfile = File::create(&path)?;
            front_end::render_crate_page(&mut outfile, &allver, &ver, crates, image_filter.clone());
            Ok(())
        };

        crates.top_crates_in_category(&cat.slug, 55)?
        .into_par_iter().map(|(c,_)| render_crate(c))
        .collect::<Result<(), failure::Error>>()?;

        crates.recently_updated_crates_in_category(&cat.slug)?
        .into_par_iter().map(render_crate)
        .collect::<Result<(), failure::Error>>()?;

        let path = base.join(format!("{}.html", slug));
        let mut out = File::create(&path)?;
        front_end::render_category(&mut out, cat, crates, image_filter.clone())?;
        println!("{}", path.display());
        Ok(())
    }).collect()
}
