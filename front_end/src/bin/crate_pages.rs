use kitchen_sink::RichCrate;
use front_end;
use kitchen_sink;
use rayon;
use kitchen_sink::{stopped, CrateData, KitchenSink, Origin};
use render_readme::{Highlighter, ImageOptimAPIFilter, Renderer};
use rich_crate::RichCrateVersion;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

fn main() {
    if let Err(e) = run(std::env::args().nth(1)) {
        eprintln!("error: {}", e);
        for c in e.iter_chain() {
            eprintln!("error: -- {}", c);
        }
        std::process::exit(1);
    }
}

fn is_useful1(allver: &RichCrate) -> bool {
    if allver.versions().count() < 2 {
        eprintln!("{} one release", allver.name());
        return false;
    }
    true
}

fn is_useful2(c: &RichCrateVersion) -> bool {
    if c.repository().is_none() {
        eprintln!("{} no repo", c.short_name());
        return false;
    }
    if c.is_yanked() || c.description().is_none() {
        eprintln!("{} yank", c.short_name());
        return false;
    }
    true
}

fn render(origin: &Origin, crates: &KitchenSink, path: &PathBuf, markup: &Renderer, always: bool) -> Result<(), failure::Error> {

    let allver = crates.rich_crate(origin)?;
    if !always && !is_useful1(&allver) {
        return Ok(());
    }

    let c = crates.rich_crate_version(origin, CrateData::Full)?;
    if !always && !is_useful2(&c) {
        return Ok(());
    }

    let mut buf = Vec::new();
    let title = front_end::render_crate_page(&mut buf, &allver, &c, crates, markup)?;
    fs::write(&path, buf)?;
    println!("{} | {}", path.display(), title);
    Ok(())
}

fn run(filter: Option<String>) -> Result<(), failure::Error> {
    rayon::ThreadPoolBuilder::new().thread_name(|i| format!("rayon-{}", i)).build_global()?;

    let crates = Arc::new(kitchen_sink::KitchenSink::new_default()?);
    // crates.prewarm();
    let image_filter = Arc::new(ImageOptimAPIFilter::new("czjpqfbdkz", crates.main_cache_dir().join("images.db"))?);
    let markup = &Renderer::new_filter(Highlighter::new(), image_filter);
    rayon::scope(move |s1| {
        for origin in crates.all_crates() {
            if let Some(ref filter) = filter {
                if origin.short_crate_name() != filter {
                    continue;
                }
            }
            if stopped() {
                break;
            }
            let origin = origin.clone();
            let always_render = filter.is_some();
            let crates = Arc::clone(&crates);
            let path = PathBuf::from(format!("public/crates/{}.html", origin.short_crate_name()));
            s1.spawn(move |_| {
                if let Err(e) = render(&origin, &crates, &path, markup, always_render) {
                    eprintln!("••• error: {} — {}", e, path.display());
                    for c in e.iter_chain().skip(1) {
                        eprintln!("•   error: -- {}", c);
                    }
                    if path.exists() {
                        std::fs::remove_file(path).ok();
                    }
                }
            })
        }
        Ok(())
    })
}