use fetcher::Fetcher;
use render_readme::ImageFilter;
use simple_cache::{Error, TempCacheJson};
use tokio::time::timeout;
use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Copy, Deserialize, Serialize, Debug)]
struct ImageOptimImageMeta {
    width: u32,
    height: u32,
}

/// Filter through https://imageoptim.com/api
pub struct ImageOptimAPIFilter {
    /// Get one from https://imageoptim.com/api/register
    img_prefix: String,
    img2x_prefix: String,
    meta_prefix: String,
    cache: TempCacheJson<ImageOptimImageMeta>,
    handle: tokio::runtime::Handle,
}

impl ImageOptimAPIFilter {
    pub async fn new(api_id: &str, cache_path: impl Into<PathBuf>) -> Result<Self, Error> {
        Ok(Self {
            img_prefix: format!("https://img.gs/{}/full/", api_id),
            img2x_prefix: format!("https://img.gs/{}/full,2x/", api_id),
            meta_prefix: format!("https://img.gs/{}/meta,timeout=3/", api_id),
            cache: TempCacheJson::new(cache_path, Arc::new(Fetcher::new(8)))?,
            handle: tokio::runtime::Handle::current(),
        })
    }
}

impl ImageFilter for ImageOptimAPIFilter {
    fn filter_url<'a>(&self, url: &'a str) -> (Cow<'a, str>, Option<Cow<'a, str>>) {
        // let some badges through, because they're SVG (don't need 2x scaling),
        // and show uncacheable info that needs to be up to date.
        // Can't let them all through, because of CSP.
        if url.starts_with("https://img.shields.io/") && url.contains(".svg") {
            return (url.into(), None);
        }
        (
            format!("{}{}", self.img_prefix, url).into(),
            Some(format!("{}{} 2x", self.img2x_prefix, url).into())
        )
    }

    fn image_size(&self, image_url: &str) -> Option<(u32, u32)> {
        let image_url = image_url.trim_start_matches(&self.img_prefix).trim_start_matches(&self.img2x_prefix);
        let api_url = format!("{}{}", self.meta_prefix, image_url);
        let rt = self.handle.enter();
        let cache_future = timeout(Duration::from_secs(5), self.cache.get_json(image_url, api_url, |f| f));
        let ImageOptimImageMeta { mut width, mut height } = futures::executor::block_on(cache_future)
            .map_err(|_| {
                eprintln!("warning: image req to meta of {} timed out", image_url);
            })
            .ok()?
            .map_err(|e| {
                eprintln!("warning: image req to meta of {} failed: {}", image_url, e);
            })
            .ok()??;
        drop(rt);
        if height > 1000 {
            width /= 2;
            height /= 2;
        }
        if width > 1000 {
            width /= 2;
            height /= 2;
        }
        Some((width, height))
    }
}
