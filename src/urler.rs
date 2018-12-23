use rich_crate::RichCrateVersion;
use categories::Category;
use kitchen_sink::UserType;
use urlencoding::encode;
use kitchen_sink::CrateAuthor;
use rich_crate::RichDep;

/// One thing responsible for link URL scheme on the site.
/// Should be used for every internal `<a href>`.
pub struct Urler {
}

impl Urler {
    pub fn new() -> Self {
        Self {}
    }

    /// Link to a dependency of a crate
    pub fn dependency(&self, dep: &RichDep) -> String {
        format!("/crates/{}", encode(&dep.name))
    }

    /// Summary of all dependencies
    pub fn deps(&self, krate: &RichCrateVersion) -> String {
        format!("https://deps.rs/crate/{}/{}", encode(krate.short_name()), encode(krate.version()))
    }

    pub fn reverse_deps(&self, krate: &RichCrateVersion) -> String {
        format!("https://crates.io/crates/{}/reverse_dependencies", encode(krate.short_name()))
    }

    /// Link to crate individual page
    pub fn krate(&self, krate: &RichCrateVersion) -> String {
        format!("/crates/{}", encode(krate.short_name()))
    }

    pub fn keyword(&self, name: &str) -> String {
        format!("https://crates.io/keywords/{}", encode(name))
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
}
