use std::time::Duration;
use fetcher::Fetcher;
use serde_derive::*;
pub use simple_cache::Error;
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
            cache: TempCacheJson::new(cache_path.as_ref(), Arc::new(Fetcher::new(8)), Duration::from_secs(3600*24*31*3))?,
        })
    }

    pub async fn builds(&self, crate_name: &str, version: &str) -> Result<bool, Error> {
        let res = self.build_status(crate_name, version).await?;
        Ok(res.map_or(false, |s| s.iter().any(|st| st.build_status)))
    }

    pub async fn build_status(&self, crate_name: &str, version: &str) -> Result<Option<Vec<BuildStatus>>, Error> {
        let key = format!("{}-{}", crate_name, version);
        // Don't cache 404s, since builds can appear later
        if let Some(Some(cached)) = self.cache.get(key.as_str())? {
            return Ok(Some(cached));
        }
        let url = format!("https://docs.rs/crate/{}/{}/builds.json", crate_name, version);
        Ok(self.cache.get_json(&key, url, |t| t).await?.and_then(|f| f))
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_docsrsclient() {
    let client = DocsRsClient::new("../data/docsrs.db").expect("new");

    assert!(client.builds("libc", "0.2.40").await.expect("libc"));
    client.build_status("libc", "0.2.40").await.expect("libc");
}
