//! This crate generates HTML templates for crates.rs
//!
//! Most template require their own type of struct that does
//! some lightweight conversion from data model/APIs,
//! because the template engine Ructe doesn't support
//! complex expressions in the templates.

extern crate categories;
extern crate chrono;
extern crate failure;
extern crate kitchen_sink;
extern crate lab;
extern crate rayon;
extern crate render_readme;
extern crate rich_crate;
extern crate semver;
extern crate semver_parser;
extern crate urlencoding;
extern crate locale;
extern crate url;

mod cat_page;
mod crate_page;
mod download_graph;
mod home_page;
mod iter;
mod urler;
use kitchen_sink::{stopped, KitchenSinkErr};
use failure::ResultExt;
use categories::Category;
use crate_page::*;
use kitchen_sink::KitchenSink;
use render_readme::Renderer;
use rich_crate::RichCrate;
use rich_crate::RichCrateVersion;
use std::fs::read_to_string;
use std::io::Write;
use urler::Urler;

include!(concat!(env!("OUT_DIR"), "/templates.rs"));

/// Metadata used in the base template, mostly for `<meta>`
pub struct Page {
    title: String,
    description: Option<String>,
    item_name: Option<String>,
    item_description: Option<String>,
    keywords: Option<String>,
    created: Option<String>,
    alternate: Option<String>,
    canonical: Option<String>,
    noindex: bool,
    alt_critical_css: Option<String>,
}

impl Page {
    pub fn site_twitter_handle(&self) -> &str {
        "@CratesRS"
    }

    pub fn critical_css(&self) -> templates::Html<String> {
        let path = self.alt_critical_css.as_ref().map(|s| s.as_str())
            .unwrap_or("../style/public/critical.css");
        templates::Html(read_to_string(path).expect(path))
    }
}

/// See `cat_page.rs.html`
pub fn render_category(out: &mut Write, cat: &Category, crates: &KitchenSink, markup: &Renderer) -> Result<(), failure::Error> {
    let urler = Urler::new();
    let page = cat_page::CatPage::new(cat, crates, markup).context("can't prepare rendering of category page")?;
    templates::cat_page(out, &page, &urler)?;
    Ok(())
}

/// See `homepage.rs.html`
pub fn render_homepage(out: &mut Write, crates: &KitchenSink) -> Result<(), failure::Error> {
    let urler = Urler::new();
    templates::homepage(out, &home_page::HomePage::new(crates)?, &urler)?;
    Ok(())
}

/// See `crate_page.rs.html`
pub fn render_crate_page(out: &mut Write, all: &RichCrate, ver: &RichCrateVersion, kitchen_sink: &KitchenSink, markup: &Renderer) -> Result<String, failure::Error> {
    if stopped() {
        Err(KitchenSinkErr::Stopped)?;
    }

    let urler = Urler::new();
    let c = CratePage {
        top_keyword: kitchen_sink.top_keyword(all).context("top keyword")?,
        all_contributors: kitchen_sink.all_contributors(ver).context("all contrib")?,
        all, ver, kitchen_sink, markup,
    };
    templates::crate_page(out, &urler, &c).context("crate page io")?;
    Ok(c.page_title())
}


/// Ructe doesn't like complex expressions…
trait MyAsStr {
    fn as_str(&self) -> Option<&str>;
}

impl<S: AsRef<str>> MyAsStr for Option<S> {
    fn as_str(&self) -> Option<&str> {
        self.as_ref().map(|s| s.as_ref())
    }
}

pub(crate) fn date_now() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}
