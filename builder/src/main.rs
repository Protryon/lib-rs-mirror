mod db;
mod parse;

use chrono::prelude::*;
use crate::db::Compat;
use kitchen_sink::*;
use parse::*;
use std::fs;
use std::path::Path;
use std::process::Command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let crates = kitchen_sink::KitchenSink::new_default()?;

    let db = db::BuildDb::new(crates.main_cache_dir().join("builds.db"))?;
    let docker_root = crates.main_cache_dir().join("docker");
    let tarball_reldir = Path::new("tarballs-tmp");
    let tarball_absdir = docker_root.join(tarball_reldir);
    let _ = fs::create_dir_all(&tarball_absdir);

    for (name, _) in crates.all_crates_io_crates() {
        if stopped() {
            break;
        }
        let origin = Origin::from_crates_io_name(name);
        if let Err(e) = analyze_crate(&origin, &db, &crates, &docker_root, tarball_reldir, &tarball_absdir) {
            eprintln!("•• {}: {}", name, e);
            continue;
        }
    }
    Ok(())
}

fn analyze_crate(origin: &Origin, db: &db::BuildDb, crates: &KitchenSink, docker_root: &Path, tarball_reldir: &Path, tarball_absdir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let all = crates.rich_crate(origin)?;
    let k = crates.rich_crate_version(origin)?;

    if k.edition() == Edition::E2018 {
        db.set_compat(k.origin(), k.version(), "1.30.1", Compat::Incompatible)?;
        return Ok(());
    }

    let compat_info = db.get_compat(k.origin(), k.version())?;
    if !compat_info.is_empty() {
        println!("got it {:?}", compat_info);
        return Ok(());
    }

    let (d1,d2,d3) = k.direct_dependencies()?;
    if d1.len() + d2.len() + d3.len() > 4 {
        eprintln!("Too many deps {}", k.short_name());
        return Ok(());
    }

    let res = db.get_raw_build_info(origin, k.version())?;
    let builds = match res {
        Some(res) => res,
        None => {
            let (stdout, stderr) = do_builds(&crates, &all, &k, &tarball_reldir, &tarball_absdir, &docker_root)?;
            db.set_raw_build_info(origin, k.version(), &stdout, &stderr)?;
            (stdout, stderr)
        },
    };
    for f in parse_analyses(&builds.0, &builds.1) {
        if let Some(rustc_version) = f.rustc_version {
            for (name, version, compat) in f.crates {
                db.set_compat(&Origin::from_crates_io_name(&name), &version, &rustc_version, compat)?;
            }
        }
    }
    Ok(())
}

fn do_builds(crates: &KitchenSink, all: &RichCrate, k: &RichCrateVersion, tarball_reldir: &Path, tarball_absdir: &Path, docker_root: &Path) -> Result<(String, String), Box<dyn std::error::Error>> {
    let tarball_data = crates.tarball(k.short_name(), k.version())?;
    let filename = format!("crate-{}-{}.tar.gz", k.short_name(), k.version());
    // has to be relative, because docker
    let tarball_relpath = tarball_reldir.join(&filename);
    let tarball_abspath = tarball_absdir.join(&filename);
    fs::write(&tarball_abspath, tarball_data)?;
    scopeguard::guard((), |_| {
        let _ = fs::remove_file(tarball_abspath);
    });

    let version_info = all.versions().iter().find(|v| v.num == k.version()).ok_or("Bad version")?;
    // use cargo-lts to rewind deps to a week after publication of this crate
    // (it can't be exact date, because timezones, plus crates may rely on sibling crates or bugfixes released shortly after)
    let deps_date = DateTime::parse_from_rfc3339(&version_info.created_at)? + chrono::Duration::days(7);
    let deps_cutoff = if deps_date.year() < 2018 {
        "2018-02-27".to_string() // oldest compiler version we test
    } else {
        deps_date.format("%Y-%m-%d").to_string()
    };

    let res = Command::new("docker")
        .current_dir(docker_root)
        .arg("build")
        .arg("--build-arg").arg(format!("crate_tarball={}", tarball_relpath.display()))
        .arg("--build-arg").arg(format!("deps_date={}", deps_cutoff))
        .arg("-t").arg("testing1")
        .arg(".")
        .status()?;

    if !res.success() {
        Err("failed build")?;
    }

    let out = Command::new("docker")
        .current_dir(docker_root)
        .arg("run")
        .arg("--rm")
        .arg("-m1500m")
        .arg("testing1")
        .arg("/tmp/run-crate-tests.sh")
        .output()?;

    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let mut stderr = String::from_utf8_lossy(&out.stderr).into_owned();

    if !out.status.success() {
        stderr += "\nexit failure\n";
    }

    Ok((stdout, stderr))
}
