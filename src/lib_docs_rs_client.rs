extern crate simple_cache;
extern crate serde;
extern crate serde_json;
#[macro_use] extern crate serde_derive;
use std::path::Path;
use simple_cache::SimpleCache;
pub use simple_cache::Error;

#[derive(Debug)]
pub struct DocsRsClient {
    cache: SimpleCache,
}

#[derive(Debug, Deserialize)]
pub struct BuildStatus {
    build_status: bool, //true,
    build_time: String, //"2018-03-26T08:57:46+02:00",
    rustc_version: String,
}

impl DocsRsClient {
    pub fn new(cache_path: impl AsRef<Path>) -> Result<Self, Error> {
        Ok(Self {
            cache: SimpleCache::new(cache_path.as_ref())?,
        })
    }

    pub fn builds(&self, crate_name: &str, version: &str) -> Result<bool, Error> {
        Ok(self.build_status(crate_name, version)?.get(0).map(|st| st.build_status).unwrap_or(false))
    }

    pub fn build_status(&self, crate_name: &str, version: &str) -> Result<Vec<BuildStatus>, Error> {
        let old = format!("meta/{}-{}.docsrs.json", crate_name, version);
        let url = format!("https://docs.rs/crate/{}/{}/builds.json", crate_name, version);
        let new = format!("docs.rs/{}", crate_name);
        self.cache.get_json((&new, version), &old, url)
    }
}

#[test]
fn test_docsrsclient() {
    let client = DocsRsClient::new("../data/cache.db").expect("new");

    assert!(client.builds("libc", "0.2.40").expect("libc"));
    client.build_status("libc", "0.2.40").expect("libc");
}
