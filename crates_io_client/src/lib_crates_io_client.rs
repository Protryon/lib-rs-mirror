use serde::Serialize;
use serde::de::DeserializeOwned;
use std::borrow::Cow;
use std::path::PathBuf;
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
    fetcher: Arc<Fetcher>,
    cache: TempCacheJson<(String, Payload)>,
    metacache: TempCacheJson<(String, CrateMetaFile)>,
    tarballs_path: PathBuf,
    readmes: SimpleCache,
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
        let fetcher = Arc::new(Fetcher::new(4));
        Ok(Self {
            cache: TempCacheJson::new(&cache_base_path.join("cratesio.bin"), fetcher.clone())?,
            metacache: TempCacheJson::new(&cache_base_path.join("cratesiometa.bin"), fetcher.clone())?,
            readmes: SimpleCache::new(&cache_base_path.join("readmes.db"), fetcher.clone())?,
            tarballs_path: cache_base_path.join("tarballs"),
            fetcher,
        })
    }

    pub fn cleanup(&self) {
        let _ = self.cache.save();
        let _ = self.metacache.save();
    }

    pub fn cache_only(&mut self, no_net: bool) -> &mut Self {
        self.cache.set_cache_only(no_net);
        self.metacache.set_cache_only(no_net);
        self.readmes.cache_only = no_net;
        self
    }

    pub async fn crate_data(&self, crate_name: &str, version: &str) -> Result<Vec<u8>, Error> {
        let tarball_path = self.tarballs_path.join(format!("{}/{}.crate", fs_safe(crate_name), fs_safe(version)));
        if let Ok(data) = std::fs::read(&tarball_path) {
            return Ok(data);
        }

        if self.cache.cache_only() {
            return Err(Error::NotInCache);
        }

        // it really uses unencoded names, e.g. + is accepted, %2B is not!
        let url = format!("https://static.crates.io/crates/{name}/{name}-{version}.crate", name = crate_name, version = version);
        let data = self.fetcher.fetch(&url).await?;
        if data.len() < 10 || data[0] != 31 || data[1] != 139 {
            return Err(Error::Other(format!("Not tarball: {}", url)));
        }
        let _ = std::fs::create_dir_all(tarball_path.parent().unwrap());
        std::fs::write(&tarball_path, &data)?;
        Ok(data)
    }

    pub async fn readme(&self, crate_name: &str, version: &str) -> Result<Option<Vec<u8>>, Error> {
        let key = format!("{}.html", crate_name);
        let url = format!("https://crates.io/api/v1/crates/{}/{}/readme", encode(crate_name), encode(version));
        self.readmes.get_cached((&key, version), &url).await
    }

    pub async fn crate_meta(&self, crate_name: &str, as_of_version: &str) -> Result<Option<CrateMetaFile>, Error> {
        let encoded_name = encode(crate_name);
        self.get_json_from(&self.metacache, (&encoded_name, as_of_version), &encoded_name).await
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
            if ver == key.1 || self.cache.cache_only() {
                return Ok(Some(B::from(res)));
            }
        }

        if self.cache.cache_only() {
            return Err(Error::NotInCache);
        }

        self.cache.delete(key.0)?; // out of date

        let url = format!("https://crates.io/api/v1/crates/{}", path.as_ref());
        let res = Box::pin(self.cache.get_json(key.0, url, |raw: B| {
            Some((key.1.to_string(), raw.to()))
        })).await?;
        Ok(res.map(|(_, res)| B::from(res)))
    }

    async fn get_json_from<B: Serialize + DeserializeOwned + Clone + Send>(&self, from_cache: &TempCacheJson<(String, B)>, key: (&str, &str), path: impl AsRef<str>) -> Result<Option<B>, Error> {
        if let Some((ver, res)) = from_cache.get(key.0)? {
            if ver == key.1 || from_cache.cache_only() {
                return Ok(Some(res));
            }
        }

        if from_cache.cache_only() {
            return Err(Error::NotInCache);
        }

        from_cache.delete(key.0)?; // out of date

        let url = format!("https://crates.io/api/v1/crates/{}", path.as_ref());
        let res = from_cache.get_json(key.0, url, |raw: B| Some((key.1.to_string(), raw))).await?;
        Ok(res.map(|(_, res)| res))
    }
}

fn fs_safe(name: &str) -> Cow<str> {
    if name.as_bytes().iter().all(|&c| c >= b' ' && c != b'/' && c != b'\\' && c < 0x7f) {
        return name.into();
    } else {
        name.as_bytes().iter().map(|b| format!("{:02x}", b)).collect::<String>().into()
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

    client.crate_meta("capi", "0.0.1").await.unwrap();
    let owners = client.crate_owners("cargo-deb", "1.10.0").await.expect("crate_owners").expect("found some");
    assert_eq!(3, owners.len(), "that will fail when metadata updates");
    assert!(CratesIoClient::new(Path::new("../data")).expect("new").cache_only(true).crate_data("fail404", "999").await.is_err());
}
