#![allow(unused)]
#![allow(dead_code)]
use std::collections::HashSet;
use chrono::prelude::*;
use kitchen_sink::CrateOwner;
use kitchen_sink::KitchenSink;
use kitchen_sink::Origin;
use kitchen_sink::MiniDate;
use kitchen_sink::DependerChanges;
use kitchen_sink::OwnerKind;
use libflate::gzip::Decoder;
use serde_derive::Deserialize;
use std::collections::HashMap;
use std::convert::TryInto;
use std::fs::File;
use std::io::BufReader;
use std::io::Read;
use tar::Archive;

const NUM_CRATES: usize = 40000;
type BoxErr = Box<dyn std::error::Error + Sync + Send>;

#[tokio::main]
async fn main() -> Result<(), BoxErr> {
    tokio::runtime::Handle::current().spawn(async move {
        let handle = tokio::runtime::Handle::current();
        /// I can't be bothered to make async stream adapter to make async body impl Read
        let res = reqwest::blocking::get("https://static.crates.io/db-dump.tar.gz")?;
        let res = BufReader::with_capacity(256_000_000, res);
        let mut a = Archive::new(Decoder::new(res)?);
        let ksink = KitchenSink::new_default().await?;

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
                        p => eprintln!("Ignored file {}", p),
                    };

                    if let (Some(crates), Some(versions)) = (&crates, &versions) {
                        if let Some(dependencies) = dependencies.take() {
                            eprintln!("Indexing dependencies for {} crates", dependencies.len());
                            index_dependencies(crates, versions, &dependencies, &ksink)?;
                        }
                    }


                    if let (Some(crates), Some(versions)) = (&crates, &versions) {
                        if let Some(downloads) = downloads.take() {
                            eprintln!("Indexing {} crates, {} versions, {} downloads", crates.len(), versions.len(), downloads.len());
                            index_downloads(crates, versions, &downloads, &ksink)?;
                        }
                    }
                }
            }

            if let (Some(crates), Some(teams), Some(users)) = (crates, teams, users) {
                if let Some(crate_owners) = crate_owners.take() {
                    eprintln!("Indexing owners of {} crates", crate_owners.len());
                    handle.spawn(async move {
                        index_owners(&crates, crate_owners, &teams, &users, &ksink).await.unwrap();
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

/// Direct reverse dependencies, but with release dates (when first seen or last used)
#[inline(never)]
fn index_dependencies(crates: &CratesMap, versions: &VersionsMap, deps: &CrateDepsMap, ksink: &KitchenSink) -> Result<(), BoxErr> {
    let mut deps_changes = HashMap::with_capacity(crates.len());

    for (crate_id, name) in crates {

        if let Some(vers) = versions.get(crate_id) {
            let mut from_oldest: Vec<_> = vers.iter().filter_map(|v| {
                date_from_str(&v.created_at).ok().map(|date| (v.id, MiniDate::new(date)))
            }).collect();
            from_oldest.sort_by_key(|a| a.0);

            let mut over_time = HashMap::new();
            let last_idx = from_oldest.len()-1;
            for (idx, (ver_id, release_date)) in from_oldest.into_iter().enumerate() {
                // out of order versions (1.1.1 released after 2.0.0) are not an issue
                // because we only mark deps as still-used, not explicit removals based on diffs
                //
                // ..except the last one :(
                let end_date = if idx == last_idx {
                    release_date.next_year()
                } else {
                    release_date
                };

                for &dep_id in deps.get(&ver_id).map(Vec::as_slice).unwrap_or_default() {
                    if dep_id == *crate_id {
                        // libcpocalypse semver trick - not relevant
                        continue;
                    }

                    // (first seen, last seen)
                    over_time.entry(dep_id).or_insert((release_date, end_date)).1 = end_date;
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
        let mut by_day = HashMap::with_capacity(uses.len()*2);
        for (first, last) in uses {
            by_day.entry(first).or_insert(DependerChanges {at: first, added:0, removed:0}).added += 1;
            if last <= today {
                by_day.entry(last).or_insert(DependerChanges {at: last, added:0, removed:0}).removed += 1;
            }
        }
        let mut by_day: Vec<_> = by_day.values().copied().collect();
        by_day.sort_by_key(|a| a.at);

        let origin = Origin::from_crates_io_name(name);
        ksink.index_dependers_liveness_ranges(&origin, by_day);
    }
    Ok(())
}

#[inline(never)]
async fn index_owners(crates: &CratesMap, owners: CrateOwners, teams: &Teams, users: &Users, ksink: &KitchenSink) -> Result<(), BoxErr> {
    for (crate_id, owners) in owners {
        if let Some(k) = crates.get(&crate_id) {
            let owners: Vec<_> = owners
                .into_iter()
                .filter_map(|o| {
                    let invited_at = o.created_at.splitn(2, '.').next().unwrap().to_string(); // trim millis part
                    let invited_by_github_id =
                        o.created_by_id.and_then(|id| users.get(&id).map(|u| u.github_id as u32).or_else(|| teams.get(&id).map(|t| t.github_id)));
                    Some(match o.owner_kind {
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
                            }
                        },
                        1 => {
                            let u = teams.get(&o.owner_id).expect("owner consistency");
                            CrateOwner {
                                login: u.login.to_owned(),
                                invited_at: Some(invited_at),
                                github_id: Some(u.github_id),
                                invited_by_github_id,
                                name: Some(u.name.to_owned()),
                                avatar: None,
                                url: None,
                                kind: OwnerKind::Team,
                            }
                        },
                        _ => panic!("bad owner type"),
                    })
                })
                .collect();
            let origin = Origin::from_crates_io_name(k);
            ksink.index_crates_io_crate_owners(&origin, owners).await?;
        }
    }
    Ok(())
}

#[derive(Deserialize)]
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

#[derive(Deserialize)]
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
        out.entry(version_id).or_insert_with(|| Vec::with_capacity(365*4)).push((date, downloads));
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
