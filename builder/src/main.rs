
use rand::thread_rng;
use rand::seq::SliceRandom;

mod db;
mod parse;



use kitchen_sink::*;
use parse::*;

use std::path::Path;
use std::process::Command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let crates = kitchen_sink::KitchenSink::new_default()?;

    let db = db::BuildDb::new(crates.main_cache_dir().join("builds.db"))?;
    let docker_root = crates.main_cache_dir().join("docker");
    prepare_docker(&docker_root)?;

    for (_, all) in crates.all_crates_io_crates() {
        if stopped() {
            break;
        }
        if let Err(e) = analyze_crate(&all, &db, &crates, &docker_root) {
            eprintln!("•• {}: {}", all.name(), e);
            continue;
        }
    }
    Ok(())
}

fn analyze_crate(all: &CratesIndexCrate, db: &db::BuildDb, crates: &KitchenSink, docker_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let ref origin = Origin::from_crates_io_name(all.name());
    let ver = all.latest_version();

    let compat_info = db.get_compat(origin, ver.version())?;
    if !compat_info.is_empty() {
        println!("{} got it {:?}", ver.name(), compat_info);
        return Ok(());
    }

    let res = db.get_raw_build_info(origin, ver.version())?;
    let builds = match res {
        Some(res) => res,
        None => {
            let (stdout, stderr) = do_builds(&crates, &all, &docker_root)?;
            db.set_raw_build_info(origin, ver.version(), &stdout, &stderr)?;
            (stdout, stderr)
        },
    };
    for f in parse_analyses(&builds.0, &builds.1) {
        println!("{:#?}", f);
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

    let res = Command::new("docker")
        .current_dir(docker_root)
        .arg("build")
        .arg("-t").arg("testing1")
        .arg(".")
        .status()?;

    if !res.success() {
        Err("failed build")?;
    }
    Ok(())
}

fn do_builds(_crates: &KitchenSink, all: &CratesIndexCrate, docker_root: &Path) -> Result<(String, String), Box<dyn std::error::Error>> {

    let mut rng = thread_rng();
    let ver = all.versions();
    let versions = ver.choose_multiple(&mut rng, (ver.len()/3).max(1).min(8));

    let mut cmd = Command::new("docker");
    cmd
        .current_dir(docker_root)
        .arg("run")
        .arg("--rm")
        .arg("-m1500m")
        .arg("testing1")
        .arg("/tmp/run-crate-tests.sh");
    for ver in versions {
        cmd.arg(format!("{}=\"{}\"\n", all.name(), ver.version()));
    }
    let out = cmd
        .output()?;

    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let mut stderr = String::from_utf8_lossy(&out.stderr).into_owned();

    if !out.status.success() {
        stderr += "\nexit failure\n";
    }

    Ok((stdout, stderr))
}
