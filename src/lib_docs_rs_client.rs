extern crate simple_cache;
extern crate serde;
extern crate serde_json;
#[macro_use] extern crate serde_derive;
use std::path::Path;
use simple_cache::SimpleCache;
use simple_cache::TempCache;
pub use simple_cache::Error;

pub struct DocsRsClient {
    cache_old: SimpleCache,
    cache: TempCache<Option<Vec<BuildStatus>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildStatus {
    build_status: bool, //true,
    build_time: String, //"2018-03-26T08:57:46+02:00",
    rustc_version: String,
}

impl DocsRsClient {
    pub fn new(cache_path: impl AsRef<Path>) -> Result<Self, Error> {
        Ok(Self {
            cache_old: SimpleCache::new(cache_path.as_ref())?,
            cache: TempCache::new(cache_path.as_ref())?,
        })
    }

    pub fn builds(&self, crate_name: &str, version: &str) -> Result<bool, Error> {
        let res = self.build_status(crate_name, version)?;
        Ok(res.and_then(|s| s.get(0).map(|st| st.build_status)).unwrap_or(false))
    }

    pub fn build_status(&self, crate_name: &str, version: &str) -> Result<Option<Vec<BuildStatus>>, Error> {
        let key = format!("{}-{}", crate_name, version);
        if let Some(cached) = self.cache.get(&key)? {
            return Ok(cached)
        }
        let url = format!("https://docs.rs/crate/{}/{}/builds.json", crate_name, version);
        let new = format!("docs.rs/{}", crate_name);
        self.cache_old.get_json((&new, version), &url)
        .and_then(|res| {
            self.cache.set(key, res.clone())?;
            Ok(res)
        })
    }
}

#[test]
fn test_docsrsclient() {
    let client = DocsRsClient::new("../data/cache.db").expect("new");

    assert!(client.builds("libc", "0.2.40").expect("libc"));
    client.build_status("libc", "0.2.40").expect("libc");
}
