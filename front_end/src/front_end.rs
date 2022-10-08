//! This crate generates HTML templates for https://lib.rs
//!
//! Most template require their own type of struct that does
//! some lightweight conversion from data model/APIs,
//! because the template engine Ructe doesn't support
//! complex expressions in the templates.

use rich_crate::Origin;
use ahash::HashSetExt;
mod all_versions;
mod author_page;
mod cat_page;
mod crate_page;
mod crev;
mod download_graph;
mod home_page;
mod install_page;
mod iter;
mod maintainer_dashboard;
mod not_found_page;
mod reverse_dependencies;
mod search_page;
mod urler;
mod global_stats;
pub use crate::not_found_page::*;
pub use crate::search_page::*;
pub use crate::global_stats::*;
use futures::future::try_join_all;
use kitchen_sink::CrateOwnerRow;
use kitchen_sink::CrateOwners;
use maintainer_dashboard::MaintainerDashboard;
use crate::author_page::*;
use crate::crate_page::*;
use crate::urler::Urler;
use categories::Category;
use chrono::prelude::*;

use anyhow::Context;
use kitchen_sink::KitchenSink;
use kitchen_sink::Review;
use kitchen_sink::RichAuthor;
use kitchen_sink::{stopped, KitchenSinkErr};
use render_readme::Links;
use render_readme::Markup;
use render_readme::Renderer;
use rich_crate::RichCrate;
use rich_crate::RichCrateVersion;
use std::borrow::Cow;
use ahash::HashSet;
use std::io::Write;
use url::Url;

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
    #[allow(dead_code)]
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
pub async fn render_category(out: &mut impl Write, cat: &Category, crates: &KitchenSink, renderer: &Renderer) -> Result<(), anyhow::Error> {
    let urler = Urler::new(None);
    let page = cat_page::CatPage::new(cat, crates, renderer).await?;
    templates::cat_page(out, &page, &urler)?;
    Ok(())
}

/// See `homepage.rs.html`
pub async fn render_homepage<W>(out: &mut W, crates: &KitchenSink) -> Result<(), anyhow::Error> where W: ?Sized, for<'a> &'a mut W: Write {
    let urler = Urler::new(None);
    let home = home_page::HomePage::new(crates).await?;
    let all = home.all_categories().await;
    templates::homepage(out, &home, &all, &urler)?;
    Ok(())
}

/// See `atom.rs.html`
pub async fn render_feed(out: &mut impl Write, crates: &KitchenSink) -> Result<(), anyhow::Error> {
    let urler = Urler::new(None);
    templates::atom(out, &home_page::HomePage::new(crates).await?, &urler)?;
    Ok(())
}

pub async fn render_sitemap(sitemap: &mut impl Write, crates: &KitchenSink) -> Result<(), anyhow::Error> {
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
                _ => "weekly",
            },
        )?;
    }

    sitemap.write_all(b"\n</urlset>\n")?;
    Ok(())
}

/// See `author.rs.html`
pub async fn render_author_page<W: Write>(out: &mut W, rows: Vec<CrateOwnerRow>, aut: &RichAuthor, kitchen_sink: &KitchenSink, renderer: &Renderer) -> Result<(), anyhow::Error> {
    if stopped() {
        return Err(KitchenSinkErr::Stopped.into());
    }

    let urler = Urler::new(None);
    let c = AuthorPage::new(aut, rows, kitchen_sink, renderer).await.context("Can't load data for author page")?;
    templates::author(out, &urler, &c).context("author page io")?;
    Ok(())
}

/// See `maintainer_dashboard.rs.html`
/// See `maintainer_dashboard_atom.rs.html`
pub async fn render_maintainer_dashboard<W: Write>(out: &mut W, atom_feed: bool, rows: Vec<CrateOwnerRow>, aut: &RichAuthor, kitchen_sink: &KitchenSink, renderer: &Renderer) -> Result<(), anyhow::Error> {
    if stopped() {
        return Err(KitchenSinkErr::Stopped.into());
    }

    let urler = Urler::new(None);
    let c = MaintainerDashboard::new(aut, rows, kitchen_sink, &urler, renderer).await.context("Can't load data for the dashboard")?;
    if !atom_feed {
        templates::maintainer_dashboard(out, &urler, &c)
    } else {
        templates::maintainer_dashboard_atom(out, &urler, &c)
    }.context("maintainer dashboard io")?;
    Ok(())
}

/// See `crate_page.rs.html`
pub async fn render_crate_page<W: Write>(out: &mut W, all: &RichCrate, ver: &RichCrateVersion, kitchen_sink: &KitchenSink, renderer: &Renderer) -> Result<Option<DateTime<Utc>>, anyhow::Error> {
    if stopped() {
        return Err(KitchenSinkErr::Stopped.into());
    }

    let urler = Urler::new(Some(ver.origin().clone()));
    let c = CratePage::new(all, ver, kitchen_sink, renderer).await.context("Can't load data for crate page")?;
    templates::crate_page(out, &urler, &c).context("crate page io")?;
    Ok(c.date_created())
}

/// See `reverse_dependencies.rs.html`
pub async fn render_crate_reverse_dependencies<W: Write>(out: &mut W, ver: &RichCrateVersion, kitchen_sink: &KitchenSink, renderer: &Renderer) -> Result<(), anyhow::Error> {
    if stopped() {
        return Err(KitchenSinkErr::Stopped.into());
    }
    let urler = Urler::new(None);
    let c = reverse_dependencies::CratePageRevDeps::new(ver, kitchen_sink, renderer).await?;
    templates::reverse_dependencies(out, &urler, &c)?;
    Ok(())
}

/// See `install.rs.html`
pub async fn render_install_page(out: &mut impl Write, ver: &RichCrateVersion, kitchen_sink: &KitchenSink, renderer: &Renderer) -> Result<(), anyhow::Error> {
    if stopped() {
        return Err(KitchenSinkErr::Stopped.into());
    }
    let urler = Urler::new(None); // Don't set self-crate, because we want to link back to crate page
    let c = crate::install_page::InstallPage::new(ver, kitchen_sink, renderer).await;
    templates::install(out, &urler, &c).context("install page io")?;
    Ok(())
}

/// See `all_versions.rs.html`
pub async fn render_all_versions_page(out: &mut impl Write, all: RichCrate, ver: &RichCrateVersion, kitchen_sink: &KitchenSink) -> Result<(), anyhow::Error> {
    if stopped() {
        return Err(KitchenSinkErr::Stopped.into());
    }
    let urler = Urler::new(None); // Don't set self-crate, because we want to link back to crate page
    let c = crate::all_versions::AllVersions::new(all, ver, kitchen_sink, &urler).await?;
    templates::all_versions(out, &urler, &c).context("all_versions page io")?;
    Ok(())
}

/// See `crev.rs.html`
pub async fn render_crate_reviews(out: &mut impl Write, reviews: &[Review], ver: &RichCrateVersion, kitchen_sink: &KitchenSink, renderer: &Renderer) -> Result<(), anyhow::Error> {
    if stopped() {
        return Err(KitchenSinkErr::Stopped.into());
    }
    let urler = Urler::new(None); // Don't set self-crate, because we want to link back to crate page
    let c = crate::crev::ReviewsPage::new(reviews, ver, kitchen_sink, renderer).await;
    templates::crev(out, &urler, &c).context("crev page io")?;
    Ok(())
}

pub async fn render_trending_crates(out: &mut impl Write, kitchen_sink: &KitchenSink, renderer: &Renderer) -> Result<(), anyhow::Error> {
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

pub async fn render_debug_page(out: &mut impl Write, kitchen_sink: &KitchenSink, origin: &Origin) -> Result<(), anyhow::Error> {

    let t = kitchen_sink.traction_stats(origin).await?;
    let r = kitchen_sink.crate_ranking_for_builder(origin).await?;
    let dl = kitchen_sink.downloads_per_month_or_equivalent(origin).await?.unwrap_or(0);
    let dep = kitchen_sink.crates_io_dependents_stats_of(origin).await?;
    let owners = kitchen_sink.crate_owners(origin, CrateOwners::All).await?;

    writeln!(out, "<pre>{t:#?}
rank: {r}
dl: {dl}
{dep:#?}
").unwrap();
    for o in owners {
        writeln!(out, "{o:?}").unwrap();
    }
    write!(out, "</pre>").unwrap();
    Ok(())
}

pub async fn render_compat_page(out: &mut impl Write, all: RichCrate, kitchen_sink: &KitchenSink) -> Result<(), anyhow::Error> {
    let mut rustc_versions = HashSet::new();
    rustc_versions.insert(60);
    rustc_versions.insert(55);
    rustc_versions.insert(50);
    rustc_versions.insert(45);
    rustc_versions.insert(40);
    rustc_versions.insert(35);
    rustc_versions.insert(30);

    let by_crate_ver = kitchen_sink.rustc_compatibility(&all).await?;

    for c in by_crate_ver.values() {
        for v in c.all_rustc_versions() { rustc_versions.insert(v); }
    }

    let mut rustc_versions = rustc_versions.into_iter().collect::<Vec<u16>>();
    rustc_versions.sort_unstable();
    templates::compat(out, (rustc_versions, by_crate_ver))?;
    Ok(())
}

pub fn render_error(out: &mut impl Write, err: &anyhow::Error) {
    templates::error_page(out, err).expect("error rendering error page");
}

/// See `crate_page.rs.html`
pub fn render_static_page(out: &mut impl Write, title: String, page: &Markup, renderer: &Renderer) -> Result<(), anyhow::Error> {
    if stopped() {
        return Err(KitchenSinkErr::Stopped.into());
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

/// See `crate_page.rs.html`
pub fn render_static_trusted_html(out: &mut impl Write, title: String, html: String) -> Result<(), anyhow::Error> {
    if stopped() {
        return Err(KitchenSinkErr::Stopped.into());
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

pub fn limit_text_len(text: &str, mut len_min: usize, mut len_max: usize) -> Cow<'_, str> {
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
    format!("{}…", cut).into()
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

pub(crate) fn date_now() -> impl std::fmt::Display {
    Utc::now().format("%Y-%m-%d")
}

/// Used to render descriptions
pub(crate) fn render_maybe_markdown_str(s: &str, markup: &Renderer, allow_links: bool, own_crate_name: Option<&str>) -> templates::Html<String> {
    let looks_like_markdown = s.bytes().any(|b| b == b'`');
    templates::Html(if looks_like_markdown {
        markup.markdown_str(s, allow_links, own_crate_name)
    } else {
        use templates::ToHtml;
        let mut buf = Vec::with_capacity(s.len() + 16);
        s.to_html(&mut buf).unwrap();
        String::from_utf8(buf).unwrap()
    })
}

/// Nicely rounded number of downloads
///
/// To show that these numbers are just approximate.
pub(crate) fn format_downloads(num: u32) -> (String, &'static str) {
    match num {
        a @ 0..=99 => (format!("{}", a), ""),
        a @ 0..=500 => (format!("{}", a / 10 * 10), ""),
        a @ 0..=999 => (format!("{}", a / 50 * 50), ""),
        a @ 0..=9999 => (format!("{}.{}", a / 1000, a % 1000 / 100), "K"),
        a @ 0..=999_999 => (format!("{}", a / 1000), "K"),
        a => (format!("{}.{}", a / 1_000_000, a % 1_000_000 / 100_000), "M"),
    }
}

pub(crate) fn format_downloads_verbose(num: u32) -> (String, &'static str) {
    match num {
        a @ 0..=99 => (format!("{}", a), ""),
        a @ 0..=999_999 => (format!("{}", a / 1000), "thousand"),
        a => (format!("{}.{}", a / 1_000_000, a % 1_000_000 / 100_000), "million"),
    }
}

pub(crate) fn url_domain(url: &str) -> Option<Cow<'static, str>> {
    Url::parse(url).ok().and_then(|url| {
        url.host_str().and_then(|host| {
            if host.ends_with(".github.io") {
                Some("github.io".into())
            } else if host.ends_with(".githubusercontent.com") {
                None
            } else {
                Some(host.trim_start_matches("www.").to_string().into())
            }
        })
    })
}
