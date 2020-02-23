use simple_cache::{Error, TempCache};
use std::borrow::Cow;
use std::path::PathBuf;

/// Callbacks for every image URL in the document
pub trait ImageFilter: Send + Sync + 'static {
    /// Ability to change the image URL
    /// Returns 1x image and 2x image
    fn filter_url<'a>(&self, url: &'a str) -> (Cow<'a, str>, Option<Cow<'a, str>>);

    /// Given the URL, get image size in CSS pixels
    ///
    /// This will be used to add `width`/`height` attributes to `<img>` elements.
    fn image_size(&self, url: &str) -> Option<(u32, u32)>;
}

impl ImageFilter for () {
    fn filter_url<'a>(&self, url: &'a str) -> (Cow<'a, str>, Option<Cow<'a, str>>) {
        (url.into(), None)
    }

    fn image_size(&self, _url: &str) -> Option<(u32, u32)> {
        None
    }
}

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
    cache: TempCache<ImageOptimImageMeta>,
    handle: tokio::runtime::Handle,
}

impl ImageOptimAPIFilter {
    pub async fn new(api_id: &str, cache_path: impl Into<PathBuf>) -> Result<Self, Error> {
        Ok(Self {
            img_prefix: format!("https://img.gs/{}/full/", api_id),
            img2x_prefix: format!("https://img.gs/{}/full,2x/", api_id),
            meta_prefix: format!("https://img.gs/{}/meta,timeout=90/", api_id),
            cache: TempCache::new(cache_path)?,
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
        self.handle.enter(|| futures::executor::block_on(self.cache.get_json(image_url, api_url, |f| f)))
            .map_err(|e| {
                eprintln!("warning: image req to meta of {} failed: {}", image_url, e);
            })
            .ok()
            .and_then(|f| f)
            .map(|ImageOptimImageMeta { mut width, mut height }| {
                if height > 1000 {
                    width /= 2;
                    height /= 2;
                }
                if width > 1000 {
                    width /= 2;
                    height /= 2;
                }
                (width, height)
            })
    }
}