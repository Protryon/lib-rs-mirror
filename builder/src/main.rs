use std::collections::BTreeMap;
use ahash::HashSet;
use ahash::HashSetExt;
use std::path::PathBuf;
use std::time::Duration;
use futures::future::try_join_all;
use log::debug;
use log::error;
use log::info;
use log::warn;
use parking_lot::Mutex;
use rand::Rng;
use rand::seq::SliceRandom;

use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::sync::Arc;

const CONCURRENCY: usize = 3; // builds in parallel

const DOCKER_NAME: &str = "rustesting2";
const DOCKERFILE_DEFAULT_RUSTC: RustcMinorVersion = 61;
const DOCKERFILE_PRELUDE: &str = r##"
FROM rustops/crates-build-env
RUN useradd -u 4321 --create-home --user-group -s /bin/bash rustyuser
RUN chown -R 4321:4321 /home/rustyuser
RUN swapoff -a || true
USER rustyuser
WORKDIR /home/rustyuser
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain 1.61.0 --verbose
ENV PATH="$PATH:/home/rustyuser/.cargo/bin"
RUN rustup set profile minimal
"##; // must have newline!

const TEMP_JUNK_DIR: &str = "/var/tmp/crates_env";

const RUST_VERSIONS: [RustcMinorVersion; 14] = [
    61,
    60,
    59,
    58,
    56,
    55,
    53,
    51,
    48,
    47,
    40,
    38,
    32,
    63,
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
    version: SemVer,
    rustc_compat: CompatRanges,
    is_app: bool,
}

fn out_of_disk_space() -> bool {
    match fs2::available_space(TEMP_JUNK_DIR) {
        Ok(size) => {
            info!("free disk space: {}MB", size / 1_000_000);
            size < 5_000_000_000 // cargo easily chews gigabytes of disk space per build
        },
        Err(e) => {
            error!("disk space check: {}", e);
            true
        },
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let crates = Arc::new(kitchen_sink::KitchenSink::new_default().await?);

    let db = BuildDb::new(crates.main_cache_dir().join("builds.db"))?;
    let docker_root = crates.main_cache_dir().join("docker");
    eprintln!("starting…");
    prepare_docker(&docker_root)?;

    let (s, r) = crossbeam_channel::bounded::<Vec<_>>(200);

    let builds = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(500)); // wait for more data
        let mut candidates: Vec<ToCheck> = Vec::new();
        let mut rng = rand::thread_rng();
        let mut one_crate_version = HashSet::new();

        while let Ok(mut next_batch) = r.recv() {
            std::thread::sleep(Duration::from_millis(500)); // wait for more data
            if stopped() || out_of_disk_space() {
                eprintln!("Stopping early");
                break;
            }

            candidates.append(&mut next_batch);
            for mut tmp in r.try_iter().take(100) {
                candidates.append(&mut tmp);
            }

            // biggest gap, then latest ver; best at the end, because pops
            candidates.sort_unstable_by(|a,b| a.score.cmp(&b.score).then(a.version.cmp(&b.version)));

            let mut available_rust_versions = RUST_VERSIONS.to_vec();
            available_rust_versions.shuffle(&mut rng);


            let max_to_waste_trying = (candidates.len()/2).max(RUST_VERSIONS.len());
            let versions: Vec<_> = std::iter::from_fn(|| candidates.pop())
            .take(max_to_waste_trying)
            .filter_map(|x| {
                let max_ver = x.rustc_compat.oldest_ok_certain().unwrap_or(999);
                let min_ver = x.rustc_compat.newest_bad_likely().unwrap_or(18);

                // cargo install --message-format is v1.58+ :(
                let upper_limit = x.rustc_compat.oldest_ok().unwrap_or(if x.is_app {62} else {55});
                // don't pick 1.29 as the first choice
                let lower_limit = x.rustc_compat.newest_bad().unwrap_or(if x.is_app {58} else {43});

                // min_ver ignores bad deps, but that often leads it to be stuck on last edition
                // which is super wasteful
                let min_ver = min_ver.max((lower_limit.min(max_ver)).saturating_sub(5));
                // same for approx max ver that may come from msrv or binaries
                let max_ver = max_ver.min(upper_limit.max(min_ver) + 10);

                let best_ver = (upper_limit * 5 + lower_limit * 11)/16; // bias towards lower ver, because lower versions see features from newer versions

                let origin = Origin::from_crates_io_name(&x.crate_name);
                let mut existing_info = db.get_compat_raw(&origin).unwrap_or_default();
                existing_info.retain(|inf| inf.crate_version == x.version);

                let possible_rusts = available_rust_versions.iter().enumerate()
                .filter(|&(_, &minor)| {
                    minor > min_ver && minor < max_ver
                })
                .filter(|&(_, &v)| {
                    !existing_info.iter().any(|inf| inf.rustc_version == v)
                });
                let maybe_rustc_idx = if !x.rustc_compat.has_ever_built() {
                    // pick latest to avoid building with a compiler that may not understand new manifest or edition (cargo failures give worse info)
                    possible_rusts.min_by_key(|&(_, &v)| (upper_limit as i32 - v as i32).abs())
                    .map(|(k,v)| { debug!("{}-{} never built, trying latest R.{}", x.crate_name, x.version, v); (k,v) })
                } else {
                    possible_rusts.min_by_key(|&(_, &minor)| ((minor as i32) - best_ver as i32).abs())
                };
                let rustc_idx = match maybe_rustc_idx {
                    Some((idx, _)) => idx,
                    None => {
                        debug!("Can't find rust for {}@{}, because it needs r{}-{}, but only got {:?}", x.crate_name, x.version, min_ver, max_ver, available_rust_versions);
                        return None;
                    },
                };

                // it's better to make multiple passes, and re-evaluate the crate after pass/fail of this version
                if !one_crate_version.insert(x.crate_name.clone()) {
                    return None;
                }

                let rustc_ver = available_rust_versions.swap_remove(rustc_idx);
                let mut required_deps = Vec::new();
                for (c,(v, newest_bad)) in x.rustc_compat.required_deps() {
                    if newest_bad <= rustc_ver { // this check is useless, since newest_bad is for wrong version
                        required_deps.push((c.into(), v.clone()));
                    }
                }

                Some(CrateToRun {
                    rustc_ver,
                    crate_name: x.crate_name,
                    version: x.version,
                    required_deps,
                    is_app: x.is_app,
                })
            })
            .take((RUST_VERSIONS.len() *2/3).min(CONCURRENCY)) // max concurrency
            .collect();

            eprintln!("\nselected: {}/{} ({})", versions.len(), candidates.len(), versions.iter().take(10).map(|c| format!("{} {}", c.crate_name, c.version)).collect::<Vec<_>>().join(", "));

            if candidates.len() > 3000 {
                candidates.drain(..candidates.len()/2);
            }

            if let Err(e) = run_and_analyze_versions(&db, &docker_root, versions) {
                eprintln!("•• {}", e);
            }
        }
        eprintln!("builder end");
    });

    let mut filters: Vec<_> = std::env::args().skip(1).collect();
    let mut do_all = false;
    filters.retain(|v| if v == "--all" { do_all = true; false } else { true });

    let all_crates_map = crates.all_crates_io_crates();
    let mut map_iter;
    let mut recent_iter;
    let all_crates: &mut dyn Iterator<Item = _> = if !do_all {
        let mut recent = crates.notable_recently_updated_crates(5000).await?;
        recent.shuffle(&mut rand::thread_rng());
        recent_iter = recent.into_iter().filter_map(|(o, _)| {
            all_crates_map.get(o.short_crate_name())
        });
        &mut recent_iter
    } else {
        map_iter = all_crates_map.values();
        &mut map_iter
    };

    for all in all_crates {
        if stopped() {
            break;
        }

        if filters.len() == 1 {
            if !all.name().contains(&filters[0]) {
                continue;
            }
        } else if filters.len() > 1 {
            if !filters.iter().any(|f| f == all.name()) {
                continue;
            }
        }

        if all.versions().len() == 1 || all.versions().len() > 500 {
            continue; // junk?
        }

        match find_versions_to_build(all, &crates).await {
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
    eprintln!("sender end");
    builds.join().unwrap();
    eprintln!("bye");
    Ok(())
}

async fn find_versions_to_build(all: &CratesIndexCrate, crates: &KitchenSink) -> Result<Vec<ToCheck>, Box<dyn std::error::Error>> {
    let crate_name: Arc<str> = all.name().into();
    let origin = &Origin::from_crates_io_name(&crate_name);

    let mut compat_info = crates.rustc_compatibility_for_builder(&crates.rich_crate_async(origin).await?).await.map_err(|_| "rustc_compatibility")?;

    let has_anything_built_ok_yet = compat_info.values().any(|c| c.has_ever_built());

    let mut rng = rand::thread_rng();
    let mut candidates: Vec<_> = all.versions().iter().rev() // rev() starts from most recent
        .filter(|v| !v.is_yanked())
        .take(16)
        .filter_map(|v| SemVer::parse(v.version()).ok())
        .map(|v| {
            let c = compat_info.remove(&v).unwrap_or_default();
            (v, c)
        })
        .filter(|(_, c)| c.oldest_ok().unwrap_or(999) > 20) // old crates, don't bother
        .enumerate()
        .map(|(idx, (version, compat))| {
            let has_ever_built = compat.has_ever_built();
            let has_failed = compat.newest_bad_likely().is_some();
            let no_compat_bottom = compat.newest_bad().is_none();
            let oldest_ok = compat.oldest_ok().unwrap_or(999);
            let newest_bad = compat.newest_bad().unwrap_or(0).max(19); // we don't test rust < 19
            let oldest_ok_certain = compat.oldest_ok_certain().unwrap_or(999);
            let newest_bad_certain = compat.newest_bad_likely().unwrap_or(0).max(19); // we don't test rust < 19
            let gap = oldest_ok.saturating_sub(newest_bad) as u32; // unknown version gap
            let gap_certain = oldest_ok_certain.saturating_sub(newest_bad_certain) as u32; // unknown version gap
            let score = rng.gen_range(0..if has_anything_built_ok_yet { 3 } else { 10 }) // spice it up a bit
                + gap_certain.min(10)
                + if gap > 4 { gap * 2 } else { gap }
                + if gap > 10 { gap } else { 0 }
                + if !has_ever_built && has_failed && has_anything_built_ok_yet { 15 } else { 0 } // unusable data? try fixing first
                + if has_ever_built { 0 } else { 2 } // move to better ones
                + if newest_bad < compat.newest_bad_certain().unwrap_or(0) { 5 } else { 0 } // really unusable data
                + if compat.oldest_ok_certain().unwrap_or(999) > oldest_ok { 2 } else { 0 } // haven't really checked min ver yet
                + if newest_bad > 29 { 1 } else { 0 } // don't want to check old crap
                + if no_compat_bottom { 2 } else { 0 } // don't want to show 1.0 as compat rust
                + if oldest_ok  > 35 { 1 } else { 0 }
                + if oldest_ok  > 40 { 4 } else { 0 }
                + if oldest_ok  > 47 { 2 } else { 0 }
                + if oldest_ok  > 50 { 5 } else { 0 }
                + if oldest_ok  > 53 { 8 } else { 0 }
                + if version.pre.is_empty() { 15 } else { 0 } // don't waste time testing alphas
                + if idx == 0 { 10 } else { 0 } // prefer latest
                + 5u32.saturating_sub(idx as u32); // prefer newer

            ToCheck {
                score,
                version,
                rustc_compat: compat,
                crate_name: crate_name.clone(),
                is_app: false,
            }
        })
        .filter(|c| {
            c.rustc_compat.oldest_ok().unwrap_or(999) > 1+c.rustc_compat.newest_bad().unwrap_or(0)
        })
        .collect();


    let popularity_factor = if candidates.is_empty() { 0. } else { crates.crate_ranking_for_builder(origin).await.unwrap_or(0.3) };
    if popularity_factor < 0.2 {
        return Ok(vec![]);
    }

    for c in candidates.iter_mut() {
        c.score = (c.score as f64 * popularity_factor) as u32;
    }

    if has_anything_built_ok_yet {
        // biggest gap, then latest ver
        candidates.sort_unstable_by(|a,b| b.score.cmp(&a.score).then(b.version.cmp(&a.version)));
        candidates.truncate(6);
    } else {
        candidates.shuffle(&mut rng); // dunno which versions will build
        candidates.truncate(3); // don't waste time on broken crates
    }

    try_join_all(candidates.iter_mut().take(3).map(|c| { async move {
        let meta = crates.crate_files_summary_from_crates_io_tarball(&c.crate_name, &c.version.to_string()).await?;
        let msrv = crates.index_msrv_from_manifest(&Origin::from_crates_io_name(&c.crate_name), &meta.manifest)?;
        if msrv > 1 {
            c.rustc_compat.add_compat(msrv - 1, Compat::DefinitelyIncompatible, None);
        }
        if meta.lib_file.is_none() && meta.bin_file.is_some() {
            c.is_app = true;
        }
        Ok::<_, CError>(())
    }})).await?;

    for c in &candidates {
        println!("{} {}\t^{}\tinferred={}~{} checked={}~{}", crate_name, c.version, c.score,
            c.rustc_compat.oldest_ok().unwrap_or(0), c.rustc_compat.newest_bad().unwrap_or(0),
            c.rustc_compat.oldest_ok_certain().unwrap_or(0), c.rustc_compat.newest_bad_likely().unwrap_or(0));
        for (k,(v,r)) in c.rustc_compat.required_deps() {
            println!(" + {}@{} for r{}", k, v, r);
        }
    }

    if candidates.is_empty() {
        println!("{} ???\tno candidates", crate_name);
    }

    Ok(candidates)
}

struct CrateToRun {
    rustc_ver: RustcMinorVersion,
    crate_name: Arc<str>,
    version: SemVer,
    required_deps: Vec<(Box<str>, SemVer)>,
    is_app: bool,
}

fn run_and_analyze_versions(db: &BuildDb, docker_root: &Path, versions: Vec<CrateToRun>) -> Result<(), Box<dyn std::error::Error>> {
    if versions.is_empty() {
        return Ok(());
    }

    // reuse tarballs cached by our crates-io client
    for c in &versions {
        let dest = Path::new(TEMP_JUNK_DIR).join("registry/cache/github.com-1ecc6299db9ec823").join(format!("{}-{}.crate", c.crate_name, c.version));
        if !dest.exists() {
            let src = Path::new("/var/lib/crates-server/tarballs").join(format!("{}/{}.crate", c.crate_name, c.version));
            let _ = std::fs::hard_link(&src, &dest).map_err(|e| warn!("tarball {} -> {}: {}", src.display(), dest.display(), e));
        }
    }

    let (stdout, stderr) = do_builds(docker_root, &versions)?;

    let mut to_set = BTreeMap::new();
    for f in parse_analyses(&stdout, &stderr) {
        if let Some(rustc_version) = f.rustc_version {
            for (rustc_override, crate_name, crate_version, new_compat, reason) in f.crates {
                let origin = Origin::from_crates_io_name(&crate_name);
                let rustc_version = rustc_override.unwrap_or(rustc_version);

                to_set.entry((rustc_version, origin, crate_version, reason))
                    .and_modify(|existing_compat: &mut Compat| {
                        if new_compat.is_better(&existing_compat) {
                            *existing_compat = new_compat;
                        }
                    })
                    .or_insert(new_compat);
            }
        }
    }

    let tmp = to_set.iter().map(|((rv, o, cv, reason), c)| {
        SetCompatMulti { origin: o, ver: cv, rustc_version: *rv, compat: *c, reason: &reason }
    }).collect::<Vec<_>>();
    if db.set_compat_multi(&tmp).is_err() {
        // retry, sqlite is flaky
        std::thread::sleep(Duration::from_secs(1));
        db.set_compat_multi(&tmp)?;
    }
    Ok(())
}

fn prepare_docker(docker_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let _ = std::fs::create_dir_all(&docker_root);

    for &p in &["git","registry","target","job_inputs"] {
        let p = Path::new(TEMP_JUNK_DIR).join(p);
        let _ = std::fs::create_dir_all(&p);
    }
    // let _ = Command::new("chmod").arg("-R").arg("a+rwX").arg(TEMP_JUNK_DIR).status()?;
    // let _ = Command::new("chown").arg("-R").arg("4321:4321").arg(TEMP_JUNK_DIR).status()?;

    let mut child = Command::new("docker")
        .current_dir(docker_root)
        .arg("build")
        .arg("-t").arg(DOCKER_NAME)
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?;

    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(DOCKERFILE_PRELUDE.as_bytes())?;
    for v in RUST_VERSIONS {
        if v != DOCKERFILE_DEFAULT_RUSTC {
            writeln!(stdin, "RUN rustup toolchain add {}", rustc_minor_ver_to_version(v))?;
        }
    }
    stdin.write_all(b"RUN rustup toolchain list\nRUN chmod -R a-w ~/.rustup ~/.cargo/bin ~/.cargo/env\n")?;
    drop(stdin);

    let res = child.wait()?;
    if !res.success() {
        Err("failed build")?;
    }

    let _ = Command::new("docker")
        .current_dir(docker_root)
        .arg("run")
        .arg("--rm")
        .arg("-v").arg(format!("{}/git:/home/rustyuser/.cargo/git", TEMP_JUNK_DIR))
        .arg("-v").arg(format!("{}/registry:/home/rustyuser/.cargo/registry", TEMP_JUNK_DIR))
        .arg(DOCKER_NAME)
        .arg("bash").arg("-c").arg("cargo install libc --vers 99.9.9 --color=always -vv") // force index update
        .status()?;
    Ok(())
}

// must match bash script below
fn job_inputs_dir(root: &Path, c: &CrateToRun) -> PathBuf {
    root.join(format!("crate-{}-{}--{}-job", c.crate_name, c.version, rustc_minor_ver_to_version(c.rustc_ver)))
}

// versions is (rustc version, crate version)
fn do_builds(docker_root: &Path, versions: &[CrateToRun]) -> Result<(String, String), Box<dyn std::error::Error>> {

    let job_inputs_root = Path::new(TEMP_JUNK_DIR).join("job_inputs");
    for c in versions {
        let dir = job_inputs_dir(&job_inputs_root, c);
        let _ = std::fs::create_dir(&dir);

        let mut cargo_toml = format!("[package]\nname=\"_____\"\nversion=\"0.0.0\"\n[profile.dev]\ndebug=false\n[dependencies]\n{} = \"{}\"\n", c.crate_name, c.version);
        for (c, v) in &c.required_deps {
            use std::fmt::Write;
            let _ = writeln!(&mut cargo_toml, "{} = \"<= {}, {}{}\"", c, v, if v.major == 0 {"0."} else {""}, if v.major == 0 { v.minor } else { v.major });
        }
        debug!("{}", cargo_toml);
        std::fs::write(dir.join("Cargo.toml"), cargo_toml)?;
    }

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
            local stdoutfile="$4"
            local stderrfile="$5"
            local job_inputs_dir="/home/rustyuser/job_inputs/crate-$crate_name-$libver--$rustver-job"
            mkdir -p "crate-$crate_name-$libver/src";
            cd "crate-$crate_name-$libver";
            touch src/lib.rs
            cp "$job_inputs_dir/Cargo.toml" Cargo.toml
            export CARGO_TARGET_DIR=/home/rustyuser/cargo_target/$rustver;
            {{
                echo >"$stdoutfile" "CHECKING $rustver $crate_name $libver"
                RUSTC_BOOTSTRAP=1 timeout 20 cargo +"$rustver" -Z no-index-update fetch || CARGO_NET_GIT_FETCH_WITH_CLI=true timeout 60 cargo +"$rustver" fetch --color=always -vv;
                timeout 90 nice cargo +$rustver {check_command} -j3 --locked --message-format=json >>"$stdoutfile" 2>"$stderrfile";
            }} || {{
                local rustfetchver="$rustver"
                if [ "$rustver" == "1.20.0" ]; then
                    rustfetchver="1.25.0"
                fi
                echo >>"$stdoutfile" "CHECKING $rustver $crate_name $libver (minimal-versions)"
                printf > Cargo.toml '[package]\nname="_____"\nversion="0.0.0"\n[profile.dev]\ndebug=false\n[dependencies]\n%s = "=%s"\n[dev-dependencies]\nminimal-versions-are-broken="1"' "$crate_name" "$libver";
                rm -f Cargo.lock
                RUSTC_BOOTSTRAP=1 timeout 20 cargo +"$rustfetchver" -Z no-index-update -Z minimal-versions generate-lockfile;
                timeout 40 nice cargo +$rustver {check_command} -j3 --locked --message-format=json >>"$stdoutfile" 2>>"$stderrfile";
            }}
        }}
        for job in {jobs}; do
            (
                check_crate_with_rustc $job "/tmp/output-$job" "/tmp/outputerr-$job" && echo "# R.$job done OK" || echo "# R.$job failed"
            ) &
        done
        wait
        for job in {jobs}; do
            echo "{divider}"
            cat "/tmp/output-$job";
            echo >&2 "{divider}"
            cat >&2 "/tmp/outputerr-$job";
            cleanup $job
        done
    "##,
        check_command = if versions.iter().all(|v| v.is_app && v.rustc_ver >= 58) { "install \"$crate_name\" --version \"$libver\" --debug --root=./install-test" } else { "check" },
        divider = parse::DIVIDER,
        jobs = versions.iter().map(|c| format!("\"{} {} {}\"", rustc_minor_ver_to_version(c.rustc_ver), c.crate_name, c.version)).collect::<Vec<_>>().join(" "),
    );

    let mut child = Command::new("nice")
        .current_dir(docker_root)
        .arg("docker")
        .arg("run")
        .arg("--rm")
        .arg("-v").arg(format!("{}/git:/home/rustyuser/.cargo/git", TEMP_JUNK_DIR))
        .arg("-v").arg(format!("{}/registry:/home/rustyuser/.cargo/registry", TEMP_JUNK_DIR))
        .arg("-v").arg(format!("{}/target:/home/rustyuser/cargo_target", TEMP_JUNK_DIR))
        .arg("-v").arg(format!("{}:/home/rustyuser/job_inputs:ro", job_inputs_root.display()))
        .arg("-e").arg("CARGO_INCREMENTAL=0")
        // .arg("-e").arg("CARGO_UNSTABLE_AVOID_DEV_DEPS=true") // breaks --locked
        .arg("-e").arg("CARGO_UNSTABLE_BINDEPS=true")
        .arg("-e").arg("CARGO_BUILD_JOBS=3")
        .arg("-m3000m")
        .arg("--cpus=3")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .arg(DOCKER_NAME)
        .arg("bash").arg("-c").arg(script)
        .spawn()?;

    let stdout = streamfetch("stdout", child.stdout.take().unwrap());
    let stderr = streamfetch("stderr", child.stderr.take().unwrap());

    let status = child.wait()?;

    let stdout = std::mem::take(&mut *stdout.lock());
    let mut stderr = std::mem::take(&mut *stderr.lock());

    if !status.success() {
        stderr += &format!("\nexit failure {:?}\n", status);
    }

    Ok((stdout, stderr))
}

fn rustc_minor_ver_to_version(rustc_minor_ver: u16) -> String {
    if rustc_minor_ver == 56 {
        "1.56.1".into()
    } else {
        format!("1.{}.0", rustc_minor_ver)
    }
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
            if line.len() > 230 && line.is_char_boundary(230) {
                line.truncate(230);
            }
            println!("{}: {}", prefix, line);
        }
    });
    out2
}
