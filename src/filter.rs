use std::path::Path;
use std::borrow::Cow;
use simple_cache::{SimpleCache, Error};

/// Callbacks for every image URL in the document
pub trait ImageFilter: Send + Sync + 'static {
    /// Ability to change the image URL
    fn filter_url<'a>(&self, url: &'a str) -> Cow<'a, str>;

    /// Given the URL, get image size in CSS pixels
    ///
    /// This will be used to add `width`/`height` attributes to `<img>` elements.
    fn image_size(&self, url: &str) -> Option<(u32, u32)>;
}

impl ImageFilter for () {
    fn filter_url<'a>(&self, url: &'a str) -> Cow<'a, str> {
        url.into()
    }
    fn image_size(&self, _url: &str) -> Option<(u32, u32)> {
        None
    }
}


#[derive(Deserialize, Debug)]
struct ImageOptimImageMeta {
    width: u32,
    height: u32,
}

/// Filter through https://imageoptim.com/api
#[derive(Debug)]
pub struct ImageOptimAPIFilter {
    /// Get one from https://imageoptim.com/api/register
    api_id: &'static str,
    cache: SimpleCache,
}

impl ImageOptimAPIFilter {
    pub fn new(api_id: &'static str, cache_path: &Path) -> Result<Self, Error> {
        Ok(Self {
            api_id,
            cache: SimpleCache::new(cache_path)?,
        })
    }
}

impl ImageFilter for ImageOptimAPIFilter {
    fn filter_url<'a>(&self, url: &'a str) -> Cow<'a, str> {
        format!("https://img.gs/{}/full/{}", self.api_id, url).into()
    }

    fn image_size(&self, url: &str) -> Option<(u32, u32)> {
        let api_url = format!("https://img.gs/{}/meta,timeout=90/{}", self.api_id, url);
        self.cache.get_json((url, ""), &url, api_url)
            .map_err(|e| {
                eprintln!("warning: image req to meta of {} failed: {}", url, e);
            })
            .ok()
            .map(|ImageOptimImageMeta{mut width, mut height}| {
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
