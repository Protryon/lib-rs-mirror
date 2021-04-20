use parking_lot::Mutex;
use rand::seq::SliceRandom;
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
RUN rustup toolchain add 1.28.0
RUN rustup toolchain add 1.31.0
RUN rustup toolchain add 1.33.0
RUN rustup toolchain add 1.36.0
RUN rustup toolchain add 1.39.0
RUN rustup toolchain add 1.42.0
RUN rustup toolchain add 1.45.0
RUN rustup toolchain add 1.47.0
RUN rustup toolchain add 1.50.0
RUN rustup toolchain add 1.26.0
RUN rustup toolchain add 1.23.0
RUN rustup toolchain add 1.19.0
RUN rustup toolchain list
"##;

const TEMP_JUNK_DIR: &str = "/var/tmp/crates_env";

const RUST_VERSIONS: [&str; 12] = [
    "1.19.0",
    "1.23.0",
    "1.26.0",
    "1.28.0",
    "1.31.0",
    "1.33.0",
    "1.36.0",
    "1.39.0",
    "1.42.0",
    "1.45.0",
    "1.47.0",
    "1.50.0",
];

use crate_db::builddb::*;
mod parse;

use kitchen_sink::*;
use parse::*;

use std::path::Path;
use std::process::Command;
use std::process::Stdio;


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let crates = kitchen_sink::KitchenSink::new_default().await?;

    let db = BuildDb::new(crates.main_cache_dir().join("builds.db"))?;
    let docker_root = crates.main_cache_dir().join("docker");
    eprintln!("starting…");
    prepare_docker(&docker_root)?;

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
        if all.versions().len() < 10 {
            continue; // junk?
        }

        if let Err(e) = analyze_crate(&all, &db, &crates, &docker_root, filter.is_some()).await {
            eprintln!("•• {}: {}", all.name(), e);
            continue;
        }
    }
    Ok(())
}

async fn analyze_crate(all: &CratesIndexCrate, db: &BuildDb, crates: &KitchenSink, docker_root: &Path, force: bool) -> Result<(), Box<dyn std::error::Error>> {
    let origin = &Origin::from_crates_io_name(all.name());
    let ver = all.latest_version();

    if !force && !db.get_raw_build_info(origin, ver.version())?.is_empty() {
        println!("already done {}", all.name());
        return Ok(())
    }
    println!("checking https://lib.rs/compat/{}", all.name());

    let mut compat_info = crates.rustc_compatibility(&crates.rich_crate_async(&origin).await?).await.map_err(|_| "rustc_compatibility")?;
    let mut candidates: Vec<_> = all.versions().iter().rev().take(100)
        .filter(|v| !v.is_yanked())
        .filter_map(|v| SemVer::parse(v.version()).ok())
        .map(|v| {
            (compat_info.remove(&v).unwrap_or_default(), v)
        })
        .filter(|(compat, _)| {
            // any versions left unchecked?
            compat.oldest_ok.unwrap_or(999) != compat.newest_bad.unwrap_or(0)+1
        })
        .collect();
    // pick versions in random order
    candidates.shuffle(&mut rand::thread_rng());

    let mut available_rust_versions = RUST_VERSIONS.to_vec();
    let versions: Vec<_> = candidates.into_iter().filter_map(|(compat, v)| {
        let max_ver = compat.oldest_ok.unwrap_or(999);
        let min_ver = compat.newest_bad.unwrap_or(0);
        let rustc_idx = available_rust_versions.iter().position(|v| {
            let minor = SemVer::parse(v).unwrap().minor as u16;
            minor > min_ver && minor < max_ver
        })?;
        Some((available_rust_versions.swap_remove(rustc_idx), v))
    }).take(8).collect();

    if versions.is_empty() {
        return Ok(());
    }

    let (stdout, stderr) = do_builds(&crates, &all, &docker_root, &versions)?;
    db.set_raw_build_info(origin, ver.version(), "check", &stdout, &stderr)?;

    let mut to_set = Vec::with_capacity(20);
    for f in parse_analyses(&stdout, &stderr) {
        if let Some(rustc_version) = f.rustc_version {
            for (rustc_override, name, version, compat) in f.crates {
                let rustc_version = rustc_override.unwrap_or(&rustc_version);
                eprintln!("https://lib.rs/compat/{} # {}/{} {:?}", name, version, rustc_version, compat);
                to_set.push((Origin::from_crates_io_name(&name), version, rustc_version.to_string(), compat));
            }
        }
    }
    let tmp = to_set.iter().map(|(o, cv, rv, c)| (o, cv.as_str(), rv.as_str(), *c)).collect::<Vec<(&Origin, &str, &str, Compat)>>();
    db.set_compat_multi(&tmp)?;
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
fn do_builds(_crates: &KitchenSink, all: &CratesIndexCrate, docker_root: &Path, versions: &[(&'static str, SemVer)]) -> Result<(String, String), Box<dyn std::error::Error>> {
    let script = format!(r##"
        set -euo pipefail
        function check_crate_with_rustc() {{
            local rustver="$1"
            local libver="$2"
            echo "CHECKING $rustver {crate_name} $libver"
            mkdir -p "crate-$libver/src";
            cd "crate-$libver";
            touch src/lib.rs
            printf > Cargo.toml '[package]\nname="_____"\nversion="0.0.0"\n[profile.dev]\ndebug=false\n[dependencies]\n{crate_name} = "=%s"\n' "$libver";
            export CARGO_TARGET_DIR=/home/rustyuser/cargo_target/$rustver;
            timeout 20 cargo +$rustver fetch;
            timeout 60 nice cargo +$rustver check --locked --message-format=json;
        }}
        swapoff -a || true
        rustup default {rustc_last_version} >/dev/null;
        for job in {jobs}; do
            (
                check_crate_with_rustc $job > /tmp/"output-$job" 2>/tmp/"outputerr-$job" && echo "# $job {crate_name} done OK" || echo "# $job {crate_name} failed"
            ) &
        done
        wait
        for job in {jobs}; do
            echo "{divider}"
            cat /tmp/"output-$job";
            echo >&2 "{divider}"
            cat >&2 /tmp/"outputerr-$job";
        done
    "##,
        divider = parse::DIVIDER,
        jobs = versions.iter().map(|(rust, v)| format!("\"{} {}\"", rust, v)).collect::<Vec<_>>().join(" "),
        crate_name = all.name(),
        rustc_last_version = RUST_VERSIONS[RUST_VERSIONS.len()-1],
    );

    eprintln!("running {}", script);

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
