use std::io::Write;
use futures::StreamExt;
use kitchen_sink::stopped;
use kitchen_sink::KitchenSink;
use kitchen_sink::Origin;
use std::collections::btree_map::Entry;
use std::collections::BTreeMap;


#[tokio::main]
async fn main() {
    println!("# minimum supported rust versions for crates on https://lib.rs");
    println!("# lines starting with # are comments");
    println!("# the format is: 'crate name' 'rustc version'<='newest stable crate version that works with it'");
    println!("# or: 'crate name' !'remove all crate versions matching this semver expression'");

    let crates = KitchenSink::new_default().await.unwrap();

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

    let crates = &crates;
    futures::stream::iter(crates.all_crates_io_crates().iter())
    .for_each_concurrent(4, |(name, c)| async move {
        if stopped() {
            return;
        }
        if c.versions().len() == 1 {
            return;
        }
        if let Err(e) = crate_compat(crates, name).await {
            eprintln!("•• {}: {}", name, e);
        }
    }).await;
}

async fn crate_compat(crates: &KitchenSink, name: &str) -> kitchen_sink::CResult<()> {
    let c = crates.rich_crate_async(&Origin::from_crates_io_name(name)).await?;
    let compat = crates.rustc_compatibility(&c).await?;

    // never built, assume it's our failure to build, not a broken crate
    if !compat.values().any(|c| c.oldest_ok.is_some()) {
        return Ok(());
    }

    // TODO: check versions from oldest to find very broken crates

    let mut by_rustc = BTreeMap::new();
    let mut prev_newest_bad = 9999;
    // iterate from newest, propagating newest_bad down
    for (ver, c) in compat.into_iter().rev() {
        // ugh, how to support these!?
        if ver.is_prerelease() {
            continue;
        }
        let rustc_minor_ver = match (c.newest_bad, c.oldest_ok) {
            (Some(bad), Some(ok)) => (bad+1).min(ok), // shouldn't matter, unless the data is bad (it is :)
            (Some(bad), _) => bad+1,
            (_, Some(ok)) => ok,
            _ => continue,
        }.min(prev_newest_bad);
        prev_newest_bad = rustc_minor_ver;

        // not supported
        if rustc_minor_ver < 19 {
            continue;
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
    if !by_rustc.is_empty() {
        let stdout = std::io::stdout();
        let mut stdout = stdout.lock();
        write!(stdout, "{}", name).unwrap();
        for (rustc_minor_ver, crate_ver) in by_rustc {
            write!(stdout, " 1.{}<={}.{}.{}", rustc_minor_ver,
                crate_ver.major, // don't print prerelease and +garbage
                crate_ver.minor,
                crate_ver.patch,
            ).unwrap();
        }
        writeln!(stdout).unwrap();
    }
    Ok(())
}
