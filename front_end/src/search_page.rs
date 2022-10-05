use ahash::HashMapExt;
use crate::templates;
use crate::Page;
use crate::Urler;
use render_readme::Renderer;
use search_index::SearchResults;
use ahash::HashMap;
use std::io::Write;

pub enum SearchKind<'a> {
    Query(&'a str),
    Keyword(&'a str),
}

pub struct SearchPage<'a> {
    markup: &'a Renderer,
    pub good_results: &'a [search_index::CrateFound],
    pub bad_results: &'a [search_index::CrateFound],
    pub query: SearchKind<'a>,
    dividing_keywords: &'a [String],
    pub normalized_query: Option<&'a str>,
}

impl SearchPage<'_> {
    pub fn new<'a>(query: &'a str, results: &'a SearchResults, markup: &'a Renderer) -> SearchPage<'a> {
        let half_score = results.crates.get(0).map_or(0., |r| r.score) * 0.33;
        let num = results.crates.iter().take_while(|r| r.score >= half_score).count();
        let (good_results, mut bad_results) = results.crates.split_at(num);

        // don't show a long tail of garbage if the results really are bad
        let bad_results_cap = 10 + bad_results.len() / 2;
        bad_results = &bad_results[..bad_results_cap.min(bad_results.len())];

        SearchPage {
            query: SearchKind::Query(query),
            markup,
            good_results,
            bad_results,
            dividing_keywords: &results.keywords,
            normalized_query: results.normalized_query.as_deref(),
        }
    }

    pub fn new_keyword<'a>(keyword: &'a str, results: &'a SearchResults, markup: &'a Renderer) -> SearchPage<'a> {
        SearchPage {
            query: SearchKind::Keyword(keyword),
            markup,
            good_results: &results.crates,
            bad_results: &[],
            dividing_keywords: &results.keywords,
            normalized_query: results.normalized_query.as_deref(),
        }
    }

    pub fn search_also(&self) -> Option<impl Iterator<Item=(String, &str)>> {
        let query = match self.query {
            SearchKind::Query(s) | SearchKind::Keyword(s) => s,
        };
        if self.dividing_keywords.len() < 3 {
            return None;
        }
        Some(self.dividing_keywords.iter().map(move |k| {
            (format!("{query} {k}"), k.as_str())
        }))
    }

    pub fn did_you_mean(&self) -> Option<impl Iterator<Item=(String, &str)>> {
        let query = match self.query {
            SearchKind::Query(s) | SearchKind::Keyword(s) => s,
        }.trim();

        // did you mean is nice for single-word queries,
        // but specific queries give werid niche keywords
        let query_specificity = query.split(' ').count() * 2;
        if self.dividing_keywords.len() < 3 + query_specificity {
            return None;
        }
        let prefix = format!("{query}-");
        let suffix = format!("-{query}");
        Some(self.dividing_keywords.iter()
            .filter(move |k| !k.starts_with(&prefix) && !k.ends_with(&suffix))
            .take(3).map(move |k| {
            (format!("{query} {k}"), k.as_str())
        }))
    }

    pub fn top_keywords(&self) -> Vec<&str> {
        let query = match self.query {
            SearchKind::Query(s) | SearchKind::Keyword(s) => s,
        };
        let query_keywords: Vec<_> = query.split(|c: char| !c.is_alphanumeric()).filter(|k| !k.is_empty())
            .chain(Some(query)).collect();
        let mut counts = HashMap::with_capacity(64);
        for res in self.good_results.iter().chain(self.bad_results.iter()) {
            for keyword in res.keywords.iter() {
                if query_keywords.iter().any(|&qk| qk == keyword) {
                    continue;
                }
                let cnt = counts.entry(keyword.as_str()).or_insert((0u32, 0f32));
                cnt.0 += 1;
                cnt.1 += res.score;
            }
        }
        let obvious_threshold = (self.good_results.len() + self.bad_results.len() / 2) as u32;
        let mut counts: Vec<_> = counts.into_iter()
            // keep if more than 1 crate has it
            // but don't repeat terms from the query
            .filter(|(_, (n, _))| *n > 1 && *n < obvious_threshold)
            .map(|(k, (_, v))| (k,v)).collect();
        counts.sort_unstable_by(|a, b| b.1.total_cmp(&a.1));

        let mut text_len = 0;
        counts.into_iter().map(|(k, _)| k).take_while(|k| {
            text_len += 2 + k.len();
            text_len < 75
        }).collect()
    }

    pub fn page(&self) -> Page {
        let mut desc = String::with_capacity(300);
        match self.query {
            SearchKind::Query(_) => desc.push_str("Found Rust crates: "),
            SearchKind::Keyword(q) => desc.push_str(&format!("#{} = ", q)),
        };
        for r in &self.good_results[0..self.good_results.len().min(10)] {
            desc.push_str(&r.crate_name);
            desc.push_str(", ");
        }
        desc.push_str("etc.");
        Page {
            title: match self.query {
                SearchKind::Query(q) => format!("‘{}’ search", q),
                SearchKind::Keyword(q) => format!("#{}", q),
            },
            description: Some(desc),
            noindex: true,
            search_meta: true,
            critical_css_data: Some(include_str!("../../style/public/search.css")),
            critical_css_dev_url: Some("/search.css"),
            ..Default::default()
        }
    }

    /// For color of the version
    ///
    /// It tries to guess which versions seem "unstable".
    ///
    /// TODO: Merge with the better version history analysis from the individual crate page.
    pub fn version_class(&self, ver: &str) -> &str {
        let v = match semver::Version::parse(ver) {
            Ok(v) => v,
            _ => return "unstable",
        };
        match (v.major, v.minor, v.patch, !v.pre.is_empty()) {
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
    pub fn render_maybe_markdown_str(&self, s: &str) -> templates::Html<String> {
        crate::render_maybe_markdown_str(s, &self.markup, false, None)
    }
}

pub fn render_serp_page(out: &mut dyn Write, query: &str, results: &SearchResults, markup: &Renderer) -> Result<(), anyhow::Error> {
    let urler = Urler::new(None);
    let page = SearchPage::new(query, results, markup);
    templates::serp(out, &page, &urler)?;
    Ok(())
}

pub fn render_keyword_page(out: &mut dyn Write, keyword: &str, results: &SearchResults, markup: &Renderer) -> Result<(), anyhow::Error> {
    let urler = Urler::new(None);
    let page = SearchPage::new_keyword(keyword, results, markup);
    templates::serp(out, &page, &urler)?;
    Ok(())
}
