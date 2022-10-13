use fetcher::Fetcher;
use render_readme::ImageFilter;
use simple_cache::{Error, TempCacheJson};
use tokio::time::timeout;
use std::borrow::Cow;
use std::path::Path;
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
    pub async fn new(api_id: &str, cache_path: impl AsRef<Path>) -> Result<Self, Error> {
        Ok(Self {
            img_prefix: format!("https://img.gs/{api_id}/full/"),
            img2x_prefix: format!("https://img.gs/{api_id}/full,2x/"),
            meta_prefix: format!("https://img.gs/{api_id}/meta,timeout=3/"),
            cache: TempCacheJson::new(cache_path, Arc::new(Fetcher::new(8)), Duration::from_secs(3600*24*15))?,
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
            format!("{}{url}", self.img_prefix).into(),
            Some(format!("{}{url} 2x", self.img2x_prefix).into())
        )
    }

    fn image_size(&self, image_url: &str) -> Option<(u32, u32)> {
        let image_url = image_url.trim_start_matches(&self.img_prefix).trim_start_matches(&self.img2x_prefix);
        let api_url = format!("{}{image_url}", self.meta_prefix);
        let rt = self.handle.enter();
        let cache_future = timeout(Duration::from_secs(5), self.cache.get_json(image_url, api_url, |f| f));
        let ImageOptimImageMeta { mut width, mut height } = futures::executor::block_on(cache_future)
            .map_err(|_| {
                eprintln!("warning: image req to meta of {image_url} timed out");
            })
            .ok()?
            .map_err(|e| {
                eprintln!("warning: image req to meta of {image_url} failed: {e}");
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
