use either::Either;
use front_end;
use kitchen_sink;
use kitchen_sink::RichCrate;
use kitchen_sink::{stopped, KitchenSink, Origin};
use render_readme::{Highlighter, ImageOptimAPIFilter, Renderer};
use rich_crate::RichCrateVersion;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let handle = tokio::runtime::Handle::current();
    if let Err(e) = handle.spawn(run(std::env::args().nth(1))).await.unwrap() {
        eprintln!("error: {}", e);
        for c in e.iter_chain() {
            eprintln!("error: -- {}", c);
        }
        std::process::exit(1);
    }
}

fn is_useful1(allver: &RichCrate) -> bool {
    if allver.versions().len() < 2 {
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

async fn render(origin: &Origin, crates: &KitchenSink, path: &PathBuf, markup: &Renderer, always: bool) -> Result<(), failure::Error> {
    let allver = crates.rich_crate_async(origin).await?;
    if !always && !is_useful1(&allver) {
        return Ok(());
    }

    let c = crates.rich_crate_version_async(origin).await?;
    if !always && !is_useful2(&c) {
        return Ok(());
    }

    let mut buf = Vec::new();
    front_end::render_crate_page(&mut buf, &allver, &c, crates, markup).await?;
    fs::write(&path, buf)?;
    println!("{} http://localhost:3000/crates/{}", path.display(), c.short_name());
    Ok(())
}

async fn run(filter: Option<String>) -> Result<(), failure::Error> {
    let crates = Arc::new(kitchen_sink::KitchenSink::new_default().await?);
    let image_filter = Arc::new(ImageOptimAPIFilter::new("czjpqfbdkz", crates.main_cache_dir().join("images.db")).await?);
    let markup = &Renderer::new_filter(Some(Highlighter::new()), image_filter);

    let tmp;
    let always_render = filter.is_some();
    let all_crates = if let Some(filter) = &filter {
        tmp = vec![if filter.contains(':') {
            Origin::from_str(filter)
        } else {
            Origin::from_crates_io_name(filter)
        }];
        Either::Left(tmp.into_iter())
    } else {
        Either::Right(crates.all_crates())
    };
    for origin in all_crates {
        if stopped() {
            break;
        }
        let path = PathBuf::from(format!("public/crates/{}.html", origin.short_crate_name()));
        if let Err(e) = render(&origin, &crates, &path, markup, always_render).await {
            eprintln!("••• error: {} — {}", e, path.display());
            for c in e.iter_chain().skip(1) {
                eprintln!("•   error: -- {}", c);
            }
            if path.exists() {
                std::fs::remove_file(path).ok();
            }
        }
    }
    Ok(())
}
