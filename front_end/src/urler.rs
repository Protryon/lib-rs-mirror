use categories::Category;
use kitchen_sink::CrateAuthor;
use kitchen_sink::UserType;
use rich_crate::Origin;
use rich_crate::RichCrateVersion;
use rich_crate::RichDep;
use urlencoding::encode;

/// One thing responsible for link URL scheme on the site.
/// Should be used for every internal `<a href>`.
pub struct Urler {
    own_crate_name: Option<String>,
}

impl Urler {
    pub fn new(own_crate_name: Option<String>) -> Self {
        Self { own_crate_name }
    }

    /// Link to a dependency of a crate
    pub fn dependency(&self, dep: &RichDep) -> String {
        format!("/crates/{}", encode(&dep.package))
    }

    /// Summary of all dependencies
    pub fn deps(&self, krate: &RichCrateVersion) -> String {
        format!("https://deps.rs/crate/{}/{}", encode(krate.short_name()), encode(krate.version()))
    }

    pub fn reverse_deps(&self, krate: &RichCrateVersion) -> String {
        format!("https://crates.io/crates/{}/reverse_dependencies", encode(krate.short_name()))
    }

    pub fn crates_io_crate(&self, krate: &RichCrateVersion) -> Option<String> {
        Some(self.crates_io_crate_by_name(krate.short_name()))
    }

    fn crates_io_crate_by_name(&self, crate_name: &str) -> String {
        format!("https://crates.io/crates/{}", encode(crate_name))
    }

    /// Link to crate individual page
    pub fn krate(&self, krate: &RichCrateVersion) -> String {
        self.crate_by_name(krate.short_name())
    }

    pub fn crate_by_origin(&self, o: &Origin) -> String {
        format!("/crates/{}", encode(o.short_crate_name()))
    }

    pub fn crate_by_name(&self, crate_name: &str) -> String {
        if self.own_crate_name.as_ref().map_or(false, |n| n == crate_name) {
            self.crates_io_crate_by_name(crate_name)
        } else {
            format!("/crates/{}", encode(crate_name))
        }
    }

    pub fn keyword(&self, name: &str) -> String {
        format!("/keywords/{}", encode(&name.to_lowercase()))
    }

    /// First page of category listing
    pub fn category(&self, cat: &Category) -> String {
        let mut out = String::with_capacity(1 + cat.slug.len());
        for s in cat.slug.split("::") {
            out.push('/');
            out.push_str(s);
        }
        out
    }

    /// n-th page (1-indexed) of category listing
    pub fn category_page(&self, cat: &Category, p: usize) -> String {
        if p < 2 {
            return self.category(cat);
        }
        format!("https://crates.io/categories/{}?page={}", cat.slug, p)
    }

    /// Crate author's URL
    ///
    /// This will probably change to a listing page rather than arbitrary personal URL
    pub fn author(&self, author: &CrateAuthor<'_>) -> Option<String> {
        if let Some(ref gh) = author.github {
            Some(match gh.user_type {
                UserType::User => format!("https://crates.io/users/{}", encode(&gh.login)),
                UserType::Org | UserType::Bot => format!("https://github.com/{}", encode(&gh.login)),
            })
        } else if let Some(ref info) = author.info {
            if let Some(ref em) = info.email {
                Some(format!("mailto:{}", em)) // add name? encode?
            } else if let Some(ref url) = info.url {
                assert!(url.starts_with("http:") || url.starts_with("https:") || url.starts_with("mailto:"));
                Some(url.to_owned())
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn search_crates_io(&self, query: &str) -> String {
        format!("https://crates.io/search?q={}", encode(query))
    }

    pub fn search_crates_rs(&self, query: &str) -> String {
        format!("https://lib.rs/search?q={}", encode(query))
    }

    pub fn search_ddg(&self, query: &str) -> String {
        format!("https://duckduckgo.com/?q=site%3Alib.rs+{}", encode(query))
    }
}
