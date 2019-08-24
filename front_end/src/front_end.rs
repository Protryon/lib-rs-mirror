//! This crate generates HTML templates for crates.rs
//!
//! Most template require their own type of struct that does
//! some lightweight conversion from data model/APIs,
//! because the template engine Ructe doesn't support
//! complex expressions in the templates.

mod cat_page;
mod crate_page;
mod download_graph;
mod home_page;
mod iter;
mod not_found_page;
mod search_page;
mod install_page;
mod urler;
pub use crate::not_found_page::*;
pub use crate::search_page::*;

use categories::Category;
use chrono::prelude::*;
use crate::crate_page::*;
use crate::urler::Urler;
use failure::ResultExt;
use failure;
use kitchen_sink::KitchenSink;
use kitchen_sink::{stopped, KitchenSinkErr};
use render_readme::Markup;
use render_readme::Renderer;
use rich_crate::RichCrate;
use rich_crate::RichCrateVersion;
use std::borrow::Cow;
use std::io::Write;

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
    alternate_type: Option<&'static str>,
    canonical: Option<String>,
    noindex: bool,
    search_meta: bool,
    critical_css_data: Option<&'static str>,
}

impl Page {
    pub fn site_twitter_handle(&self) -> &str {
        "@CratesRS"
    }

    pub fn critical_css(&self) -> templates::Html<&'static str> {
        let data = self.critical_css_data.unwrap_or(include_str!("../../style/public/critical.css"));
        templates::Html(data)
    }
}

/// See `cat_page.rs.html`
pub fn render_category(out: &mut impl Write, cat: &Category, crates: &KitchenSink, renderer: &Renderer) -> Result<(), failure::Error> {
    let urler = Urler::new(None);
    let page = cat_page::CatPage::new(cat, crates, renderer).context("can't prepare rendering of category page")?;
    templates::cat_page(out, &page, &urler)?;
    Ok(())
}

/// See `homepage.rs.html`
pub fn render_homepage<W>(out: &mut W, crates: &KitchenSink) -> Result<(), failure::Error> where W: ?Sized, for<'a> &'a mut W: Write {
    let urler = Urler::new(None);
    templates::homepage(out, &home_page::HomePage::new(crates)?, &urler)?;
    Ok(())
}

/// See `atom.rs.html`
pub fn render_feed(out: &mut impl Write, crates: &KitchenSink) -> Result<(), failure::Error> {
    let urler = Urler::new(None);
    templates::atom(out, &home_page::HomePage::new(crates)?, &urler)?;
    Ok(())
}

pub fn render_sitemap(sitemap: &mut impl Write, crates: &KitchenSink) -> Result<(), failure::Error> {
    let all_crates = crates.sitemap_crates()?;
    let urler = Urler::new(None);

    sitemap.write_all(br#"<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">"#)?;

    let now = Utc::now().timestamp();
    for (origin, rank, lastmod) in all_crates {
        let age = now - lastmod;
        write!(
            sitemap,
            r#"
<url><changefreq>{freq}</changefreq><priority>{pri:0.1}</priority><lastmod>{date}</lastmod><loc>https://lib.rs{url}</loc></url>"#,
            url = urler.crate_by_origin(&origin),
            date = Utc.timestamp(lastmod, 0).to_rfc3339(),
            pri = ((rank - 0.15) * 2.).min(1.),
            freq = match age {
                x if x > 3600 * 24 * 30 * 18 => "yearly",
                x if x > 3600 * 24 * 60 => "monthly",
                x if x > 3600 * 24 * 7 => "weekly",
                _ => "daily",
            },
        )?;
    }

    sitemap.write_all(b"\n</urlset>\n")?;
    Ok(())
}

/// See `crate_page.rs.html`
pub fn render_crate_page<W: Write>(out: &mut W, all: &RichCrate, ver: &RichCrateVersion, kitchen_sink: &KitchenSink, renderer: &Renderer) -> Result<String, failure::Error> {
    if stopped() {
        Err(KitchenSinkErr::Stopped)?;
    }

    let urler = Urler::new(Some(ver.origin().clone()));
    let c = CratePage::new(all, ver, kitchen_sink, renderer).context("New crate page")?;
    templates::crate_page(out, &urler, &c).context("crate page io")?;
    Ok(c.page_title())
}

/// See `install.rs.html`
pub fn render_install_page(out: &mut impl Write, ver: &RichCrateVersion, kitchen_sink: &KitchenSink, renderer: &Renderer) -> Result<(), failure::Error> {
    if stopped() {
        Err(KitchenSinkErr::Stopped)?;
    }
    let urler = Urler::new(None); // Don't set self-crate, because we want to link back to crate page
    let c = crate::install_page::InstallPage::new(ver, kitchen_sink, renderer);
    templates::install(out, &urler, &c).context("install page io")?;
    Ok(())
}

/// See `crate_page.rs.html`
pub fn render_static_page(out: &mut impl Write, title: String, page: &Markup, renderer: &Renderer) -> Result<(), failure::Error> {
    if stopped() {
        Err(KitchenSinkErr::Stopped)?;
    }

    let (html, warnings) = renderer.page(page, Some(("https://lib.rs", "https://lib.rs")), false, None);
    if !warnings.is_empty() {
        eprintln!("static: {:?}", warnings);
    }

    templates::static_page(out, &Page {
        title,
        alternate: None,
        alternate_type: None,
        canonical: None,
        critical_css_data: None,
        created: None,
        description: None,
        item_description: None,
        item_name: None,
        keywords: None,
        noindex: false,
        search_meta: true,
    }, templates::Html(html))?;
    Ok(())
}

pub fn limit_text_len<'t>(text: &'t str, len_min: usize, len_max: usize) -> Cow<'t, str> {
    assert!(len_min <= len_max);
    if text.len() <= len_max {
        return text.into();
    }
    let mut cut = &text[..len_max];
    let optional = &cut[len_min..];
    if let Some(pos) = optional.find(&['.',',','!','\n','?',')',']'][..]).or_else(|| optional.find(' ')) {
        cut = cut[..len_min + pos + 1].trim_end_matches(&['.',',','!','\n','?',' '][..]);
    };
    return format!("{}…", cut).into();
}

#[test]
fn limit_text_len_test() {
    assert_eq!("hello world", limit_text_len("hello world", 100, 200));
    assert_eq!("hel…", limit_text_len("hello world", 1, 3));
    assert_eq!("hello…", limit_text_len("hello world", 1, 10));
    assert_eq!("hello world…", limit_text_len("hello world! long", 1, 15));
    assert_eq!("hello (world)…", limit_text_len("hello (world) long! lorem ipsum", 1, 15));
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
    Utc::now().format("%Y-%m-%d").to_string()
}
