use std::collections::HashMap;

use crate_db::builddb::*;
mod parse;

use kitchen_sink::*;
use parse::*;

use std::path::Path;
use std::process::Command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let crates = kitchen_sink::KitchenSink::new_default()?;

    let db = BuildDb::new(crates.main_cache_dir().join("builds.db"))?;
    let docker_root = crates.main_cache_dir().join("docker");
    prepare_docker(&docker_root)?;

    let filter = std::env::args().skip(1).next();

    for (_, all) in crates.all_crates_io_crates() {
        if stopped() {
            break;
        }
        if let Some(f) = &filter {
            if !all.name().contains(f) {
                continue;
            }
        }
        if let Err(e) = analyze_crate(&all, &db, &crates, &docker_root) {
            eprintln!("•• {}: {}", all.name(), e);
            continue;
        }
    }
    Ok(())
}

fn analyze_crate(all: &CratesIndexCrate, db: &BuildDb, crates: &KitchenSink, docker_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let ref origin = Origin::from_crates_io_name(all.name());

    let compat_info = db.get_compat(origin)?;
    if compat_info.iter().any(|c| c.crate_version == "1.24.1") {
        println!("{} got it {:?}", all.name(), compat_info);
        return Ok(());
    }

    println!("checking {}", all.name());
    let ver = all.latest_version();

    let (stdout, stderr) = do_builds(&crates, &all, &docker_root)?;
    println!("{}\n{}\n", stdout, stderr);
    db.set_raw_build_info(origin, ver.version(), &stdout, &stderr)?;

    for f in parse_analyses(&stdout, &stderr) {
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
    let mut versions = HashMap::new();
    for ver in all.versions().iter().filter(|v| !v.is_yanked()).filter_map(|v| SemVer::parse(v.version()).ok()) {
        let unstable = ver.major == 0;
        let major = if unstable {ver.minor} else {ver.major};
        versions.insert((unstable, major), ver); // later wins
    }

    let mut cmd = Command::new("docker");
    cmd
        .current_dir(docker_root)
        .arg("run")
        .arg("--rm")
        .arg("-m1500m")
        .arg("testing1")
        .arg("/tmp/run-crate-tests.sh");
    for ver in versions.values().take(15) {
        cmd.arg(format!("{}=\"{}\"\n", all.name(), ver));
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
