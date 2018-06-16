extern crate serde;
extern crate simple_cache;
extern crate chrono;
#[macro_use] extern crate serde_derive;
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
            cache: SimpleCache::new(cache_base_path, "cache.db")?,
            crates: SimpleCache::new(cache_base_path, "crates.db")?,
        })
    }

    pub fn cache_only(&mut self, no_net: bool) -> &mut Self {
        self.cache.cache_only = no_net;
        self
    }

    pub fn crate_data(&self, crate_name: &str, version: &str) -> Result<Vec<u8>, Error> {
        let key = format!("crates/{}-{}.crate", crate_name, version);
        let url = format!("https://crates.io/api/v1/crates/{}/{}/download", crate_name, version);
        self.crates.get_cached(&key, &url)
    }

    pub fn krate(&self, crate_name: &str, cache_buster: &str) -> Result<CratesIoCrate, Error> {
        Ok(CratesIoCrate {
            meta: self.crate_meta(crate_name, cache_buster)?,
            downloads: self.crate_downloads(crate_name, cache_buster)?,
            owners: self.crate_owners(crate_name, cache_buster)?,
        })
    }

    pub fn crate_meta(&self, crate_name: &str, as_of_version: &str) -> Result<CrateMetaFile, Error> {
        self.get_json(&format!("meta/{}{}.json", crate_name, as_of_version), crate_name)
    }

    pub fn crate_downloads(&self, crate_name: &str, as_of_version: &str) -> Result<CrateDownloadsFile, Error> {
        self.get_json(&format!("down/{}.d{}.json", crate_name, as_of_version), format!("{}/downloads", crate_name))
    }

    pub fn crate_owners(&self, crate_name: &str, as_of_version: &str) -> Result<Vec<CrateOwner>, Error> {
        let u: CrateOwnersFile = self.get_json(&format!("user/{}.u{}.json", crate_name, as_of_version), format!("{}/owner_user", crate_name))?;
        let mut t: CrateTeamsFile = self.get_json(&format!("user/{}.t{}.json", crate_name, as_of_version), format!("{}/owner_team", crate_name))?;
        let mut out = u.users;
        out.append(&mut t.teams);
        Ok(out)
    }

    fn get_json<B>(&self, cache_name: &str, path: impl AsRef<str>) -> Result<B, Error>
        where B: for<'a> serde::Deserialize<'a>
    {
        let url = format!("https://crates.io/api/v1/crates/{}", path.as_ref());
        self.cache.get_json(cache_name, url)
    }
}

#[test]
fn cratesioclient() {
    let client = CratesIoClient::new("../data");

    client.crate_meta("capi", "0.0.1").expect("cargo-deb");
    let owners = client.crate_owners("cargo-deb", "1.10.0").expect("crate_owners");
    assert_eq!(2, owners.len(), "that will fail when metadata updates");
    match CratesIoClient::new("../data").cache_only(true).crate_data("fail404","999") {
        Err(Error::NotCached) => {},
        e => panic!("{:?}", e),
    }
}
