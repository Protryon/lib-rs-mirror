use crate::templates;
use crate::Page;
use crate::Urler;
use render_readme::Renderer;
use std::io::Write;


pub struct NotFoundPage<'a> {
    markup: &'a Renderer,
    pub results: &'a [search_index::CrateFound],
    pub query: &'a str,
}

impl NotFoundPage<'_> {
    pub fn new<'a>(query: &'a str, results: &'a [search_index::CrateFound], markup: &'a Renderer) -> NotFoundPage<'a> {
        NotFoundPage {
            query,
            markup,
            results,
        }
    }

    pub fn page(&self) -> Page {

        Page {
            title: "Crate not found".into(),
            description: Some("Error".into()),
            item_name: None,
            item_description: None,
            keywords: None,
            created: None,
            alternate: None,
            alternate_type: None,
            canonical: None,
            noindex: true,
            search_meta: true,
            critical_css_data: Some(include_str!("../../style/public/search.css")),
        }
    }

    /// For color of the version
    ///
    /// It tries to guess which versions seem "unstable".
    ///
    /// TODO: Merge with the better version history analysis from the individual crate page.
    pub fn version_class(&self, ver: &str) -> &str {
        let v = semver::Version::parse(ver).expect("semver");
        match (v.major, v.minor, v.patch, v.is_prerelease()) {
            (1..=15, _, _, false) => "stable",
            (0, m, p, false) if m >= 2 && p >= 3 => "stable",
            (m, ..) if m >= 1 => "okay",
            (0, 1, p, _) if p >= 10 => "okay",
            (0, 3..=10, p, _) if p > 0 => "okay",
            _ => "unstable",
        }
    }

    /// Nicely rounded number of downloads
    ///
    /// To show that these numbers are just approximate.
    pub fn downloads(&self, num: u64) -> (String, &str) {
        match num {
            a @ 0..=99 => (format!("{}", a), ""),
            a @ 0..=500 => (format!("{}", a / 10 * 10), ""),
            a @ 0..=999 => (format!("{}", a / 50 * 50), ""),
            a @ 0..=9999 => (format!("{}.{}", a / 1000, a % 1000 / 100), "K"),
            a @ 0..=999_999 => (format!("{}", a / 1000), "K"),
            a => (format!("{}.{}", a / 1_000_000, a % 1_000_000 / 100_000), "M"),
        }
    }

    /// Used to render descriptions
    pub fn render_markdown_str(&self, s: &str) -> templates::Html<String> {
        templates::Html(self.markup.markdown_str(s, false))
    }
}

pub fn render_404_page(out: &mut dyn Write, query: &str, results: &[search_index::CrateFound], markup: &Renderer) -> Result<(), failure::Error> {
    let urler = Urler::new(None);
    let page = NotFoundPage::new(query, results, markup);
    templates::not_found(out, &page, &urler)?;
    Ok(())
}
