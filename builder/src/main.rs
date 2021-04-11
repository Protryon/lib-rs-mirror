use std::sync::Arc;
use parking_lot::Mutex;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::collections::BTreeMap;

// sudo rm -rf /var/tmp/crates_env/target/*/debug/{.fingerprint,.cargo-lock,incremental}; sudo chown -R 4321:4321 /var/tmp/crates_env/; sudo chmod -R a+rwX /var/tmp/crates_env/
const DOCKERFILE: &[u8] = include_bytes!("../docker/Dockerfile");
const TEMP_JUNK_DIR: &str = "/var/tmp/crates_env";

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
        if all.versions().len() < 3 {
            continue; // junk?
        }

        // let's do leaf crates first
        if all.latest_version().dependencies().len() > 2 {
            continue;
        }
        if let Err(e) = analyze_crate(&all, &db, &crates, &docker_root) {
            eprintln!("•• {}: {}", all.name(), e);
            continue;
        }
    }
    Ok(())
}

fn analyze_crate(all: &CratesIndexCrate, db: &BuildDb, crates: &KitchenSink, docker_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let origin = &Origin::from_crates_io_name(all.name());

    let compat_info = db.get_compat(origin)?;
    // look for missing 1.41 tests (ignoring the "probably" ones that aren't authoritative)
    if compat_info.iter().any(|c| c.compat != Compat::ProbablyWorks && c.crate_version.minor == 41) {
        println!("{} got it {:?}", all.name(), compat_info);
        return Ok(());
    }

    println!("checking {}", all.name());
    let ver = all.latest_version();

    let (stdout, stderr) = do_builds(&crates, &all, &docker_root)?;
    println!("{}\n{}\n", stdout, stderr);
    db.set_raw_build_info(origin, ver.version(), "check", &stdout, &stderr)?;

    for f in parse_analyses(&stdout, &stderr) {
        println!("f={:#?}", f);
        if let Some(rustc_version) = f.rustc_version {
            for (rustc_override, name, version, compat) in f.crates {
                let rustc_version = rustc_override.unwrap_or(&rustc_version);
                db.set_compat(&Origin::from_crates_io_name(&name), &version, rustc_version, compat)?;
            }
        }
    }
    Ok(())
}

fn prepare_docker(docker_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    for &p in &["git","registry","target"] {
        let p = Path::new(TEMP_JUNK_DIR).join(p);
        let _ = std::fs::create_dir_all(&p);
    }
    let _ = Command::new("chmod").arg("-R").arg("a+rwX").arg(TEMP_JUNK_DIR).status()?;
    let _ = Command::new("chown").arg("-R").arg("4321:4321").arg(TEMP_JUNK_DIR).status()?;

    let mut child = Command::new("docker")
        .current_dir(docker_root)
        .arg("build")
        .arg("-t").arg("rustesting2")
        .arg("-")
        .stdin(Stdio::piped())
        .spawn()?;

    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(DOCKERFILE)?;
    drop(stdin);

    let res = child.wait()?;
    if !res.success() {
        Err("failed build")?;
    }
    Ok(())
}

fn do_builds(_crates: &KitchenSink, all: &CratesIndexCrate, docker_root: &Path) -> Result<(String, String), Box<dyn std::error::Error>> {
    let mut versions = BTreeMap::new();
    let tmp: Vec<_> = all.versions().iter().filter(|v| !v.is_yanked()).take(200).filter_map(|v| SemVer::parse(v.version()).ok()).collect();
    for ver in tmp.iter() {
        let unstable = ver.major == 0;
        let major = if unstable { ver.minor } else { ver.major };
        versions.insert((unstable, major), ver); // later wins
    }

    let script = format!(r##"
        set -euo pipefail
        rustup default 1.36.0;
        export CARGO_TARGET_DIR=/home/rustyuser/cargo_target/1.36.0;
        for libver in {lib_versions}; do
            (
                mkdir -p "crate-$libver"/src;
                cd "crate-$libver";
                touch src/lib.rs;
                printf > Cargo.toml '[package]\nname="_____"\nversion="0.0.0"\n[profile.dev]\ndebug=false\n[dependencies]\n{crate_name} = "=%s"\n' "$libver";
                timeout 90 cargo fetch;
            ) &
        done
        wait
        for libver in {lib_versions}; do
            for rustver in {rustc_versions}; do
                (
                    cd "crate-$libver";
                    echo "{divider}"
                    echo >&2 "{divider}"
                    echo "CHECKING $rustver {crate_name} $libver"
                    rustup default $rustver;
                    export CARGO_TARGET_DIR=/home/rustyuser/cargo_target/$rustver;
                    time timeout 300 cargo check --locked --message-format=json;
                ) && {{ exit 99; }} || true # stop as soon as it succeeds
            done
        done
    "##,
        divider = parse::DIVIDER,
        lib_versions = versions.values().rev().take(10).rev().map(|v| format!("\"{}\"", v)).collect::<Vec<_>>().join(" "),
        crate_name = all.name(),
        rustc_versions = "1.29.2 1.36.0 1.41.1 1.51.0",
    );

    eprintln!("running {}", script);

    let mut child = Command::new("docker")
        .current_dir(docker_root)
        .arg("run")
        .arg("--rm")
        .arg("-v").arg(format!("{}/git:/home/rustyuser/.cargo/git", TEMP_JUNK_DIR))
        .arg("-v").arg(format!("{}/registry:/home/rustyuser/.cargo/registry", TEMP_JUNK_DIR))
        .arg("-v").arg(format!("{}/target:/home/rustyuser/cargo_target", TEMP_JUNK_DIR))
        .arg("-e").arg("CARGO_INCREMENTAL=0")
        .arg("-m2000m")
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
            if line.len() > 130 {
                if line.is_char_boundary(130) {
                    line.truncate(130);
                }
            }
            println!("{}: {}", prefix, line);
        }
    });
    out2
}
