//! This crate generates HTML templates for crates.rs
//!
//! Most template require their own type of struct that does
//! some lightweight conversion from data model/APIs,
//! because the template engine Ructe doesn't support
//! complex expressions in the templates.

mod author_page;
mod cat_page;
mod crate_page;
mod download_graph;
mod home_page;
mod install_page;
mod iter;
mod not_found_page;
mod reverse_dependencies;
mod search_page;
mod urler;
pub use crate::not_found_page::*;
pub use crate::search_page::*;
use futures::future::try_join_all;

use crate::author_page::*;
use crate::crate_page::*;
use crate::urler::Urler;
use categories::Category;
use chrono::prelude::*;
use failure;
use failure::ResultExt;
use kitchen_sink::Compat;
use kitchen_sink::RichAuthor;
use kitchen_sink::KitchenSink;
use kitchen_sink::{stopped, KitchenSinkErr};
use render_readme::Links;
use render_readme::Markup;
use render_readme::Renderer;
use rich_crate::RichCrate;
use rich_crate::RichCrateVersion;
use semver::Version as SemVer;
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::io::Write;

include!(concat!(env!("OUT_DIR"), "/templates.rs"));

/// Metadata used in the base template, mostly for `<meta>`
#[derive(Default)]
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
    critical_css_dev_url: Option<&'static str>,
    local_css_data: Option<&'static str>,
}

impl Page {
    pub fn site_twitter_handle(&self) -> &str {
        "@CratesRS"
    }

    pub fn critical_css(&self) -> templates::Html<&'static str> {
        #[cfg(debug_assertions)]
        {
            if let Some(url) = self.critical_css_dev_url {
                // it's super ugly hack, but just for dev
                return templates::Html(Box::leak(format!("</style><link rel=stylesheet href='{}'><style>", url).into_boxed_str()));
            }
        }
        let data = self.critical_css_data.unwrap_or(include_str!("../../style/public/critical.css"));
        templates::Html(data)
    }

    pub fn local_css_data(&self) -> Option<templates::Html<&'static str>> {
        self.local_css_data.map(templates::Html)
    }
}

/// See `cat_page.rs.html`
pub async fn render_category(out: &mut impl Write, cat: &Category, crates: &KitchenSink, renderer: &Renderer) -> Result<(), failure::Error> {
    let urler = Urler::new(None);
    let page = cat_page::CatPage::new(cat, crates, renderer).await.context("can't prepare rendering of category page")?;
    templates::cat_page(out, &page, &urler)?;
    Ok(())
}

/// See `homepage.rs.html`
pub async fn render_homepage<W>(out: &mut W, crates: &KitchenSink) -> Result<(), failure::Error> where W: ?Sized, for<'a> &'a mut W: Write {
    let urler = Urler::new(None);
    let home = home_page::HomePage::new(crates).await?;
    let all = home.all_categories().await;
    templates::homepage(out, &home, &all, &urler)?;
    Ok(())
}

/// See `atom.rs.html`
pub async fn render_feed(out: &mut impl Write, crates: &KitchenSink) -> Result<(), failure::Error> {
    let urler = Urler::new(None);
    templates::atom(out, &home_page::HomePage::new(crates).await?, &urler)?;
    Ok(())
}

pub async fn render_sitemap(sitemap: &mut impl Write, crates: &KitchenSink) -> Result<(), failure::Error> {
    let all_crates = crates.sitemap_crates().await?;
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

/// See `author.rs.html`
pub async fn render_author_page<W: Write>(out: &mut W, aut: &RichAuthor, kitchen_sink: &KitchenSink, renderer: &Renderer) -> Result<(), failure::Error> {
    if stopped() {
        Err(KitchenSinkErr::Stopped)?;
    }

    let urler = Urler::new(None);
    let c = AuthorPage::new(aut, kitchen_sink, renderer).await.context("New crate page")?;
    templates::author(out, &urler, &c).context("author page io")?;
    Ok(())
}

/// See `crate_page.rs.html`
pub async fn render_crate_page<W: Write>(out: &mut W, all: &RichCrate, ver: &RichCrateVersion, kitchen_sink: &KitchenSink, renderer: &Renderer) -> Result<Option<DateTime<FixedOffset>>, failure::Error> {
    if stopped() {
        Err(KitchenSinkErr::Stopped)?;
    }

    let urler = Urler::new(Some(ver.origin().clone()));
    let c = CratePage::new(all, ver, kitchen_sink, renderer).await.context("New crate page")?;
    templates::crate_page(out, &urler, &c).context("crate page io")?;
    Ok(c.date_created())
}

/// See `reverse_dependencies.rs.html`
pub async fn render_crate_reverse_dependencies<W: Write>(out: &mut W, ver: &RichCrateVersion, kitchen_sink: &KitchenSink, renderer: &Renderer) -> Result<(), failure::Error> {
    if stopped() {
        Err(KitchenSinkErr::Stopped)?;
    }
    let urler = Urler::new(None);
    let c = reverse_dependencies::CratePageRevDeps::new(ver, kitchen_sink, renderer).await?;
    templates::reverse_dependencies(out, &urler, &c)?;
    Ok(())
}

/// See `install.rs.html`
pub async fn render_install_page(out: &mut impl Write, ver: &RichCrateVersion, kitchen_sink: &KitchenSink, renderer: &Renderer) -> Result<(), failure::Error> {
    if stopped() {
        Err(KitchenSinkErr::Stopped)?;
    }
    let urler = Urler::new(None); // Don't set self-crate, because we want to link back to crate page
    let c = crate::install_page::InstallPage::new(ver, kitchen_sink, renderer).await;
    templates::install(out, &urler, &c).context("install page io")?;
    Ok(())
}

pub struct CompatRange {
    oldest_ok: SemVer,
    newest_bad: SemVer,
}

pub async fn render_trending_crates(out: &mut impl Write, kitchen_sink: &KitchenSink, renderer: &Renderer) -> Result<(), failure::Error> {
    let (top, upd) = futures::join!(kitchen_sink.trending_crates(55), Box::pin(kitchen_sink.notable_recently_updated_crates(70)));
    let upd = upd?;

    let mut seen = HashSet::new();
    let mut tmp1 = Vec::with_capacity(upd.len());
    for (k, _) in upd.iter() {
        if seen.insert(k) {
            let f1 = kitchen_sink.rich_crate_version_async(k);
            let f2 = kitchen_sink.rich_crate_async(k);
            tmp1.push(async move { futures::try_join!(f1, f2) });
        }
    }
    tmp1.truncate(40);

    let mut tmp2 = Vec::with_capacity(top.len());
    for (k, _) in top.iter() {
        if seen.insert(k) {
            let f1 = kitchen_sink.rich_crate_version_async(k);
            let f2 = kitchen_sink.rich_crate_async(k);
            tmp2.push(async move { futures::try_join!(f1, f2) });
        }
    }
    tmp2.truncate(40);

    let (mut updated, trending) = futures::try_join!(try_join_all(tmp1), try_join_all(tmp2))?;

    // updated were sorted by rank…
    updated.sort_by_cached_key(|(_, all)| {
        std::cmp::Reverse(all.versions().iter().map(|v| &v.created_at).max().map(|v| v.to_string()))
    });

    let urler = Urler::new(None);
    templates::trending(out, &Page {
        title: "New and trending crates".to_owned(),
        description: Some("Rust packages that have been recently published or gained popularity. See what's new.".to_owned()),
        noindex: false,
        search_meta: true,
        critical_css_data: Some(include_str!("../../style/public/home.css")),
        critical_css_dev_url: Some("/home.css"),
        ..Default::default()
    }, &trending, &updated, &urler, renderer)?;
    Ok(())
}

pub fn render_debug_page(out: &mut impl Write, ver: &RichCrateVersion, kitchen_sink: &KitchenSink) -> Result<(), failure::Error> {
    let mut by_crate_ver = BTreeMap::new();
    let mut rustc_versions = BTreeSet::new();

    let compat = kitchen_sink.rustc_compatibility(ver.origin())?;

    for c in &compat {
        rustc_versions.insert(c.rustc_version.clone());

        let t = by_crate_ver.entry(&c.crate_version).or_insert_with(|| CompatRange {
            oldest_ok: "999.999.999".parse().unwrap(),
            newest_bad: "0.0.0".parse().unwrap(),
        });
        match c.compat {
            Compat::VerifiedWorks | Compat::ProbablyWorks => {
                if t.oldest_ok > c.rustc_version {
                    t.oldest_ok = c.rustc_version.clone();
                }
            },
            Compat::Incompatible | Compat::BrokenDeps => {
                if t.newest_bad < c.rustc_version {
                    t.newest_bad = c.rustc_version.clone();
                }
            },
        }
    }

    let rustc_versions = rustc_versions.into_iter().rev().collect::<Vec<_>>();
    templates::debug(out, (rustc_versions, by_crate_ver))?;
    Ok(())
}

pub fn render_error(out: &mut impl Write, err: &failure::Error) {
    templates::error_page(out, err).expect("error rendering error page");
}

/// See `crate_page.rs.html`
pub fn render_static_page(out: &mut impl Write, title: String, page: &Markup, renderer: &Renderer) -> Result<(), failure::Error> {
    if stopped() {
        Err(KitchenSinkErr::Stopped)?;
    }

    let (html, warnings) = renderer.page(page, Some(("https://lib.rs", "https://lib.rs")), Links::Trusted, None);
    if !warnings.is_empty() {
        eprintln!("static: {:?}", warnings);
    }

    templates::static_page(out, &Page {
        title,
        local_css_data: Some("main li {margin-top:0.25em;margin-bottom:0.25em}"),
        noindex: false,
        search_meta: true,
        ..Default::default()
    }, templates::Html(html))?;
    Ok(())
}

pub fn limit_text_len<'t>(text: &'t str, mut len_min: usize, mut len_max: usize) -> Cow<'t, str> {
    assert!(len_min <= len_max);
    if text.len() <= len_max {
        return text.into();
    }
    while !text.is_char_boundary(len_max) {
        len_max += 1;
    }
    while !text.is_char_boundary(len_min) {
        len_min += 1;
    }
    let mut cut = &text[..len_max];
    let optional = &cut[len_min..];
    if let Some(pos) = optional.find(&['.', ',', '!', '\n', '?', ')', ']'][..]).or_else(|| optional.find(' ')) {
        cut = cut[..=len_min + pos].trim_end_matches(&['.', ',', '!', '\n', '?', ' '][..]);
    };
    return format!("{}…", cut).into();
}

#[test]
fn limit_text_len_test() {
    assert_eq!("hello world", limit_text_len("hello world", 100, 200));
    assert_eq!("hel…", limit_text_len("hello world", 1, 3));
    assert_eq!("こん…", limit_text_len("こんにちは、世界", 3, 5));
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

/// Used to render descriptions
pub(crate) fn render_markdown_str(s: &str, markup: &Renderer) -> templates::Html<String> {
    templates::Html(markup.markdown_str(s, false, None))
}
