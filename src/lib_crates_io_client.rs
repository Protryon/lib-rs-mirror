use rand;
use serde;

#[macro_use] extern crate serde_derive;
use std::collections::HashMap;
use std::path::Path;
use chrono::{Date, TimeZone, Utc};

mod crate_meta;
mod crate_deps;
mod crate_owners;
mod crate_downloads;
pub use crate::crate_meta::*;
pub use crate::crate_deps::*;
pub use crate::crate_owners::*;
pub use crate::crate_downloads::*;
pub use simple_cache::Error;
use simple_cache::SimpleCache;
use simple_cache::TempCache;

pub struct CratesIoClient {
    cache: TempCache<(String, Payload)>,
    crates: SimpleCache,
}

macro_rules! cioopt {
    ($e:expr) => {
        match $e {
            Some(ok) => ok,
            None => return Ok(None),
        }
    };
}

#[derive(Debug)]
pub struct CratesIoCrate {
    pub meta: CrateMetaFile,
    pub downloads: CrateDownloadsFile,
    pub owners: Vec<CrateOwner>,
}

impl CratesIoClient {
    pub fn new(cache_base_path: &Path) -> Result<Self, Error> {
        Ok(Self {
            cache: TempCache::new(&cache_base_path.join("cratesio.bin"))?,
            crates: SimpleCache::new(&cache_base_path.join("crates.db"))?,
        })
    }

    pub fn cache_only(&mut self, no_net: bool) -> &mut Self {
        self.cache.cache_only = no_net;
        self.crates.cache_only = no_net;
        self
    }

    pub fn crate_data(&self, crate_name: &str, version: &str) -> Result<Option<Vec<u8>>, Error> {
        let newkey = format!("{}.crate", crate_name);
        let url = format!("https://crates.io/api/v1/crates/{}/{}/download", crate_name, version);
        self.crates.get_cached((&newkey, version), &url)
    }

    pub fn krate(&self, crate_name: &str, cache_buster: &str) -> Result<Option<CratesIoCrate>, Error> {
        let meta = match self.crate_meta(crate_name, cache_buster)? {
            Some(d) => d,
            None => return Ok(None),
        };
        let downloads = match self.crate_downloads(crate_name, cache_buster)? {
            Some(d) => d,
            None => return Ok(None),
        };
        let owners = match self.crate_owners(crate_name, cache_buster)? {
            Some(d) => d,
            None => return Ok(None),
        };
        Ok(Some(CratesIoCrate {
            meta,
            downloads,
            owners,
        }))
    }

    pub fn crate_meta(&self, crate_name: &str, as_of_version: &str) -> Result<Option<CrateMetaFile>, Error> {
        self.get_json((crate_name, as_of_version), crate_name)
    }

    pub fn crate_downloads(&self, crate_name: &str, as_of_version: &str) -> Result<Option<CrateDownloadsFile>, Error> {
        let url = format!("{}/downloads", crate_name);
        let new_key = (url.as_str(), as_of_version);
        let data: CrateDownloadsFile = cioopt!(self.get_json(new_key, &url)?);
        if !self.cache.cache_only && data.is_stale() && rand::random::<u8>() > 100 {
            eprintln!("downloads expired {}@{}", crate_name, as_of_version);
            let _ = self.cache.delete(new_key.0);
            let fresh: CrateDownloadsFile = cioopt!(self.get_json(new_key, &url)?);
            assert!(!fresh.is_stale());
            Ok(Some(fresh))
        } else {
            Ok(Some(data))
        }
    }

    pub fn crate_owners(&self, crate_name: &str, as_of_version: &str) -> Result<Option<Vec<CrateOwner>>, Error> {
        let url = format!("{}/owner_user", crate_name);
        let u: CrateOwnersFile = cioopt!(self.get_json((&url, as_of_version), &url)?);

        let url = format!("{}/owner_team", crate_name);
        let mut t: CrateTeamsFile = cioopt!(self.get_json((&url, as_of_version), &url)?);
        let mut out = u.users;
        out.append(&mut t.teams);
        Ok(Some(out))
    }

    fn get_json<B>(&self, key: (&str, &str), path: impl AsRef<str>) -> Result<Option<B>, Error>
        where B: for<'a> serde::Deserialize<'a> + Payloadable
    {
        if let Some((ver, res)) = self.cache.get(key.0)? {
            if self.cache.cache_only || ver == key.1 {
                return Ok(Some(B::from(res)));
            }
            let wants = semver::Version::parse(key.1);
            let has = semver::Version::parse(&ver);
            if wants.and_then(|wants| has.map(|has| (wants,has)))
                .ok().map_or(false, |(wants,has)| has > wants) {
                eprintln!("Cache regression: {}@{} vs {}" , key.0, ver, key.1);
            }
        }

        let url = format!("https://crates.io/api/v1/crates/{}", path.as_ref());

        let res = self.cache.get_json(key.0, url, |raw: B| {
            Some((key.1.to_string(), raw.to()))
        })?;
        Ok(res.map(|(_, res)| B::from(res)))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum Payload {
    CrateMetaFile(CrateMetaFile),
    CrateOwnersFile(CrateOwnersFile),
    CrateTeamsFile(CrateTeamsFile),
    CrateDownloadsFile(CrateDownloadsFile),
}

pub(crate) trait Payloadable {
    fn to(&self) -> Payload;
    fn from(val: Payload) -> Self;
}

impl Payloadable for CrateMetaFile {
    fn to(&self) -> Payload { Payload::CrateMetaFile(self.clone()) }
    fn from(val: Payload) -> Self { match val { Payload::CrateMetaFile(d) => d, _ => panic!("bad cache") } }
}

impl Payloadable for CrateOwnersFile {
    fn to(&self) -> Payload { Payload::CrateOwnersFile(self.clone()) }
    fn from(val: Payload) -> Self { match val { Payload::CrateOwnersFile(d) => d, _ => panic!("bad cache") } }
}

impl Payloadable for CrateDownloadsFile {
    fn to(&self) -> Payload { Payload::CrateDownloadsFile(self.clone()) }
    fn from(val: Payload) -> Self { match val { Payload::CrateDownloadsFile(d) => d, _ => panic!("bad cache") } }
}

impl Payloadable for CrateTeamsFile {
    fn to(&self) -> Payload { Payload::CrateTeamsFile(self.clone()) }
    fn from(val: Payload) -> Self { match val { Payload::CrateTeamsFile(d) => d, _ => panic!("bad cache") } }
}

pub struct DailyVersionDownload<'a> {
    pub version: Option<&'a CrateMetaVersion>,
    pub downloads: usize,
    pub date: Date<Utc>,
}

impl CratesIoCrate {
    pub fn daily_downloads(&self) -> Vec<DailyVersionDownload<'_>> {
        let versions: HashMap<_,_> = self.meta.versions.iter().map(|v| (v.id, v)).collect();
        self.downloads.version_downloads.iter().map(|d| {
            DailyVersionDownload {
                version: versions.get(&d.version).map(|v| *v),
                downloads: d.downloads,
                date: parse_date(&d.date),
            }
        })
        .chain(
            self.downloads.meta.extra_downloads.iter().map(|d| {
                DailyVersionDownload {
                    version: None,
                    downloads: d.downloads,
                    date: parse_date(&d.date),
                }
            })
        )
        .collect()
    }
}

pub(crate) fn parse_date(date: &str) -> Date<Utc> {
    let y = date[0..4].parse().expect("dl date parse");
    let m = date[5..7].parse().expect("dl date parse");
    let d = date[8..10].parse().expect("dl date parse");
    Utc.ymd(y, m, d)
}

#[test]
fn cratesioclient() {
    let client = CratesIoClient::new(Path::new("../data")).expect("new");

    client.crate_meta("capi", "0.0.1").expect("cargo-deb");
    let owners = client.crate_owners("cargo-deb", "1.10.0").expect("crate_owners").expect("found some");
    assert_eq!(2, owners.len(), "that will fail when metadata updates");
    match CratesIoClient::new(Path::new("../data")).expect("new").cache_only(true).crate_data("fail404", "999").unwrap() {
        None => {},
        Some(e) => panic!("{:?}", e),
    }
}
