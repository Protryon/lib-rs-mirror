#![allow(unused)]
#![allow(dead_code)]
use chrono::prelude::*;
use kitchen_sink::KitchenSink;
use libflate::gzip::Decoder;
use serde_derive::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::io::Read;
use tar::Archive;

const NUM_CRATES: usize = 40000;
type BoxErr = Box<dyn std::error::Error + Sync + Send>;

#[tokio::main]
async fn main() -> Result<(), BoxErr> {
    let mut a = Archive::new(Decoder::new(BufReader::new(File::open("db-dump.tar.gz")?))?);
    let ksink = KitchenSink::new_default().await?;

    let mut crate_owners = None;
    let mut crates = None;
    let mut metadata = None;
    let mut teams = None;
    let mut users = None;
    let mut downloads = None;
    let mut versions = None;

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
                if let Some(downloads) = downloads.take() {
                    eprintln!("Indexing {} crates, {} versions, {} downloads", crates.len(), versions.len(), downloads.len());
                    index_downloads(crates, versions, &downloads, &ksink)?;
                }
            }
        }
    }
    Ok(())
}

#[inline(never)]
fn index_downloads(crates: &CratesMap, versions: &VersionsMap, downloads: &VersionDownloads, ksink: &KitchenSink) -> Result<(), BoxErr> {
    for (crate_id, name) in crates {
        let mut by_day = HashMap::new();
        if let Some(v) = versions.get(crate_id) {
            for version in v {
                if let Some(d) = downloads.get(&version.id) {
                    for (day, dl) in d.iter().cloned() {
                        *by_day.entry(day).or_insert(0) += dl;
                    }
                }
            }
        } else {
            eprintln!("Bad crate? {} {}", crate_id, name);
        }
        ksink.index_crate_downloads(name, &by_day)?;
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

#[inline(never)]
fn parse_crate_owners(file: impl Read) -> Result<HashMap<u32, CrateOwnerRow>, BoxErr> {
    let mut csv = csv::ReaderBuilder::new().has_headers(true).flexible(false).from_reader(file);
    let mut out = HashMap::with_capacity(NUM_CRATES);
    for r in csv.records() {
        let r = r?;
        let r = r.deserialize::<CrateOwnerRow>(None).map_err(|e| format!("wat? {:#?} {}", r, e))?;
        out.insert(r.crate_id, r);
    }
    Ok(out)
}

#[derive(Deserialize)]
struct TeamRow {
    avatar: String,
    github_id: u32,
    id: u32,
    login: String,
    name: String,
}

#[inline(never)]
fn parse_teams(file: impl Read) -> Result<HashMap<u32, TeamRow>, BoxErr> {
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
    github_id: i32, // -1 happens :(
    login: String,
    id: u32,
    name: String,
}

#[inline(never)]
fn parse_users(file: impl Read) -> Result<HashMap<u32, UserRow>, BoxErr> {
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

#[inline(never)]
fn parse_version_downloads(mut file: impl Read) -> Result<VersionDownloads, BoxErr> {
    let mut csv = csv::ReaderBuilder::new().has_headers(true).flexible(false).from_reader(file);
    let mut out = HashMap::with_capacity(NUM_CRATES);
    for r in csv.records() {
        let r = r?;
        let mut r = r.iter();
        let date = r.next().ok_or("no date")?;
        let y = date[0..4].parse()?;
        let m = date[5..7].parse()?;
        let d = date[8..10].parse()?;
        let date = Utc.ymd(y, m, d);
        let downloads = r.next().and_then(|s| s.parse().ok()).ok_or("bad dl")?;
        let version_id = r.next().and_then(|s| s.parse().ok()).ok_or("bad dl")?;
        out.entry(version_id).or_insert_with(Vec::new).push((date, downloads));
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
