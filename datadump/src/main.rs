#![allow(unused)]
#![allow(dead_code)]
use serde::Deserialize;
use serde::Deserializer;
use ahash::HashSetExt;
use ahash::HashMapExt;
use chrono::prelude::*;
use kitchen_sink::CrateOwner;
use kitchen_sink::DependerChanges;
use kitchen_sink::KitchenSink;
use kitchen_sink::MiniDate;
use kitchen_sink::Origin;
use kitchen_sink::OwnerKind;
use kitchen_sink::StatsHistogram;
use libflate::gzip::Decoder;
use rayon::prelude::*;
use ahash::HashMap;
use ahash::HashSet;
use std::convert::TryInto;
use std::fs::File;
use std::io::BufReader;
use std::io::Read;
use tar::Archive;
use smartstring::alias::String as SmolStr;

struct Incident<Date, Set> {
    start: Date, end: Date,
    headroom: u32, lookaround: u8,
    names: Set,
}

// start end crates affected
const DOWNLOAD_SPAM_INCIDENTS: [Incident<&'static str, &'static [&'static str]>; 7] = [
    Incident { start: "2021-12-10", end: "2022-04-10", headroom: 4000, lookaround: 7, names: &[
        "vsdb", "btm", "vsdbsled", "vsdb_derive","ruc",
    ]},
    Incident { start: "2021-12-21", end: "2022-01-22", headroom: 4000, lookaround: 7, names: &[
        "ppv-lite86", "serde_cbor","time","fast-math","ieee754","time-macros","once_cell", "clap", "serde", "memoffset",
        "rand", "rand_core", "lazy_static", "half", "nix","cfg-if", "log", "libc", "num-traits", "serde_derive", "getrandom", "rand_chacha", "parking_lot", "lock_api",
        "instant", "parking_lot_core", "smallvec", "scopeguard", "sha3", "digest", "keccak", "proc-macro2", "unicode-xid", "quote", "syn", "fs2", "crc32fast", "fxhash",
        "byteorder", "crossbeam-epoch", "crossbeam-utils", "memoffset", "autocfg", "const_fn", "rio", "bitflags",
        "rocksdb", "librocksdb-sys", "zstd", "zstd-safe", "zstd-sys", "cc", "jobserver", "crypto-common", "generic-array", "version_check", "typenum", "block-buffer",
        "unicode-width", "atty", "itoa",
    ]},
    Incident { start: "2022-06-15", end: "2022-07-17", headroom: 0, lookaround: 7, names: &[
        "audiopus_sys", "mmids-core", "proc-mounts", "serde-pickle", "speexdsp-resampler", "stdext", "azure_data_cosmos",
        "azure_core", "tonic-web", "wav", "webrtc", "ccm", "debug-helper", "interceptor", "mock_instant", "sdp", "stdext",
        "turn", "webrtc-data", "webrtc-ice", "webrtc-dtls", "webrtc-mdns", "webrtc-media", "cranelift-isle", "str_stack", "ed25519-consensus", "igd",
        "riff", "webrtc-srtp", "webrtc-sctp", "partition-identity", "substring", "iter-read", "cidr-utils", "der-oid-macro",
        "x509-parser", "yasna"
    ]},
    Incident { start: "2022-06-29", end: "2022-07-14", headroom: 0, lookaround: 7, names: &[
        "va_list", "vkrs", "xmpp-parsers", "vpncloud", "symbolic-sourcemap", "symbolic-symcache", "tag_safe", "ydcv-rs", "way-cooler", "superlu-sys",
        "symbolic-proguard", "wemo", "xhtmlchardet", "tinysearch", "tenacious", "tofu", "tojson_macros", "urdict", "twitter-api",
        "uchardet", "task_queue", "yatlv", "table", "zmq-rs", "unixbar", "unrar", "zombie", "toml-config", "vorbis",
        "symbolic-debuginfo", "telemetry", "xcolor", "syntaxext_lint", "treena", "traverse", "symbolic_polynomials",
        "symbolic-unreal", "skeletal_animation", "td_revent", "symbolic-symcache", "telegram-bot", "v8-sys", "secp256k1",
        "test-assembler",
    ]},
    Incident { start: "2022-06-29", end: "2022-07-13", headroom: 0, lookaround: 2, names: SOURCEGRAPH_SPAMMED},
    Incident { start: "2022-06-29", end: "2022-07-13", headroom: 300, lookaround: 7, names: &[]},
    Incident { start: "2022-07-06", end: "2022-07-13", headroom: 100, lookaround: 2, names: &[]},
];

const SOURCEGRAPH_SPAMMED: &[&str] = &[
    "abomonation", "alsa", "assimp-sys", "aster", "atom_syndication", "aws-smithy-protocol-test", "bio", "bootloader", "cargo", "cargo-edit",
    "cargo-outdated", "cargo-readme ", "cobs", "clippy", "compiletest_rs", "conrod", "coreaudio-sys", "cortex-a", "cpython", "cron", "crust", "datetime",
    "decimal", "devicemapper", "elastic-array ", "elastic-array", "euclid", "flame", "flexi_logger", "freetype-rs", "ftp", "gdk", "generator",
    "genmesh", "gleam", "gstreamer", "gtk", "igd", "imageproc", "immeta", "jsonrpc", "kuchiki", "liquid", "llvm-sys", "lodepng", "mp4parse",
    "mysql", "nalgebra", "notify-rust", "obj", "opencv", "pbr", "pcap", "piston2d-gfx_graphics ", "piston2d-opengl_graphics", "piston_window",
    "pistoncore-sdl2_window ", "plist", "pnet_macros", "polars-core", "postgis", "primal-sieve ", "primal-sieve", "pty", "r2d2-diesel",
    "r2d2_mysql", "racer", "regex_macros", "router", "routing", "rss", "rust-htslib", "rustfmt", "rustler", "select", "self_encryption",
    "servo-fontconfig-sys", "servo-glutin", "servo-skia", "signal", "sprs", "stb_truetype", "symbolic-debuginfo", "symbolic_demangle",
    "sysfs_gpio", "sysinfo", "systemd", "systemstat", "timely", "tobj", "tokei", "utime", "va_list", "wavefront_obj", "xcb",
    "tract-core", "vkrs", "vpncloud", "xmpp-parsers", "way-cooler", "ydcv-rs", "tag_safe", "symbolic-sourcemap", "symbolic-unreal",
    "symbolic-symcache", "skeletal_animation", "v8-sys", "td_revent", "superlu-sys", "symbolic-proguard", "telegram-bot", "rustorm",
    "rustfbp", "scaly", "slack", "scribe", "rustwlc", "rsgenetic", "select_color", "rs-graph", "sdl2_ttf", "slabmalloc", "sdl2_image",
    "rsteam", "rusted_cypher", "speedtest-rs", "sassers", "spaceapi", "rust-mpfr", "sdl2_mixer", "shuttle", "sexp", "serde_xml",
    "sel4-start", "smtp", "by_address", "sawtooth-xo", "rosc", "ssbh_lib", "ssdp", "sel4-sys", "static-server", "squash-sys",
    "tokei", "rs-es", "rusp", "snailquote", "sql_lexer", "rusoto", "cobs",
];

const NUM_CRATES: usize = 100_000;
type BoxErr = Box<dyn std::error::Error + Sync + Send>;

#[tokio::main]
async fn main() {
    env_logger::init();
    let path = std::env::args_os().nth(1);

    let res: Result<(), BoxErr> = tokio::runtime::Handle::current().spawn(async move {
        let handle = tokio::runtime::Handle::current();
        let ksink = KitchenSink::new_default().await?;
        let mut tmp1;
        let mut tmp2;
        let src: &mut dyn Read = if let Some(path) = path {
            eprintln!("Loading local");
            tmp1 = std::fs::File::open(path)?;
            &mut tmp1
        } else {
            // I can't be bothered to make async stream adapter to make async body impl Read
            tmp2 = reqwest::blocking::get("https://static.crates.io/db-dump.tar.gz")?;
            &mut tmp2
        };
        let res = BufReader::with_capacity(8_000_000, src);
        let mut a = Archive::new(Decoder::new(res)?);

        tokio::task::block_in_place(move || {
            let mut crate_owners = None;
            let mut crates = None;
            let mut metadata = None;
            let mut teams = None;
            let mut users = None;
            let mut downloads = None;
            let mut versions = None;
            let mut dependencies = None;

            for file in a.entries()? {
                let file = file?;
                if !file.header().entry_type().is_file() {
                    continue;
                }
                if let Some(path) = file.path()?.file_name().and_then(|f| f.to_str()) {
                    eprint!("{path} ({}MB): ", file.header().size()? / 1000 / 1000);
                    match path {
                        "crate_owners.csv" => {
                            eprintln!("parse_crate_owners…");
                            crate_owners = Some(parse_crate_owners(file)?);
                        },
                        "crates.csv" => {
                            eprintln!("parse_crates…");
                            crates = Some(parse_crates(file)?);
                        },
                        "metadata.csv" => {
                            eprintln!("parse_metadata…");
                            metadata = Some(parse_metadata(file)?);
                        },
                        "teams.csv" => {
                            eprintln!("parse_teams…");
                            teams = Some(parse_teams(file)?);
                        },
                        "users.csv" => {
                            eprintln!("parse_users…");
                            users = Some(parse_users(file)?);
                        },
                        "version_downloads.csv" => {
                            eprintln!("parse_version_downloads…");
                            downloads = Some(parse_version_downloads(file)?);
                        },
                        "versions.csv" => {
                            eprintln!("parse_versions…");
                            versions = Some(parse_versions(file)?);
                        },
                        "dependencies.csv" => {
                            eprintln!("parse_dependencies");
                            dependencies = Some(parse_dependencies(file)?);
                        },
                        // expected ignored
                        "reserved_crate_names.csv" | // not publishing any
                        "version_authors.csv" | // is in index
                        "badges.csv" | // got from cargo.tomls
                        "crates_categories.csv" | // got better data than this
                        "crates_keywords.csv" | "keywords.csv" | // got better data than this
                        "categories.csv" | // got my own categories
                        "metadata.json" | "README.md" | // not relevant
                        "import.sql" | "export.sql" | "schema.sql" // NoSQL
                        => eprintln!("skip"),
                        p => eprintln!("Ignored unexpected file {p}"),
                    };

                    if let (Some(crates), Some(versions), Some(crate_owners)) = (&crates, &versions, &crate_owners) {
                        if let Some(dependencies) = dependencies.take() {
                            eprintln!("Indexing dependencies for {} crates", dependencies.len());
                            index_active_rev_dependencies(crates, versions, &dependencies, crate_owners, &ksink)?;
                            eprintln!("Versions histogram");
                            versions_histogram(crates, versions, &dependencies, &ksink)?;
                        }
                    }


                    if let (Some(crates), Some(versions)) = (&crates, &versions) {
                        if let Some(mut downloads) = downloads.take() {
                            eprintln!("Despamming");
                            filter_download_spam(crates, versions, &mut downloads);
                            eprintln!("Indexing downloads for {} crates, {} dl-versions", versions.len(), downloads.len());
                            index_downloads(crates, versions, &downloads, &ksink);
                        }
                    }
                }
            }

            if let (Some(crates), Some(teams), Some(users)) = (crates, teams, users) {
                if let Some(crate_owners) = crate_owners.take() {
                    handle.spawn(async move {
                        eprintln!("Indexing owners of {} crates", crate_owners.len());
                        let owners = process_owners(&crates, crate_owners, &teams, &users);
                        eprintln!("Upserting {} owners", owners.len());
                        ksink.index_crates_io_crate_all_owners(owners).await.unwrap();
                    });
                }
            }
            Ok(())
        })
    })
    .await.unwrap();

    if let Err(e) = res {
        eprintln!("datadump failed: {e}");
        let mut src = e.source();
        while let Some(e) = src {
            eprintln!(" {e}");
            src = e.source();
        }
        std::process::exit(1);
    }
}

// Cap spammed download data to ~prev week's max during incident period
#[inline(never)]
fn filter_download_spam(crates: &CratesMap, versions: &VersionsMap, downloads: &mut VersionDownloads) {
    // the downloads in the datadump aren't complete, so incidents can be fixed only when they're recent
    let earliest_date_available = downloads.values().flatten().map(|&(d, _, _)| d).min().unwrap();
    let incidents: Vec<_> = DOWNLOAD_SPAM_INCIDENTS.iter().filter_map(|&Incident {start,end,headroom,lookaround,names}| {
        let start = date_from_str(start).unwrap();
        // otherwise it won't have enough before/after data
        if earliest_date_available > start - chrono::Duration::days(lookaround.into()) {
            return None;
        }
        Some(Incident {
            start,
            end: date_from_str(end).unwrap(),
            headroom, lookaround,
            names: names.iter().copied().collect::<HashSet<_>>(),
        })
    }).collect();
    if incidents.is_empty() {
        return;
    }
    for (crate_id, name) in crates.iter() {
        let versions = versions.get(crate_id).expect(name);
        for &Incident { start, end, headroom, lookaround, .. } in incidents.iter().filter(|i| i.names.is_empty() || i.names.contains(name.as_str())) {
            let min_date = start - chrono::Duration::days(lookaround.into());
            let max_date = end + chrono::Duration::days(lookaround.into());

            for version_id in versions.iter().map(|row| row.id) {
                if let Some(mut dl) = downloads.get_mut(&version_id) {
                    let before = dl.iter().filter(|(day, _, _)| (*day >= min_date && *day < start) )
                        .map(|&(_, dl, _)| dl).max().unwrap_or(0);
                    let after = dl.iter().filter(|(day, _, _)| (*day <= max_date && *day > end) )
                        .map(|&(_, dl, _)| dl).max().unwrap_or(before * 5 / 4);
                    let max_dl = headroom + before.max(after);
                    let expected = (before + after)/2;

                    dl.iter_mut()
                        .filter(|(day, dl, _)| {
                            *dl > max_dl && *day >= start && *day <= end
                        })
                        .for_each(|(day, dl, ovr)| {
                            // eprintln!("cut {day} {dl} > {max_dl} to {expected} for {name} in {start}-{end} incident");
                            *dl = expected;
                            *ovr = true;
                        });
                }
            }
        }
    }
}

#[inline(never)]
fn index_downloads(crates: &CratesMap, versions: &VersionsMap, downloads: &VersionDownloads, ksink: &KitchenSink) {
    for (crate_id, name) in crates {
        if let Some(vers) = versions.get(crate_id) {
            let data = vers
                .iter()
                .filter_map(|version| {
                    if let Some(d) = downloads.get(&version.id) {
                        return Some((version.num.as_str(), d.as_slice()));
                    }
                    None
                })
                .collect();
            if let Err(e) = ksink.index_crate_downloads(name, &data) {
                eprintln!("Can't index downloads for {name}: {e}");
            }
        } else {
            eprintln!("Bad crate? {crate_id} {name}");
        }
    }
}

const EXAMPLES_PER_BUCKET: usize = 5;

fn versions_histogram(crates: &CratesMap, versions: &VersionsMap, deps: &CrateDepsMap, ksink: &KitchenSink) -> Result<(), BoxErr> {
    let mut num_releases = HashMap::new();
    let mut crate_sizes = HashMap::new();
    let mut licenses = HashMap::new();
    let mut num_deps = HashMap::new();
    let mut age = HashMap::new();
    let mut maintenance = HashMap::new();
    let mut languish = HashMap::new();
    let today = Utc::today();

    // hopefully the hashmap is randomizing examples
    for (crate_id, name) in crates {
        if let Some(v) = versions.get(crate_id) {
            let non_yanked_releases = v.iter().filter(|d| d.yanked != 't').count() as u32;
            if non_yanked_releases == 0 {
                continue;
            }

            if let Some(latest) = v.iter().max_by_key(|v| v.id) {
                if let Some(oldest) = v.iter().min_by_key(|v| v.id) {
                    let latest_date = latest.created_at.with_timezone(&Utc).date();
                    let oldest_date = oldest.created_at.with_timezone(&Utc).date();
                    insert_hist_item(&mut age, today.signed_duration_since(oldest_date).num_weeks() as u32, name);
                    insert_hist_item(&mut languish, today.signed_duration_since(latest_date).num_weeks() as u32, name);
                    let maintenance_weeks = if v.len() == 1 {
                        0
                    } else {
                        latest_date.signed_duration_since(oldest_date).num_weeks().max(1) as u32
                    };
                    insert_hist_item(&mut maintenance, maintenance_weeks, name);
                }

                if let Some(size) = latest.crate_size {
                    let size_kib = if size > 5_000_000 {
                        size / (1000 * 500) * 500 // coarser buckets
                    } else {
                        size / 1000
                    };
                    insert_hist_item(&mut crate_sizes, size_kib as u32, name);
                }
                if let Some(t) = licenses.get_mut(&latest.license) {
                    *t += 1;
                } else {
                    licenses.insert(latest.license.clone(), 1);
                }

                insert_hist_item(&mut num_deps, deps.get(&latest.id).map(|d| d.as_slice()).unwrap_or_default().len() as u32, name);
            }
            insert_hist_item(&mut num_releases, non_yanked_releases, name);
        }
    }
    ksink.index_stats_histogram("releases", num_releases);
    ksink.index_stats_histogram("sizes", crate_sizes);
    ksink.index_stats_histogram("deps", num_deps);
    ksink.index_stats_histogram("age", age);
    ksink.index_stats_histogram("maintenance", maintenance);
    ksink.index_stats_histogram("languish", languish);

    // histogram cache doesn't support string keys
    let fudge = licenses.into_iter().enumerate().map(|(i, (lic, num))| {
        (i as u32, (num, vec![lic])) // TODO: normalize license names/parse spdx
    }).collect();
    ksink.index_stats_histogram("licenses-hack", fudge);

    Ok(())
}

fn insert_hist_item(histogram: &mut StatsHistogram, val: u32, name: &str) {
    let t = histogram.entry(val).or_insert((0, Vec::<String>::new()));
    t.0 += 1;
    if t.1.len() < EXAMPLES_PER_BUCKET {
        t.1.push(name.to_owned());
    }
}

struct DepUse {
    start_date: MiniDate,
    end_date: MiniDate,
    expired: bool,
    by_crate_id: CrateId,
}

#[derive(Default)]
pub struct DepChangeAggregator {
    pub added: u16,
    /// Crate has released a new version without this dependency
    pub removed: u16,
    /// Crate has this dependnecy, but is not active any more
    pub expired: u16,
    pub by_owner: HashMap<(u32, u8), f32>, // owner id, owner kind
}

/// Direct reverse dependencies, but with release dates (when first seen or last used)
#[inline(never)]
fn index_active_rev_dependencies(crates: &CratesMap, versions: &VersionsMap, deps: &CrateDepsMap, owners: &CrateOwners, ksink: &KitchenSink) -> Result<(), BoxErr> {
    let mut deps_changes = HashMap::with_capacity(crates.len());

    for (crate_id, name) in crates {
        let has_multiple_owners = owners.get(crate_id).map(|o| o.len() > 1).unwrap_or(false);
        let vers = match versions.get(crate_id) {
            Some(v) => v,
            None => {
                eprintln!("Bad crate? {crate_id} {name}");
                continue;
            },
        };
        let mut releases_from_oldest: Vec<_> = vers.iter().map(|v| {
            (v.id, MiniDate::new(v.created_at.date()), &v.num)
        }).collect();
        if releases_from_oldest.is_empty() {
            continue; // shouldn't happen
        }
        releases_from_oldest.sort_unstable_by_key(|a| a.0);

        let mut over_time = HashMap::new();
        let mut releases = releases_from_oldest.iter().peekable();
        while let Some(&(ver_id, release_date, version_str)) = releases.next() {
            let expired;
            let end_date = if let Some(&&(_, next_release_date, _)) = releases.peek() {
                expired = false;
                // if the next releases is going to drop it, then attribute drop date
                // to some time between the releases
                release_date.half_way(next_release_date)
            } else {
                expired = true;
                // it's the final release, so relevance is now until the death of the crate.
                //
                // approximate how junky or stable the crate is
                // assuming that experimental versions get abandoned quickly
                // and 1.x are relevant for longer
                //
                // FIXME: this logic should be unified with ranking
                let junk = version_str.as_str() == "0.1.0";
                let unstable = version_str.starts_with("0.");
                // a little bit of deterministic fuzzyness
                // to make date cut-offs less sharp
                let rand = (ver_id % 17) as i32;
                let days_fresh = if junk {
                    30 * 2 + rand + if has_multiple_owners { 14 } else { 0 } // assume more owners means it's likely to be a dead hobby project
                } else if unstable && releases_from_oldest.len() < 3 {
                    30 * 6 + rand * 2 + if has_multiple_owners { 30 } else { 0 }
                } else if unstable || releases_from_oldest.len() < 3 {
                    365 + rand * 3 + if has_multiple_owners { 90 } else { 0 }
                } else {
                    365 * 2 + rand * 4 + if has_multiple_owners { 365 } else { 0 }
                };
                release_date.days_later(days_fresh)
            };

            for &dep_crate_id in deps.get(&ver_id).map(Vec::as_slice).unwrap_or_default() {
                if dep_crate_id == *crate_id {
                    // libcpocalypse semver trick - not relevant
                    continue;
                }
                let e = over_time.entry(dep_crate_id).or_insert(DepUse {start_date: release_date, end_date, expired, by_crate_id: *crate_id});
                if e.end_date < end_date {
                    e.end_date = end_date;
                    e.expired = expired;
                }
            }
        }

        for (dep_crate_id, first_last) in over_time {
            deps_changes.entry(dep_crate_id).or_insert_with(Vec::new).push(first_last);
        }
    }

    let today = MiniDate::new(Utc::today());
    for (dep_crate_id, uses) in deps_changes {
        let mut by_day = HashMap::with_capacity(uses.len() * 2);
        for DepUse {start_date, end_date, expired, by_crate_id} in uses {
            let start_use = by_day.entry(start_date).or_insert_with(DepChangeAggregator::default);
            start_use.added += 1;
            // owners are supposed to add up to 1, so that a one crate with lots of owners doesn't create lots of users, only one "user"
            let owners = owners.get(&by_crate_id).map(Vec::as_slice).unwrap_or_default();
            let per_owner_weight = 1. / owners.len() as f32;
            for o in owners {
                *start_use.by_owner.entry((o.owner_id, o.owner_kind)).or_default() += per_owner_weight;
            }
            if end_date <= today {
                let e = by_day.entry(end_date).or_insert_with(DepChangeAggregator::default);
                for o in owners {
                    *e.by_owner.entry((o.owner_id, o.owner_kind)).or_default() -= per_owner_weight;
                }
                if expired {
                    e.expired += 1;
                } else {
                    e.removed += 1;
                }
            }
        }
        let mut by_day: Vec<_> = by_day.into_iter().collect();
        by_day.sort_unstable_by_key(|a| a.0);

        let crates_own_owners: HashSet<_> = owners.get(&dep_crate_id).map(Vec::as_slice).unwrap_or_default()
            .iter().map(|o| (o.owner_id, o.owner_kind))
            .chain([(362, 1), (21274, 0)]) // rust-bus doesn't count
            .collect();
        let mut users_aggregate = HashMap::<(u32, u8), f64>::new();
        let deps_by_day = by_day.into_iter().map(|(at, DepChangeAggregator { added, removed, expired, by_owner })| {
            for (owner_id, net_change) in by_owner {
                if crates_own_owners.contains(&owner_id) {
                    continue;
                }
                *users_aggregate.entry(owner_id).or_default() += net_change as f64;
            }
            // one owner can't count as more than 1 user, but fraction of an owner is kept as a fraction (so many partial co-users add up to one real user)
            let users_abs = users_aggregate.values().map(|&v| v.min(1.)).sum::<f64>().ceil() as u16;
            DependerChanges { at, added, removed, expired, users_abs }
        }).collect::<Vec<_>>();

        let name = crates.get(&dep_crate_id).expect("bork crate");
        let origin = Origin::from_crates_io_name(name);
        ksink.index_dependers_liveness_ranges(&origin, deps_by_day);
    }
    Ok(())
}

fn process_owners(crates: &CratesMap, owners: CrateOwners, teams: &Teams, users: &Users) -> Vec<(Origin, Vec<CrateOwner>)> {
    owners.into_par_iter()
    .filter_map(move |(crate_id, owners)| {
        crates.get(&crate_id).map(move |k| (crate_id, owners, Origin::from_crates_io_name(k)))
    })
    .map(move |(crate_id, owners, origin)| {
        let mut owners: Vec<_> = owners
            .into_iter()
            .filter_map(|o| {
                // teams can't invite users
                let invited_by_github_id = o.created_by_id.and_then(|id| users.get(&id).and_then(|u| u.github_id.try_into().ok()));
                let mut o = match o.owner_kind {
                    0 => {
                        let u = users.get(&o.owner_id).expect("owner consistency");
                        if u.github_id <= 0 {
                            return None;
                        }
                        CrateOwner {
                            crates_io_login: u.login.clone(),
                            invited_at: Some(o.created_at),
                            invited_by_github_id,
                            github_id: u.github_id.try_into().ok(),
                            name: if u.name != u.login { Some(u.name.clone()) } else { None },
                            avatar: None,
                            url: None,
                            kind: OwnerKind::User,
                            last_seen_at: None,
                            contributor_only: false,
                        }
                    },
                    1 => {
                        let u = match teams.get(&o.owner_id) {
                            Some(t) => t,
                            None => {
                                eprintln!("warning: id {} is not in teams (len={}). Bad obj: {o:?} {origin:?}", o.owner_id, teams.len());
                                return None;
                            },
                        };
                        CrateOwner {
                            crates_io_login: u.login.to_owned(),
                            invited_at: Some(o.created_at),
                            github_id: Some(u.github_id),
                            invited_by_github_id,
                            name: if u.name != u.login.split(':').next().unwrap() { Some(u.name.clone()) } else { None },
                            avatar: None,
                            url: None,
                            kind: OwnerKind::Team,
                            last_seen_at: None,
                            contributor_only: false,
                        }
                    },
                    _ => unreachable!("bad owner type"),
                };
                if o.github_id == o.invited_by_github_id {
                    o.invited_by_github_id = None;
                }
                Some(o)
            })
            .collect();

        owners.sort_unstable_by(|a,b| a.invited_at.cmp(&b.invited_at));

        // crates.io has some data missing in old crates
        if let Some((first_owner, rest)) = owners.split_first_mut() {
            for other in rest {
                if other.invited_by_github_id.is_none() {
                    other.invited_by_github_id = first_owner.invited_by_github_id.or(first_owner.github_id);
                }
            }
        }
        (origin, owners)
    }).collect()
}

#[derive(Debug, Deserialize)]
struct CrateOwnerRow {
    crate_id: CrateId,
    #[serde(deserialize_with = "date_fudge")]
    created_at: DateTime<Utc>,
    created_by_id: Option<UserId>,
    owner_id: u32,
    owner_kind: u8,
}

fn date_fudge<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
    where D: Deserializer<'de>,
{
    let date_str = <&str>::deserialize(deserializer)?;

    let date_part = date_str.split('.').next().unwrap(); // trim millis
    match Utc.datetime_from_str(date_part, "%Y-%m-%d %H:%M:%S") {
        Ok(date) => Ok(date),
        Err(e) => panic!("Bad date: {date_str}: {e}"),
    }
}

type CrateId = u32;
type CrateOwners = HashMap<CrateId, Vec<CrateOwnerRow>>;

#[inline(never)]
fn parse_crate_owners(file: impl Read) -> Result<CrateOwners, BoxErr> {
    let mut csv = csv::ReaderBuilder::new().has_headers(true).flexible(false).from_reader(file);
    let mut out = HashMap::with_capacity(NUM_CRATES);
    for r in csv.records() {
        let r = r?;
        let r = r.deserialize::<CrateOwnerRow>(None).map_err(|e| format!("wat? {r:#?} {e}"))?;
        out.entry(r.crate_id).or_insert_with(|| Vec::with_capacity(1)).push(r);
    }
    Ok(out)
}

#[derive(Deserialize)]
struct TeamRow {
    avatar: Box<str>,
    github_id: u32,
    id: u32,
    login: SmolStr, // in the funny format
    name: SmolStr,  // human str
}

type TeamId = u32;
type Teams = HashMap<TeamId, TeamRow>;

#[inline(never)]
fn parse_teams(file: impl Read) -> Result<Teams, BoxErr> {
    let mut csv = csv::ReaderBuilder::new().has_headers(true).flexible(false).from_reader(file);
    let mut out = HashMap::with_capacity(NUM_CRATES);
    for r in csv.records() {
        let r = r?;
        let r = r.deserialize::<TeamRow>(None).map_err(|e| format!("{e}: {r:?}"))?;
        out.insert(r.id, r);
    }
    Ok(out)
}

#[derive(Deserialize)]
struct UserRow {
    avatar: Box<str>,
    github_id: i32, // there is -1 :(
    login: SmolStr,
    id: UserId,
    name: SmolStr,
}

type UserId = u32;
type Users = HashMap<UserId, UserRow>;

#[inline(never)]
fn parse_users(file: impl Read) -> Result<Users, BoxErr> {
    let mut csv = csv::ReaderBuilder::new().has_headers(true).flexible(false).from_reader(file);
    let mut out = HashMap::with_capacity(NUM_CRATES);
    for r in csv.records() {
        let r = r?;
        let row = r.deserialize::<UserRow>(None).map_err(|e| format!("{e}: {r:?}"))?;
        out.insert(row.id, row);
    }
    Ok(out)
}

type CrateVersionId = u32;

#[derive(Deserialize, Debug)]
struct CrateVersionRow {
    #[serde(with = "hex")]
    checksum: [u8; 32],
    crate_id: CrateId,
    crate_size: Option<u64>,
    #[serde(deserialize_with = "date_fudge")]
    created_at: DateTime<Utc>,
    downloads: u64,
    features: String, // json
    id: CrateVersionId,
    license: String,
    links: Option<String>,
    num: String, // ver
    published_by: Option<u32>,
    updated_at: String,
    yanked: char,
}

type VersionsMap = HashMap<CrateId, Vec<CrateVersionRow>>;

#[inline(never)]
fn parse_versions(mut file: impl Read) -> Result<VersionsMap, BoxErr> {
    let mut csv = csv::ReaderBuilder::new().has_headers(true).flexible(false).from_reader(file);
    let mut out = HashMap::with_capacity(NUM_CRATES);
    for r in csv.records() {
        let r = r?;
        let row = r.deserialize::<CrateVersionRow>(None)?;
        out.entry(row.crate_id).or_insert_with(Vec::new).push(row);
    }
    Ok(out)
}

type VersionDownloads = HashMap<CrateVersionId, Vec<(Date<Utc>, u32, bool)>>;

fn date_from_str(date: &str) -> Result<Date<Utc>, std::num::ParseIntError> {
    let y = date[0..4].parse()?;
    let m = date[5..7].parse()?;
    let d = date[8..10].parse()?;
    Ok(Utc.ymd(y, m, d))
}

#[inline(never)]
fn parse_version_downloads(mut file: impl Read) -> Result<VersionDownloads, BoxErr> {
    let mut csv = csv::ReaderBuilder::new().has_headers(true).flexible(false).from_reader(file);
    let mut out = HashMap::with_capacity(NUM_CRATES);
    for r in csv.records() {
        let r = r?;
        let mut r = r.iter();
        let date = date_from_str(r.next().ok_or("no date")?)?;
        let downloads = r.next().and_then(|s| s.parse().ok()).ok_or("bad dl1")?;
        let version_id = r.next().and_then(|s| s.parse().ok()).ok_or("bad dl2")?;
        out.entry(version_id).or_insert_with(|| Vec::with_capacity(365 * 4)).push((date, downloads, false));
    }
    Ok(out)
}

#[inline(never)]
fn parse_metadata(mut file: impl Read) -> Result<u64, BoxErr> {
    let mut s = String::with_capacity(60);
    file.read_to_string(&mut s)?;
    Ok(s.split('\n').nth(1).and_then(|s| s.parse().ok()).ok_or("bad num")?)
}

type CratesMap = HashMap<CrateId, SmolStr>;

#[inline(never)]
fn parse_crates(file: impl Read) -> Result<CratesMap, BoxErr> {
    let mut csv = csv::ReaderBuilder::new().has_headers(true).flexible(false).from_reader(file);
    let mut out = HashMap::with_capacity(NUM_CRATES);
    for r in csv.records() {
        let r = r?;
        let id: CrateId = r.get(5).and_then(|s| s.parse().ok()).ok_or("bad record1")?;
        let name = r.get(7).ok_or("bad record2")?;
        out.insert(id, name.into());
    }
    Ok(out)
}

/// version ID depends on crate ID
type CrateDepsMap = HashMap<u32, Vec<u32>>;

#[inline(never)]
fn parse_dependencies(file: impl Read) -> Result<CrateDepsMap, BoxErr> {
    // crate_id,default_features,features,id,kind,optional,req,target,version_id
    let mut csv = csv::ReaderBuilder::new().has_headers(true).flexible(false).from_reader(file);
    let mut out = HashMap::with_capacity(NUM_CRATES);
    for r in csv.records() {
        let r = r?;
        // 0crate_id,1default_features,2explicit_name,3features,4id,5kind,6optional,7req,8target,9version_id
        let crate_id: CrateId = r.get(0).and_then(|s| s.parse().ok()).ok_or("bad record3")?;
        let version_id: u32 = r.get(9).and_then(|s| s.parse().ok()).ok_or("bad record4")?;
        out.entry(version_id).or_insert_with(Vec::new).push(crate_id);
    }
    Ok(out)
}
