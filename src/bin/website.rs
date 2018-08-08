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
use failure::Fail;
use failure::ResultExt;

///
/// See home_page.rs for interesting bits
///
fn main() -> Result<(), failure::Error> {
    let mut out = File::create("public/index.html").expect("write to public/index.html");
    let crates = KitchenSink::new_default().expect("init caches, data, etc.");
    let done_pages = Mutex::new(HashSet::new());
    let image_filter = Arc::new(render_readme::ImageOptimAPIFilter::new("czjpqfbdkz", crates.main_cache_path())?);

    println!("Generating homepage and category pages…");
    let (res1, res2) = rayon::join(|| {
        front_end::render_homepage(&mut out, &crates)
        .context("Failed rendering homepage")
    }, || {
        let _ = fs::create_dir_all("public/crates");
        render_categories(&categories::CATEGORIES.root, Path::new("public"), &crates, &done_pages, image_filter.clone())
        .context("Failed rendering categories")
    });

    if let Err(e) = res1.and_then(|_| res2) {
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
            let allver = crates.rich_crate(&c).context("get crate")?;
            let ver = crates.rich_crate_version(&c).context("get rich crate")?;
            let path = PathBuf::from(format!("public/crates/{}.html", c.name()));
            println!("{}", path.display());
            let mut outfile = File::create(&path)
                .with_context(|_| format!("Can't create {}", path.display()))?;
            front_end::render_crate_page(&mut outfile, &allver, &ver, crates, image_filter.clone());
            Ok(())
        };

        crates.top_crates_in_category(&cat.slug, 75, true)
            .context("top crates")?
        .into_par_iter()
        .map(|(c,_)| {
            let msg = format!("Failed rendering crate {}", c.name());
            render_crate(c).context(msg)
        })
        .collect::<Result<(), _>>()?;

        crates.recently_updated_crates_in_category(&cat.slug)
            .context("recently updated crates")?
        .into_par_iter().map(render_crate)
        .collect::<Result<(), failure::Error>>()?;

        let path = base.join(format!("{}.html", slug));
        let mut out = File::create(&path)
            .with_context(|_| format!("Can't create {}", path.display()))?;
        front_end::render_category(&mut out, cat, crates, image_filter.clone())?;
        println!("{}", path.display());
        Ok(())
    }).collect()
}
