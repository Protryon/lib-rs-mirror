use futures::StreamExt;
use kitchen_sink::stopped;
use kitchen_sink::KitchenSink;
use kitchen_sink::Origin;
use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::Write;

#[tokio::main]
async fn main() {
    let filter = &std::env::args().nth(1);

    println!("# minimum supported rust versions for crates on https://lib.rs");
    println!("# lines starting with # are comments");
    println!("# the format is: 'crate name' 'rustc version'<='newest stable crate version that works with it'");
    println!("# or: 'crate name' !'remove all crate versions matching this semver expression'");

    let crates = &KitchenSink::new_default().await.unwrap();

    println!("backtrace !<=0.1.8");
    println!("backtrace !=0.2.2");
    println!("backtrace !=0.2.3");
    println!("gcc !<=0.3.0");
    println!("lazy_static !<=0.1.0");
    println!("libc !^0.1.0");
    println!("mio !<=0.3.7");
    println!("mio !=0.6.0");
    println!("nix !=0.5.0");
    println!("num !<=0.1.25");
    println!("pkg-config !<=0.3.2");
    println!("rand !<=0.3.8");
    println!("rustc-serialize !<=0.3.21");
    println!("semver !<=0.1.5");
    println!("tokio-io !<=0.1.2");
    println!("tokio-reactor !<=0.1.0");
    println!("variants !=0.0.1");
    println!("void !<=0.0.4");
    println!("winapi !<=0.1.17");

    futures::stream::iter(crates.all_crates_io_crates().iter())
    .for_each_concurrent(4, |(name, c)| async move {
        if stopped() {
            return;
        }
        if c.versions().len() == 1 {
            return;
        }
        if let Some(f) = filter {
            if !name.contains(f) {
                return;
            }
        }
        if let Err(e) = crate_compat(crates, name).await {
            eprintln!("•• {}: {}", name, e);
        }
    }).await;
}

async fn crate_compat(crates: &KitchenSink, name: &str) -> kitchen_sink::CResult<()> {
    let all = crates.rich_crate_async(&Origin::from_crates_io_name(name)).await?;
    let compat = crates.rustc_compatibility(&all).await?;

    // if the raw ones are all missing, then it has never been tested with cargo check
    // so all the data is assumed based on release dates
    let no_real_data_collected = compat.values()
        .all(|c| c.oldest_ok_raw.is_none() && c.newest_bad_raw.is_none());

    // TODO: check versions from oldest to find very broken crates

    let mut by_rustc = BTreeMap::new();
    let mut prev_rustc_ver = 9999;
    // iterate from newest, propagating newest_bad down
    for (ver, c) in compat.into_iter().rev() {
        // ugh, how to support these!?
        if ver.is_prerelease() {
            continue;
        }

        // no need to list more old versions
        if prev_rustc_ver < 19 {
            break;
        }

        // TODO: this is biased towards keeping versions that aren't sure to work
        // it should probably reject more aggressively?
        let mut rustc_minor_ver = match (c.newest_bad, c.oldest_ok) {
            (Some(bad), Some(ok)) => (bad+1).min(ok), // shouldn't matter, unless the data is bad (it is :)
            (Some(bad), _) => bad+1,
            (_, Some(ok)) => ok,
            _ => continue,
        }.min(prev_rustc_ver);
        prev_rustc_ver = rustc_minor_ver;

        // if lacking data, then be more lenient allowing more crates through
        // assuming they support their current stable + one older version at least
        if no_real_data_collected && c.newest_bad.is_none() {
            rustc_minor_ver = rustc_minor_ver.saturating_sub(1);
        }

        match by_rustc.entry(rustc_minor_ver) {
            Entry::Vacant(e) => {
                e.insert(ver);
            },
            Entry::Occupied(mut e) => if *e.get() < ver {
                e.insert(ver);
            },
        }
    }

    if let Some(&max_rustc) = by_rustc.keys().rev().next() {
        // if the crate is super compatible, don't bother listing its ancient versions
        if max_rustc > 20 {
            let mut out = String::with_capacity(100);
            out.push_str(name);
            for (rustc_minor_ver, crate_ver) in by_rustc {
                write!(&mut out, " 1.{}<={}.{}.{}", rustc_minor_ver,
                    crate_ver.major, // don't print prerelease and +garbage
                    crate_ver.minor,
                    crate_ver.patch,
                ).unwrap();
            }
            out.push('\n');
            std::io::stdout().lock().write_all(out.as_bytes()).unwrap();
        }
    }
    Ok(())
}
