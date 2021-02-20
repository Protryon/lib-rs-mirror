pub use simple_cache::Error;
use fetcher::Fetcher;
use serde_derive::*;
use simple_cache::TempCacheJson;
use std::path::Path;
use std::sync::Arc;

pub struct DocsRsClient {
    cache: TempCacheJson<Option<Vec<BuildStatus>>>,
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
            cache: TempCacheJson::new(cache_path.as_ref(), Arc::new(Fetcher::new(8)))?,
        })
    }

    pub async fn builds(&self, crate_name: &str, version: &str) -> Result<bool, Error> {
        let res = self.build_status(crate_name, version).await?;
        Ok(res.and_then(|s| s.get(0).map(|st| st.build_status)).unwrap_or(false))
    }

    pub async fn build_status(&self, crate_name: &str, version: &str) -> Result<Option<Vec<BuildStatus>>, Error> {
        let key = format!("{}-{}", crate_name, version);
        if let Some(cached) = self.cache.get(key.as_str())? {
            return Ok(cached);
        }
        let url = format!("https://docs.rs/crate/{}/{}/builds.json", crate_name, version);
        Ok(self.cache.get_json(&key, url, |t| t).await?.and_then(|f| f))
    }
}

#[tokio::test]
async fn test_docsrsclient() {
    let client = DocsRsClient::new("../data/docsrs.db").expect("new");

    assert!(client.builds("libc", "0.2.40").await.expect("libc"));
    client.build_status("libc", "0.2.40").await.expect("libc");
}
