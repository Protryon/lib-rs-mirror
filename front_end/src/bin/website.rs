//! Render the whole website - homepage, category pages, and crate pages linked there

use front_end;

use failure;
use rayon;

use categories;

use categories::CategoryMap;
use failure::ResultExt;
use kitchen_sink::running;
use kitchen_sink::{CrateData, KitchenSink, Origin};
use rayon::prelude::*;
use render_readme::ImageOptimAPIFilter;
use render_readme::{Highlighter, Renderer};
use std::collections::HashSet;
use std::fs;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc};
use parking_lot::Mutex;

///
/// See home_page.rs for interesting bits
///
fn main() {
    if let Err(e) = run() {
        eprintln!("Website generation failed: {}", e);
        for c in e.iter_chain() {
            eprintln!("error: -- {} {:?}", c, c);
        }
        std::process::exit(1);
    }
}

fn run() -> Result<(), failure::Error> {
    let mut out = BufWriter::new(File::create("public/index.html").context("write to public/index.html")?);
    let mut feed = BufWriter::new(File::create("public/atom.xml").context("write to public/index.html")?);
    let crates = KitchenSink::new_default().context("init caches, data, etc.")?;
    let done_pages = Mutex::new(HashSet::with_capacity(5000));
    let image_filter = Arc::new(ImageOptimAPIFilter::new("czjpqfbdkz", crates.main_cache_dir().join("images.db"))?);
    let markup = Renderer::new_filter(Some(Highlighter::new()), image_filter);

    println!("Generating homepage and category pages…");
    let (res1, res2) = rayon::join(
        || front_end::render_homepage(&mut out, &crates).context("Failed rendering homepage").and_then(|_| front_end::render_feed(&mut feed, &crates).context("Failed rendering homepage")),
        || {
            let _ = fs::create_dir_all("public/crates");
            render_categories(&categories::CATEGORIES.root, Path::new("public"), &crates, &done_pages, &markup).context("Failed rendering categories")
        },
    );
    res1?;
    res2?;

    println!("http://localhost:3000/");
    Ok(())
}

fn render_categories(
    cats: &CategoryMap, base: &Path, crates: &KitchenSink, done_pages: &Mutex<HashSet<Origin>>, markup: &Renderer,
) -> Result<(), failure::Error> {
    cats.par_iter()
        .map(|(slug, cat)| {
            running()?;

            if !cat.sub.is_empty() {
                let new_base = base.join(slug);
                let _ = fs::create_dir(&new_base);
                render_categories(&cat.sub, &new_base, crates, done_pages, markup)?;
            }
            let render_crate = |origin: &Origin| {
                running()?;
                {
                    let mut s = done_pages.lock();
                    if s.get(origin).is_some() {
                        return Ok(());
                    }
                    s.insert(origin.clone());
                }
                let allver = match crates.rich_crate(origin) {
                    Ok(a) => a,
                    Err(e) => {
                        eprintln!("Crate in category fail: {:?}", e);
                        return Ok(()); // skip it
                    },
                };
                let ver = crates.rich_crate_version(origin, CrateData::Full).context("get rich crate")?;
                running()?;
                let path = PathBuf::from(format!("public/crates/{}.html", ver.short_name()));
                println!("http://localhost:3000/crates/{}", ver.short_name());
                let mut outfile = BufWriter::new(File::create(&path).with_context(|_| format!("Can't create {}", path.display()))?);
                front_end::render_crate_page(&mut outfile, &allver, &ver, crates, markup).context("render crate page")?;
                Ok(())
            };

            crates
                .top_crates_in_category(&cat.slug)
                .context("top crates")?
                .par_iter()
                .take(75)
                .with_max_len(1)
                .map(|(c, _)| {
                    let msg = format!("Failed rendering crate {} from category {}", c.to_str(), slug);
                    render_crate(c).context(msg)
                })
                .collect::<Result<(), _>>()?;

            running()?;

            crates
                .recently_updated_crates_in_category(&cat.slug)
                .context("recently updated crates")?
                .par_iter()
                .with_max_len(1)
                .map(render_crate)
                .collect::<Result<(), failure::Error>>()?;

            let path = base.join(format!("{}.html", slug));
            let mut out = BufWriter::new(File::create(&path).with_context(|_| format!("Can't create {}", path.display()))?);
            front_end::render_category(&mut out, cat, crates, markup)?;
            println!("{}", path.display());
            Ok(())
        })
        .collect()
}
