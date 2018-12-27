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
        let half_score = results.get(0).map_or(0., |r| r.score) * 0.4;
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
            critical_css_data: Some(include_str!("../../style/public/search.css")),
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
