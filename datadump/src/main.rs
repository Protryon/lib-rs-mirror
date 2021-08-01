#![allow(unused)]
#![allow(dead_code)]
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
use serde_derive::Deserialize;
use std::collections::HashMap;
use std::collections::HashSet;
use std::convert::TryInto;
use std::fs::File;
use std::io::BufReader;
use std::io::Read;
use tar::Archive;

const NUM_CRATES: usize = 42000;
type BoxErr = Box<dyn std::error::Error + Sync + Send>;

#[tokio::main]
async fn main() -> Result<(), BoxErr> {
    let path = std::env::args_os().nth(1);

    tokio::runtime::Handle::current().spawn(async move {
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
                    eprint!("{} ({}KB): ", path, file.header().size()? / 1000);
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
                        p => eprintln!("Ignored unexpected file {}", p),
                    };

                    if let (Some(crates), Some(versions)) = (&crates, &versions) {
                        if let Some(dependencies) = dependencies.take() {
                            eprintln!("Indexing dependencies for {} crates", dependencies.len());
                            index_active_rev_dependencies(crates, versions, &dependencies, &ksink)?;
                            versions_histogram(crates, versions, &dependencies, &ksink)?;
                        }
                    }


                    if let (Some(crates), Some(versions)) = (&crates, &versions) {
                        if let Some(downloads) = downloads.take() {
                            eprintln!("Indexing {} crates, {} downloads", versions.len(), downloads.len());
                            index_downloads(crates, versions, &downloads, &ksink)?;
                        }
                    }
                }
            }

            if let (Some(crates), Some(teams), Some(users)) = (crates, teams, users) {
                if let Some(crate_owners) = crate_owners.take() {
                    handle.spawn(async move {
                        eprintln!("Indexing owners of {} crates", crate_owners.len());
                        let owners = process_owners(&crates, crate_owners, &teams, &users);
                        ksink.index_crates_io_crate_all_owners(owners).await.unwrap();
                    });
                }
            }
            Ok(())
        })
    })
    .await
    .unwrap()
}

#[inline(never)]
fn index_downloads(crates: &CratesMap, versions: &VersionsMap, downloads: &VersionDownloads, ksink: &KitchenSink) -> Result<(), BoxErr> {
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
            ksink.index_crate_downloads(name, &data)?;
        } else {
            eprintln!("Bad crate? {} {}", crate_id, name);
        }
    }
    Ok(())
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
                    let latest_date = date_from_str(&latest.created_at);
                    let oldest_date = date_from_str(&oldest.created_at);
                    if let (Ok(latest_date), Ok(oldest_date)) = (latest_date, oldest_date) {
                        insert_hist_item(&mut age, today.signed_duration_since(oldest_date).num_weeks() as u32, &name);
                        insert_hist_item(&mut languish, today.signed_duration_since(latest_date).num_weeks() as u32, &name);
                        let maintenance_weeks = if v.len() == 1 {
                            0
                        } else {
                            latest_date.signed_duration_since(oldest_date).num_weeks().max(1) as u32
                        };
                        insert_hist_item(&mut maintenance, maintenance_weeks, &name);
                    }
                }

                if let Some(size) = latest.crate_size {
                    let size_kib = if size > 5_000_000 {
                        size / (1000 * 500) * 500 // coarser buckets
                    } else {
                        size / 1000
                    };
                    insert_hist_item(&mut crate_sizes, size_kib as u32, &name);
                }
                if let Some(t) = licenses.get_mut(&latest.license) {
                    *t += 1;
                } else {
                    licenses.insert(latest.license.clone(), 1);
                }

                insert_hist_item(&mut num_deps, deps.get(&latest.id).map(|d| d.as_slice()).unwrap_or_default().len() as u32, &name);
            }
            insert_hist_item(&mut num_releases, non_yanked_releases, &name);
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

/// Direct reverse dependencies, but with release dates (when first seen or last used)
#[inline(never)]
fn index_active_rev_dependencies(crates: &CratesMap, versions: &VersionsMap, deps: &CrateDepsMap, ksink: &KitchenSink) -> Result<(), BoxErr> {
    let mut deps_changes = HashMap::with_capacity(crates.len());

    for (crate_id, name) in crates {

        if let Some(vers) = versions.get(crate_id) {
            let mut releases_from_oldest: Vec<_> = vers.iter().filter_map(|v| {
                date_from_str(&v.created_at).ok().map(|date| (v.id, MiniDate::new(date), &v.num))
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
                        30 * 2 + rand
                    } else if unstable && releases_from_oldest.len() < 3 {
                        30 * 6 + rand * 2
                    } else if unstable || releases_from_oldest.len() < 3 {
                        365 + rand * 3
                    } else {
                        365 * 2 + rand * 4
                    };
                    release_date.days_later(days_fresh)
                };

                for &dep_id in deps.get(&ver_id).map(Vec::as_slice).unwrap_or_default() {
                    if dep_id == *crate_id {
                        // libcpocalypse semver trick - not relevant
                        continue;
                    }
                    let e = over_time.entry(dep_id).or_insert((release_date, end_date, expired));
                    if e.1 < end_date {
                        e.1 = end_date;
                        e.2 = expired;
                    }
                }
            }

            for (dep_id, first_last) in over_time {
                deps_changes.entry(dep_id).or_insert_with(Vec::new).push(first_last);
            }
        } else {
            eprintln!("Bad crate? {} {}", crate_id, name);
        }
    }

    for (crate_id, uses) in deps_changes {
        let name = crates.get(&crate_id).expect("bork crate");
        let today = MiniDate::new(Utc::today());
        let mut by_day = HashMap::with_capacity(uses.len() * 2);
        for (start_date, end_date, expired) in uses {
            by_day.entry(start_date).or_insert(DependerChanges { at: start_date, added: 0, removed: 0, expired: 0 }).added += 1;
            if end_date <= today {
                let e = by_day.entry(end_date).or_insert(DependerChanges { at: end_date, added: 0, removed: 0, expired: 0 });
                if expired {
                    e.expired += 1;
                } else {
                    e.removed += 1;
                }
            }
        }
        let mut by_day: Vec<_> = by_day.values().copied().collect();
        by_day.sort_unstable_by_key(|a| a.at);

        let origin = Origin::from_crates_io_name(name);
        ksink.index_dependers_liveness_ranges(&origin, by_day);
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
                let invited_at = o.created_at.splitn(2, '.').next().unwrap().to_string(); // trim millis part
                let invited_by_github_id =
                    o.created_by_id.and_then(|id| users.get(&id).map(|u| u.github_id as u32).or_else(|| teams.get(&id).map(|t| t.github_id)));
                let mut o = match o.owner_kind {
                    0 => {
                        let u = users.get(&o.owner_id).expect("owner consistency");
                        if u.github_id <= 0 {
                            return None;
                        }
                        CrateOwner {
                            login: u.login.to_owned(),
                            invited_at: Some(invited_at),
                            invited_by_github_id,
                            github_id: u.github_id.try_into().ok(),
                            name: Some(u.name.to_owned()),
                            avatar: None,
                            url: None,
                            kind: OwnerKind::User,
                            last_seen_at: None,
                        }
                    },
                    1 => {
                        let u = match teams.get(&o.owner_id) {
                            Some(t) => t,
                            None => {
                                eprintln!("warning: id {} is not in teams (len={}). Bad obj: {:?} {:?}", o.owner_id, teams.len(), o, origin);
                                return None;
                            },
                        };
                        CrateOwner {
                            login: u.login.to_owned(),
                            invited_at: Some(invited_at),
                            github_id: Some(u.github_id),
                            invited_by_github_id,
                            name: Some(u.name.to_owned()),
                            avatar: None,
                            url: None,
                            kind: OwnerKind::Team,
                            last_seen_at: None,
                        }
                    },
                    _ => panic!("bad owner type"),
                };
                if o.github_id == o.invited_by_github_id {
                    o.invited_by_github_id = None;
                }
                Some(o)
            })
            .collect();

        owners.sort_by(|a,b| a.invited_at.cmp(&b.invited_at));

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
    crate_id: u32,
    created_at: String,
    created_by_id: Option<u32>,
    owner_id: u32,
    owner_kind: u8,
}

type CrateOwners = HashMap<u32, Vec<CrateOwnerRow>>;

#[inline(never)]
fn parse_crate_owners(file: impl Read) -> Result<CrateOwners, BoxErr> {
    let mut csv = csv::ReaderBuilder::new().has_headers(true).flexible(false).from_reader(file);
    let mut out = HashMap::with_capacity(NUM_CRATES);
    for r in csv.records() {
        let r = r?;
        let r = r.deserialize::<CrateOwnerRow>(None).map_err(|e| format!("wat? {:#?} {}", r, e))?;
        out.entry(r.crate_id).or_insert_with(|| Vec::with_capacity(1)).push(r);
    }
    Ok(out)
}

#[derive(Deserialize)]
struct TeamRow {
    avatar: String,
    github_id: u32,
    id: u32,
    login: String, // in the funny format
    name: String,  // human str
}

type Teams = HashMap<u32, TeamRow>;

#[inline(never)]
fn parse_teams(file: impl Read) -> Result<Teams, BoxErr> {
    let mut csv = csv::ReaderBuilder::new().has_headers(true).flexible(false).from_reader(file);
    let mut out = HashMap::with_capacity(NUM_CRATES);
    for r in csv.records() {
        let r = r?;
        let r = r.deserialize::<TeamRow>(None).map_err(|e| format!("{}: {:?}", e, r))?;
        out.insert(r.id, r);
    }
    Ok(out)
}

#[derive(Deserialize)]
struct UserRow {
    avatar: String,
    github_id: i32, // there is -1 :(
    login: String,
    id: u32,
    name: String,
}

type Users = HashMap<u32, UserRow>;

#[inline(never)]
fn parse_users(file: impl Read) -> Result<Users, BoxErr> {
    let mut csv = csv::ReaderBuilder::new().has_headers(true).flexible(false).from_reader(file);
    let mut out = HashMap::with_capacity(NUM_CRATES);
    for r in csv.records() {
        let r = r?;
        let row = r.deserialize::<UserRow>(None).map_err(|e| format!("{}: {:?}", e, r))?;
        out.insert(row.id, row);
    }
    Ok(out)
}

#[derive(Deserialize, Debug)]
struct CrateVersionRow {
    crate_id: u32,
    crate_size: Option<u64>,
    created_at: String,
    downloads: u64,
    features: String, // json
    id: u32,
    license: String,
    num: String, // ver
    published_by: Option<u32>,
    updated_at: String,
    yanked: char,
}

type VersionsMap = HashMap<u32, Vec<CrateVersionRow>>;

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

type VersionDownloads = HashMap<u32, Vec<(Date<Utc>, u32)>>;

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
        let downloads = r.next().and_then(|s| s.parse().ok()).ok_or("bad dl")?;
        let version_id = r.next().and_then(|s| s.parse().ok()).ok_or("bad dl")?;
        out.entry(version_id).or_insert_with(|| Vec::with_capacity(365 * 4)).push((date, downloads));
    }
    Ok(out)
}

#[inline(never)]
fn parse_metadata(mut file: impl Read) -> Result<u64, BoxErr> {
    let mut s = String::with_capacity(60);
    file.read_to_string(&mut s)?;
    Ok(s.split('\n').nth(1).and_then(|s| s.parse().ok()).ok_or("bad num")?)
}

type CratesMap = HashMap<u32, String>;

#[inline(never)]
fn parse_crates(file: impl Read) -> Result<CratesMap, BoxErr> {
    let mut csv = csv::ReaderBuilder::new().has_headers(true).flexible(false).from_reader(file);
    let mut out = HashMap::with_capacity(NUM_CRATES);
    for r in csv.records() {
        let r = r?;
        let id: u32 = r.get(5).and_then(|s| s.parse().ok()).ok_or("bad record")?;
        let name = r.get(7).ok_or("bad record")?;
        out.insert(id, name.to_owned());
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
        let crate_id: u32 = r.get(0).and_then(|s| s.parse().ok()).ok_or("bad record")?;
        let version_id: u32 = r.get(8).and_then(|s| s.parse().ok()).ok_or("bad record")?;
        out.entry(version_id).or_insert_with(Vec::new).push(crate_id);
    }
    Ok(out)
}
