extern crate serde;
extern crate rand;
extern crate simple_cache;
extern crate chrono;
#[macro_use] extern crate serde_derive;
use chrono::TimeZone;
use chrono::Utc;
use chrono::Date;
use std::collections::HashMap;
use std::path::Path;

mod crate_meta;
mod crate_deps;
mod crate_owners;
mod crate_downloads;
pub use crate_meta::*;
pub use crate_deps::*;
pub use crate_owners::*;
pub use crate_downloads::*;
pub use simple_cache::Error;
use simple_cache::SimpleCache;

#[derive(Debug)]
pub struct CratesIoClient {
    cache: SimpleCache,
    crates: SimpleCache,
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
            cache: SimpleCache::new(&cache_base_path.join("cache.db"))?,
            crates: SimpleCache::new(&cache_base_path.join("crates.db"))?,
        })
    }

    pub fn cache_only(&mut self, no_net: bool) -> &mut Self {
        self.cache.cache_only = no_net;
        self.crates.cache_only = no_net;
        self
    }

    pub fn crate_data(&self, crate_name: &str, version: &str) -> Result<Vec<u8>, Error> {
        let oldkey = format!("crates/{}-{}.crate", crate_name, version);
        let newkey = format!("{}.crate", crate_name);
        let url = format!("https://crates.io/api/v1/crates/{}/{}/download", crate_name, version);
        self.crates.get_cached((&newkey, version), &oldkey, &url)
    }

    pub fn krate(&self, crate_name: &str, cache_buster: &str) -> Result<CratesIoCrate, Error> {
        Ok(CratesIoCrate {
            meta: self.crate_meta(crate_name, cache_buster)?,
            downloads: self.crate_downloads(crate_name, cache_buster)?,
            owners: self.crate_owners(crate_name, cache_buster)?,
        })
    }

    pub fn crate_meta(&self, crate_name: &str, as_of_version: &str) -> Result<CrateMetaFile, Error> {
        let old = format!("meta/{}{}.json", crate_name, as_of_version);
        self.get_json(&old, (crate_name, as_of_version), crate_name)
    }

    pub fn crate_downloads(&self, crate_name: &str, as_of_version: &str) -> Result<CrateDownloadsFile, Error> {
        let old = format!("{}-{}/downloads", crate_name, as_of_version);
        let url = format!("{}/downloads", crate_name);
        let new_key = (url.as_str(), as_of_version);
        let data: CrateDownloadsFile = self.get_json(&old, new_key, &url)?;
        if data.is_stale() && rand::random::<u8>() > 200 {
            let _ = self.cache.delete(new_key, &old);
            let fresh: CrateDownloadsFile = self.get_json(&old, new_key, &url)?;
            assert!(!fresh.is_stale());
            Ok(fresh)
        } else {
            Ok(data)
        }
    }

    pub fn crate_owners(&self, crate_name: &str, as_of_version: &str) -> Result<Vec<CrateOwner>, Error> {
        let old = format!("user/{}.u{}.json", crate_name, as_of_version);
        let url = format!("{}/owner_user", crate_name);
        let u: CrateOwnersFile = self.get_json(&old, (&url, as_of_version), &url)?;

        let old = format!("user/{}.t{}.json", crate_name, as_of_version);
        let url = format!("{}/owner_team", crate_name);
        let mut t: CrateTeamsFile = self.get_json(&old, (&url, as_of_version), &url)?;
        let mut out = u.users;
        out.append(&mut t.teams);
        Ok(out)
    }

    fn get_json<B>(&self, old_key: &str, key: (&str, &str), path: impl AsRef<str>) -> Result<B, Error>
        where B: for<'a> serde::Deserialize<'a>
    {
        let url = format!("https://crates.io/api/v1/crates/{}", path.as_ref());
        self.cache.get_json(key, old_key, url)
    }
}

pub struct DailyVersionDownload<'a> {
    pub version: Option<&'a CrateMetaVersion>,
    pub downloads: usize,
    pub date: Date<Utc>,
}

impl CratesIoCrate {
    pub fn daily_downloads(&self) -> Vec<DailyVersionDownload> {
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
    Utc.ymd(y,m,d)
}


#[test]
fn cratesioclient() {
    let client = CratesIoClient::new(Path::new("../data")).expect("new");

    client.crate_meta("capi", "0.0.1").expect("cargo-deb");
    let owners = client.crate_owners("cargo-deb", "1.10.0").expect("crate_owners");
    assert_eq!(2, owners.len(), "that will fail when metadata updates");
    match CratesIoClient::new(Path::new("../data")).expect("new").cache_only(true).crate_data("fail404","999") {
        Err(Error::NotCached) => {},
        e => panic!("{:?}", e),
    }
}
