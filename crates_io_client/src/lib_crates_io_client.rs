use std::sync::Arc;
pub use simple_cache::Error;

use serde_derive::*;
use simple_cache::SimpleCache;
use simple_cache::TempCacheJson;
use fetcher::Fetcher;

use std::path::Path;
use urlencoding::encode;
mod crate_deps;
mod crate_meta;
mod crate_owners;
pub use crate::crate_deps::*;
pub use crate::crate_meta::*;
pub use crate::crate_owners::*;

pub struct CratesIoClient {
    cache: TempCacheJson<(String, Payload)>,
    crates: SimpleCache,
    readmes: SimpleCache,
    sem: tokio::sync::Semaphore,
}

macro_rules! cioopt {
    ($e:expr) => {
        match $e {
            Some(ok) => ok,
            None => return Ok(None),
        }
    };
}

impl CratesIoClient {
    pub fn new(cache_base_path: &Path) -> Result<Self, Error> {
        let fe = Arc::new(Fetcher::new(4));
        Ok(Self {
            cache: TempCacheJson::new(&cache_base_path.join("cratesio.bin"), fe.clone())?,
            crates: SimpleCache::new(&cache_base_path.join("crates.db"), fe.clone())?,
            readmes: SimpleCache::new(&cache_base_path.join("readmes.db"), fe.clone())?,
            sem: tokio::sync::Semaphore::new(2),
        })
    }

    pub fn cleanup(&self) {
        let _ = self.cache.save();
    }

    pub fn cache_only(&mut self, no_net: bool) -> &mut Self {
        self.cache.set_cache_only(no_net);
        self.crates.cache_only = no_net;
        self
    }

    pub async fn crate_data(&self, crate_name: &str, version: &str) -> Result<Option<Vec<u8>>, Error> {
        let newkey = format!("{}.crate", crate_name);
        // it really uses unencoded names, e.g. + is accepted, %2B is not!
        let url = format!("https://static.crates.io/crates/{name}/{name}-{version}.crate", name = crate_name, version = version);
        let res = self.crates.get_cached((&newkey, version), &url).await?;
        if let Some(data) = &res {
            if data.len() < 10 || data[0] != 31 || data[1] != 139 {
                return Err(Error::Other(format!("Not tarball: {}", url)));
            }
        }
        Ok(res)
    }

    pub async fn readme(&self, crate_name: &str, version: &str) -> Result<Option<Vec<u8>>, Error> {
        let key = format!("{}.html", crate_name);
        let url = format!("https://crates.io/api/v1/crates/{}/{}/readme", encode(crate_name), encode(version));
        self.readmes.get_cached((&key, version), &url).await
    }

    pub async fn crate_meta(&self, crate_name: &str, as_of_version: &str) -> Result<Option<CrateMetaFile>, Error> {
        self.get_json((crate_name, as_of_version), encode(crate_name)).await
    }

    pub async fn crate_owners(&self, crate_name: &str, as_of_version: &str) -> Result<Option<Vec<CrateOwner>>, Error> {
        let url1 = format!("{}/owner_user", encode(crate_name));
        let url2 = format!("{}/owner_team", encode(crate_name));
        let (res1, res2) = futures::join!(
            self.get_json((&url1, as_of_version), &url1),
            self.get_json((&url2, as_of_version), &url2));

        let u: CrateOwnersFile = cioopt!(res1?);
        let mut t: CrateTeamsFile = cioopt!(res2?);
        let mut out = u.users;
        out.append(&mut t.teams);
        Ok(Some(out))
    }

    async fn get_json<B>(&self, key: (&str, &str), path: impl AsRef<str>) -> Result<Option<B>, Error>
        where B: for<'a> serde::Deserialize<'a> + Payloadable
    {
        if let Some((ver, res)) = self.cache.get(key.0)? {
            if self.cache.cache_only() || ver == key.1 {
                return Ok(Some(B::from(res)));
            }
            let wants = semver::Version::parse(key.1);
            let has = semver::Version::parse(&ver);
            if wants.and_then(|wants| has.map(|has| (wants, has))).ok().map_or(false, |(wants, has)| has > wants) {
                eprintln!("Cache regression: {}@{} vs {}", key.0, ver, key.1);
            }
        }

        if self.cache.cache_only() {
            return Err(Error::NotInCache);
        }

        self.cache.delete(key.0)?; // out of date

        let _s = self.sem.acquire().await;

        let url = format!("https://crates.io/api/v1/crates/{}", path.as_ref());
        let res = Box::pin(self.cache.get_json(key.0, url, |raw: B| {
            Some((key.1.to_string(), raw.to()))
        })).await?;
        Ok(res.map(|(_, res)| B::from(res)))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum Payload {
    CrateMetaFile(CrateMetaFile),
    CrateOwnersFile(CrateOwnersFile),
    CrateTeamsFile(CrateTeamsFile),
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

impl Payloadable for CrateTeamsFile {
    fn to(&self) -> Payload { Payload::CrateTeamsFile(self.clone()) }
    fn from(val: Payload) -> Self { match val { Payload::CrateTeamsFile(d) => d, _ => panic!("bad cache") } }
}

#[tokio::test]
async fn cratesioclient() {
    let client = CratesIoClient::new(Path::new("../data")).expect("new");

    client.crate_meta("capi", "0.0.1").await.expect("cargo-deb");
    let owners = client.crate_owners("cargo-deb", "1.10.0").await.expect("crate_owners").expect("found some");
    assert_eq!(3, owners.len(), "that will fail when metadata updates");
    match CratesIoClient::new(Path::new("../data")).expect("new").cache_only(true).crate_data("fail404", "999").await.unwrap() {
        None => {},
        Some(e) => panic!("{:?}", e),
    }
}
