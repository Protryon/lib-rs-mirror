use std::sync::Arc;
use parking_lot::Mutex;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::collections::BTreeMap;

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
RUN rustup toolchain add 1.39.0
RUN rustup toolchain add 1.43.0
RUN rustup toolchain add 1.50.0
RUN rustup toolchain list
RUN cargo install --git https://gitlab.com/kornelski/LTS lts; cargo lts 2020-01-01
"##;

const TEMP_JUNK_DIR: &str = "/var/tmp/crates_env";

const RUST_VERSIONS: [&str; 3] = [
    // "1.29.2",
    // "1.36.0",
    "1.39.0",
    "1.43.0",
    // "1.47.0",
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

    let ver = all.latest_version();
    for (_, stdout, stderr) in db.get_raw_build_info(origin, ver.version())? {
        println!("already done {}, but redoing anyway", all.name());
        for f in parse_analyses(&stdout, &stderr) {
            if let Some(rustc_version) = f.rustc_version {
                for (rustc_override, name, version, compat) in f.crates {
                    let rustc_version = rustc_override.unwrap_or(&rustc_version);
                    db.set_compat(&Origin::from_crates_io_name(&name), &version, rustc_version, compat)?;
                }
            }
        }
    }
    println!("checking {}", all.name());

    let mut versions = BTreeMap::new();
    let tmp: Vec<_> = all.versions().iter().rev().take(30)
        .filter(|v| !v.is_yanked())
        .filter_map(|v| SemVer::parse(v.version()).ok())
        .filter(|v| compat_info.get(v).is_none())
        .collect();
    for ver in tmp.into_iter().rev() {
        let unstable = ver.major == 0;
        let major = if unstable { ver.minor } else { ver.major };
        versions.insert((unstable, major), ver); // later wins
    }
    let versions: Vec<SemVer> = versions.into_iter().rev().map(|(_,v)| v).take(4).rev().collect();
    if versions.is_empty() {
        return Ok(());
    }

    let (stdout, stderr) = do_builds(&crates, &all, &docker_root, &versions)?;
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
    // let _ = Command::new("chmod").arg("-R").arg("a+rwX").arg(TEMP_JUNK_DIR).status()?;
    // let _ = Command::new("chown").arg("-R").arg("4321:4321").arg(TEMP_JUNK_DIR).status()?;

    let mut child = Command::new("docker")
        .current_dir(docker_root)
        .arg("build")
        .arg("-t").arg("rustesting2")
        .arg("-")
        .stdin(Stdio::piped())
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

fn do_builds(_crates: &KitchenSink, all: &CratesIndexCrate, docker_root: &Path, versions: &[SemVer]) -> Result<(String, String), Box<dyn std::error::Error>> {
    let script = format!(r##"
        set -euo pipefail
        rustup default {rustc_old_version};
        export CARGO_TARGET_DIR=/home/rustyuser/cargo_target/{rustc_old_version};
        for libver in {lib_versions}; do
            (
                mkdir -p "crate-$libver"/src;
                cd "crate-$libver";
                touch src/lib.rs;
                printf > Cargo.toml '[package]\nname="_____"\nversion="0.0.0"\n[profile.dev]\ndebug=false\n[dependencies]\n{crate_name} = "=%s"\n' "$libver";
                timeout 60 cargo fetch;
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
                    time timeout 200 cargo check --locked --message-format=json;
                ) && {{
                    if [ "$rustver" = "{rustc_last_version}" ]; then
                        exit 1; # all rusts failed, give up trying older lib versions
                    fi
                    break;
                }} || true # stop as soon as it succeeds
            done
        done
    "##,
        divider = parse::DIVIDER,
        lib_versions = versions.iter().map(|v| format!("\"{}\"", v)).collect::<Vec<_>>().join(" "),
        crate_name = all.name(),
        rustc_versions = RUST_VERSIONS.join(" "),
        rustc_old_version = RUST_VERSIONS[0],
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
        .arg("-e").arg("CARGO_INCREMENTAL=0")
        // skip native compilation
        .arg("-e").arg("CC=true")
        .arg("-e").arg("CCXX=true")
        .arg("-e").arg("AR=true")
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
