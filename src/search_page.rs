use crate::templates;
use crate::Page;
use crate::Urler;
use std::io::Write;
use render_readme::Renderer;

pub struct SearchPage<'a> {
    markup: &'a Renderer,
    pub good_results: &'a [search_index::CrateFound],
    pub bad_results: &'a [search_index::CrateFound],
    pub query: &'a str,
}

impl SearchPage<'_> {
    pub fn new<'a>(query: &'a str, results: &'a [search_index::CrateFound], markup: &'a Renderer) -> SearchPage<'a> {
        let half_score = results.get(0).map_or(0., |r| r.score) * 0.33;
        let num = results.iter().take_while(|r| r.score >= half_score).count();
        let (good_results, bad_results) = results.split_at(num);
        SearchPage {
            query,
            markup,
            good_results,
            bad_results,
        }
    }

    pub fn page(&self) -> Page {
        let mut desc = String::with_capacity(300);
        desc.push_str("Found Rust crates: ");
        for r in &self.good_results[0..self.good_results.len().min(10)] {
            desc.push_str(&r.crate_name);
            desc.push(' ');
        }
        Page {
            title: format!("‘{}’ search", self.query),
            description: Some(desc),
            item_name: None,
            item_description: None,
            keywords: None,
            created: None,
            alternate: None,
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

pub fn render_serp_page(out: &mut dyn Write, query: &str, results: &[search_index::CrateFound], markup: &Renderer) -> Result<(), failure::Error> {
    let urler = Urler::new();
    let page = SearchPage::new(query, results, markup);
    templates::serp(out, &page, &urler)?;
    Ok(())
}
