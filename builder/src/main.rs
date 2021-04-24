use parking_lot::Mutex;
use rand::Rng;
use rand::seq::SliceRandom;
use std::collections::BTreeSet;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::sync::Arc;

// sudo rm -rf /var/tmp/crates_env/target/*/debug/{.fingerprint,.cargo-lock,incremental}; sudo chown -R 4321:4321 /var/tmp/crates_env/; sudo chmod -R a+rwX /var/tmp/crates_env/
const DOCKERFILE: &str = r##"
FROM rustops/crates-build-env
RUN useradd -u 4321 --create-home --user-group -s /bin/bash rustyuser
RUN chown -R 4321:4321 /home/rustyuser
USER rustyuser
WORKDIR /home/rustyuser
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain 1.51.0 --verbose # wat
ENV PATH="$PATH:/home/rustyuser/.cargo/bin"
RUN rustup set profile minimal
RUN rustup toolchain add 1.25.0
RUN rustup toolchain add 1.29.0
RUN rustup toolchain add 1.30.0
RUN rustup toolchain add 1.34.0
RUN rustup toolchain add 1.38.0
RUN rustup toolchain add 1.40.0
RUN rustup toolchain add 1.41.0
RUN rustup toolchain add 1.44.0
RUN rustup toolchain add 1.46.0
RUN rustup toolchain add 1.48.0
RUN rustup toolchain add 1.49.0
RUN rustup toolchain add 1.35.0
RUN rustup toolchain add 1.37.0
RUN rustup toolchain add 1.32.0
RUN rustup toolchain list
"##;

const TEMP_JUNK_DIR: &str = "/var/tmp/crates_env";

const RUST_VERSIONS: [&str; 14] = [
    "1.25.0",
    "1.29.0",
    "1.30.0",
    "1.34.0",
    "1.38.0",
    "1.40.0",
    "1.41.0",
    "1.44.0",
    "1.46.0",
    "1.48.0",
    "1.49.0",
    "1.35.0",
    "1.37.0",
    "1.32.0",
];

use crate_db::builddb::*;
mod parse;

use kitchen_sink::*;
use parse::*;

use std::path::Path;
use std::process::Command;
use std::process::Stdio;

struct ToCheck {
    score: u32,
    crate_name: Arc<str>,
    ver: SemVer,
    rustc: CompatRange,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let crates = Arc::new(kitchen_sink::KitchenSink::new_default().await?);

    let db = BuildDb::new(crates.main_cache_dir().join("builds.db"))?;
    let docker_root = crates.main_cache_dir().join("docker");
    eprintln!("starting…");
    prepare_docker(&docker_root)?;

    let (s, r) = crossbeam_channel::bounded::<Vec<_>>(12);

    let builds = std::thread::spawn(move || {
        let mut candidates: Vec<ToCheck> = Vec::new();
        let mut rng = rand::thread_rng();

        while let Ok(mut next_batch) = r.recv() {
            if stopped() {
                break;
            }

            candidates.append(&mut next_batch);
            for mut tmp in r.try_iter() {
                candidates.append(&mut tmp);
            }

            // biggest gap, then latest ver; best at the end, because pops
            candidates.sort_by(|a,b| a.score.cmp(&b.score).then(a.ver.cmp(&b.ver)));

            let mut available_rust_versions = RUST_VERSIONS.to_vec();
            available_rust_versions.shuffle(&mut rng);

            let max_to_waste_trying = (candidates.len()/3).max(60);
            let versions: Vec<_> = std::iter::from_fn(|| candidates.pop())
            .take(max_to_waste_trying)
            .filter_map(|x| {
                let max_ver = x.rustc.oldest_ok.unwrap_or(999);
                let min_ver = x.rustc.newest_bad.unwrap_or(0);
                let mut possible_rusts = available_rust_versions.iter().enumerate().map(|(i, v)| {
                    (i, v, SemVer::parse(v).unwrap().minor as u16)
                }).filter(|&(_, _, minor)| {
                    minor > min_ver && minor < max_ver
                });
                let rustc_idx = if x.rustc.newest_bad.is_none() {
                    possible_rusts.max_by_key(|&(_, _, minor)| minor)? // avoid building 2018 with oldest compiler
                } else {
                    possible_rusts.next()?
                }.0;
                Some((available_rust_versions.swap_remove(rustc_idx), x.crate_name, x.ver))
            })
            .take(RUST_VERSIONS.len()-1)
            .collect();

            eprintln!("running: {}/{}", versions.len(), candidates.len());

            if candidates.len() > 2000 {
                candidates.drain(..candidates.len()/2);
            }

            if let Err(e) = run_and_analyze_versions(&db, &docker_root, &versions) {
                eprintln!("•• {}: {}", versions[0].1, e);
            }

        }
    });

    let filter = std::env::args().nth(1);

    for all in crates.all_crates_io_crates().values() {
        if stopped() {
            break;
        }
        if let Some(f) = &filter {
            if !all.name().contains(f) {
                continue;
            }
        }
        if all.versions().len() < 3 {
            continue; // junk?
        }

        match find_versions_to_build(&all, &crates).await {
            Ok(vers) => {
                s.send(vers).unwrap();
            },
            Err(e) => {
                eprintln!("•• {}: {}", all.name(), e);
                continue;
            }
        }
    }
    drop(s);
    builds.join().unwrap();
    Ok(())
}

async fn find_versions_to_build(all: &CratesIndexCrate, crates: &KitchenSink) -> Result<Vec<ToCheck>, Box<dyn std::error::Error>> {
    let crate_name: Arc<str> = all.name().into();
    let origin = &Origin::from_crates_io_name(&crate_name);

    println!("checking https://lib.rs/compat/{}", crate_name);

    let mut compat_info = crates.rustc_compatibility(&crates.rich_crate_async(&origin).await?).await.map_err(|_| "rustc_compatibility")?;

    let has_anything_built_ok_yet = compat_info.values().any(|c| c.oldest_ok_raw.is_some());
    let mut rng = rand::thread_rng();
    let mut candidates: Vec<_> = all.versions().iter().rev().take(100)
        .filter(|v| !v.is_yanked())
        .filter_map(|v| SemVer::parse(v.version()).ok())
        .map(|v| {
            let c = compat_info.remove(&v).unwrap_or_default();
            (v, c)
        })
        .filter(|(_, c)| c.oldest_ok.unwrap_or(999) > 29) // old crates, don't bother
        .map(|(v, mut compat)| {
            let has_ever_built = compat.oldest_ok_raw.is_some();
            let has_failed = compat.newest_bad_raw.is_some();
            let no_compat_bottom = compat.newest_bad.is_none();
            let oldest_ok = compat.oldest_ok.unwrap_or(999);
            let newest_bad = compat.newest_bad.unwrap_or(0).max(19); // we don't test rust < 19
            let actual_newest_bad = compat.newest_bad_raw.unwrap_or(0);
            let gap = oldest_ok.saturating_sub(newest_bad) as u32; // unknown version gap
            let score = rng.gen_range(0..if has_anything_built_ok_yet { 3 } else { 20 }) // spice it up a bit
                + if gap > 4 { gap * 2 } else { gap }
                + if !has_ever_built && has_failed && has_anything_built_ok_yet { 15 } else { 0 } // unusable data? try fixing first
                + if has_ever_built { 0 } else { 1 } // move to better ones
                + if newest_bad < compat.newest_bad_raw.unwrap_or(0) { 5 } else { 0 } // really unusable data
                + if compat.oldest_ok_raw.unwrap_or(999) > oldest_ok { 2 } else { 0 } // haven't really checked min ver yet
                + if newest_bad > 30 { 1 } else { 0 } // don't want to check old crap
                + if no_compat_bottom { 2 } else { 0 } // don't want to show 1.0 as compat rust
                + if oldest_ok  > 30 { 1 } else { 0 }
                + if oldest_ok  > 40 { 2 } else { 0 }
                + if oldest_ok  > 50 { 3 } else { 0 };

            if !has_ever_built && has_failed {
                // compat.oldest_ok = None; // build it with some new version
                compat.newest_bad = Some(newest_bad.max(actual_newest_bad)); // don't retry old bad
            }

            ToCheck {
                score,
                ver: v,
                rustc: compat,
                crate_name: crate_name.clone()
            }
        })
        .filter(|c| {
            c.rustc.oldest_ok.unwrap_or(999) > 1+c.rustc.newest_bad.unwrap_or(0)
        })
        .collect();


    let dl = if candidates.is_empty() { 0 } else { crates.downloads_per_month(origin).await.unwrap_or(Some(0)).unwrap_or(0) };
    let popularity_factor = (dl.max(100) as f32).sqrt() / 10.0;

    for c in candidates.iter_mut() {
        c.score = (c.score as f32 * popularity_factor) as u32;
    }

    // biggest gap, then latest ver
    candidates.sort_unstable_by(|a,b| b.score.cmp(&a.score).then(b.ver.cmp(&a.ver)));

    for c in &candidates {
        println!("{} {}\t^{}\t{}~{} r{}~{}", crate_name, c.ver, c.score,
            c.rustc.oldest_ok.unwrap_or(0), c.rustc.newest_bad.unwrap_or(0),
            c.rustc.oldest_ok_raw.unwrap_or(0), c.rustc.newest_bad_raw.unwrap_or(0));
    }

    if !has_anything_built_ok_yet {
        candidates.truncate(2); // don't waste time on broken crates
    }

    Ok(candidates)
}


fn run_and_analyze_versions(db: &BuildDb, docker_root: &Path, versions: &[(&'static str, Arc<str>, SemVer)]) -> Result<(), Box<dyn std::error::Error>> {
    if versions.is_empty() {
        return Ok(());
    }

    let (stdout, stderr) = do_builds(&docker_root, &versions)?;

    let mut to_set = BTreeSet::new();
    for f in parse_analyses(&stdout, &stderr) {
        if let Some(rustc_version) = f.rustc_version {
            for (rustc_override, name, version, compat) in f.crates {
                let rustc_version = rustc_override.unwrap_or(&rustc_version);
                to_set.insert((compat, rustc_version.to_string(), Origin::from_crates_io_name(&name), version));
            }
        }
    }

    let tmp = to_set.iter().map(|(c, rv, o, cv)| {
        eprintln!("https://lib.rs/compat/{}#{} R.{}={:?}", o.short_crate_name(), cv, rv, c);
        (o, cv.as_str(), rv.as_str(), *c)
    }).collect::<Vec<(&Origin, &str, &str, Compat)>>();
    if let Err(_) = db.set_compat_multi(&tmp) {
        // retry, sqlite is flaky
        db.set_compat_multi(&tmp)?;
    }
    Ok(())
}

fn prepare_docker(docker_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    for &p in &["git","registry","target"] {
        let p = Path::new(TEMP_JUNK_DIR).join(p);
        let _ = std::fs::create_dir_all(&p);
    }
    // let _ = Command::new("chmod").arg("-R").arg("a+rwX").arg(TEMP_JUNK_DIR).status()?;
    // let _ = Command::new("chown").arg("-R").arg("4321:4321").arg(TEMP_JUNK_DIR).status()?;

    let mut child = Command::new("docker")
        .current_dir(docker_root)
        .arg("build")
        .arg("-t").arg("rustesting2")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?;

    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(DOCKERFILE.as_bytes())?;
    drop(stdin);

    let res = child.wait()?;
    if !res.success() {
        Err("failed build")?;
    }
    Ok(())
}

// versions is (rustc version, crate version)
fn do_builds(docker_root: &Path, versions: &[(&'static str, Arc<str>, SemVer)]) -> Result<(String, String), Box<dyn std::error::Error>> {
    let script = format!(r##"
        set -euo pipefail
        function yeet {{
            mv "$1" "$1-delete" && rm -rf "$1-delete" # atomic delete
        }}
        function cleanup() {{
            local rustver="$1"
            local crate_name="$2"
            local libver="$3"
            export CARGO_TARGET_DIR=/home/rustyuser/cargo_target/$rustver;
            rm -rf "$CARGO_TARGET_DIR"/debug/*/"$crate_name-"*; # eats disk space
            rm -rf "$CARGO_TARGET_DIR"/debug/*/"lib$crate_name-"*;
            yeet ~/.cargo/registry/src/*/"$crate_name-$libver";
        }}
        function check_crate_with_rustc() {{
            local rustver="$1"
            local crate_name="$2"
            local libver="$3"
            echo "CHECKING $rustver $crate_name $libver"
            mkdir -p "crate-$libver/src";
            cd "crate-$libver";
            touch src/lib.rs
            printf > Cargo.toml '[package]\nname="_____"\nversion="0.0.0"\n[profile.dev]\ndebug=false\n[dependencies]\n%s = "=%s"\n' "$crate_name" "$libver";
            export CARGO_TARGET_DIR=/home/rustyuser/cargo_target/$rustver;
            timeout 40 cargo +$rustver fetch; # RUSTC_BOOTSTRAP=1 timeout 40 cargo +$rustver -Z minimal-versions generate-lockfile ||
            timeout 30 nice cargo +$rustver check --locked --message-format=json;
        }}
        swapoff -a || true
        for job in {jobs}; do
            (
                check_crate_with_rustc $job > /tmp/"output-$job" 2>/tmp/"outputerr-$job" && echo "# R.$job done OK" || echo "# R.$job failed"
            ) &
        done
        wait
        for job in {jobs}; do
            echo "{divider}"
            cat /tmp/"output-$job";
            echo >&2 "{divider}"
            cat >&2 /tmp/"outputerr-$job";
            cleanup $job
        done
    "##,
        divider = parse::DIVIDER,
        jobs = versions.iter().map(|(rust, c, v)| format!("\"{} {} {}\"", rust, c, v)).collect::<Vec<_>>().join(" "),
    );

    let mut child = Command::new("nice")
        .current_dir(docker_root)
        .arg("docker")
        .arg("run")
        .arg("--rm")
        .arg("-v").arg(format!("{}/git:/home/rustyuser/.cargo/git", TEMP_JUNK_DIR))
        .arg("-v").arg(format!("{}/registry:/home/rustyuser/.cargo/registry", TEMP_JUNK_DIR))
        .arg("-v").arg(format!("{}/target:/home/rustyuser/cargo_target", TEMP_JUNK_DIR))
        // .arg("-e").arg("CARGO_INCREMENTAL=0")
        // skip native compilation
        // .arg("-e").arg("CC=true")
        // .arg("-e").arg("CCXX=true")
        // .arg("-e").arg("AR=true")
        .arg("-e").arg("CARGO_BUILD_JOBS=6")
        .arg("-m2700m")
        //.arg("--cpus=3")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .arg("rustesting2")
        .arg("bash").arg("-c").arg(script)
        .spawn()?;

    let stdout = streamfetch("out", child.stdout.take().unwrap());
    let stderr = streamfetch("err", child.stderr.take().unwrap());

    let status = child.wait()?;

    let stdout = std::mem::take(&mut *stdout.lock());
    let mut stderr = std::mem::take(&mut *stderr.lock());

    if !status.success() {
        stderr += &format!("\nexit failure {:?}\n", status);
    }

    Ok((stdout, stderr))
}

fn streamfetch(prefix: &'static str, inp: impl std::io::Read + Send + 'static) -> Arc<Mutex<String>> {
    let out = Arc::new(Mutex::new(String::new()));
    let out2 = out.clone();
    std::thread::spawn(move || {
        let buf = BufReader::new(inp);
        for line in buf.lines() {
            let mut line = line.unwrap();
            let mut tmp = out.lock();
            tmp.push_str(&line);
            tmp.push('\n');
            if line.len() > 230 {
                if line.is_char_boundary(230) {
                    line.truncate(230);
                }
            }
            println!("{}: {}", prefix, line);
        }
    });
    out2
}
