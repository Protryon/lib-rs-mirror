#![allow(unused_imports)]
use chrono::DateTime;
use crate::Utc;
use crate::Page;
use categories::Category;
use categories::CategoryMap;
use categories::CATEGORIES;

use futures::prelude::*;
use kitchen_sink::stopped;
use kitchen_sink::ArcRichCrateVersion;
use kitchen_sink::CrateAuthor;
use kitchen_sink::KitchenSink;
use locale::Numeric;
use rich_crate::Origin;
use rich_crate::RichCrate;
use rich_crate::RichCrateVersion;
use ahash::HashMap;
use ahash::HashSet;
use ahash::HashMapExt;
use ahash::HashSetExt;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use tokio::time::timeout_at;
use tokio::time::Instant;

/// Editorialize the category list a little
const CATEGORY_RANK_ADJUST: [(&str, f64); 29] = [
    ("asynchronous", 1.1),
    ("command-line-utilities", 1.4), // app downloads are naturally smaller
    ("command-line-interface", 1.1),
    ("concurrency", 1.2),
    ("cryptocurrencies", 0.1),
    ("cryptography::cryptocurrencies", 0.1),
    ("database", 1.25),
    ("database-implementations", 1.2),
    ("date-and-time", 0.9),
    ("development-tools::cargo-plugins", 1.3), // app downloads are naturally smaller
    ("development-tools::procedural-macro-helpers", 0.1),
    ("development-tools::profiling", 0.9),
    ("embedded", 1.1),
    ("game-development", 1.1),
    ("gui", 1.1),
    ("memory-management", 1.1),
    ("multimedia", 1.15),
    ("no-std", 0.8),
    ("science", 0.9),
    ("science::math", 0.9),
    ("science::robotics", 0.5),
    ("simulation", 1.1),
    ("template-engine", 0.8),
    ("text-editors", 0.8),
    ("text-processing", 0.85),
    ("value-formatting", 0.7),
    ("wasm", 0.9),
    ("web-programming", 1.1),
    ("web-programming::websocket", 0.2),
];

/// The list on the homepage looks flat, but it's actually a tree.
///
/// Each category contains list of top/most relevant crates in it.
pub struct HomeCategory {
    pub pop: usize,
    pub cat: &'static Category,
    pub sub: Vec<HomeCategory>,
    pub top: Vec<ArcRichCrateVersion>,
    dl: usize,
}

/// Computes data used on the home page on https://lib.rs/
pub struct HomePage<'a> {
    crates: &'a KitchenSink,
    handle: tokio::runtime::Handle,
}

impl<'a> HomePage<'a> {
    pub async fn new(crates: &'a KitchenSink) -> Result<HomePage<'a>, anyhow::Error> {
        Ok(Self {
            crates,
            handle: tokio::runtime::Handle::current(),
        })
    }

    pub fn total_crates(&self) -> String {
        Numeric::english().format_int(self.crates.all_crates().count())
    }

    /// List of all categories, sorted, with their most popular and newest crates.
    pub async fn all_categories(&self) -> Vec<HomeCategory> {
        let seen = &mut HashSet::with_capacity(5000);
        let deadline = Instant::now() + Duration::from_secs(48);
        let mut all = self.make_all_categories(&CATEGORIES.root, seen, deadline).await;
        self.add_updated_to_all_categories(&mut all, seen).await;
        all
    }

    /// Add most recently updated crates to the list of top crates in each category
    fn add_updated_to_all_categories<'z, 's: 'z>(&'s self, cats: &'z mut [HomeCategory], seen: &'z mut HashSet<Origin>) -> std::pin::Pin<Box<dyn 'z + Send + Future<Output=()>>> {
        let deadline = Instant::now() + Duration::from_secs(5);
        Box::pin(async move {
        // it's not the same order as before, but that's fine, it adds more variety
        for cat in cats {
            // depth first
            let (_, recently_updated) = futures::join!(
                self.add_updated_to_all_categories(&mut cat.sub, seen),
                self.crates.recently_updated_crates_in_category(&cat.cat.slug)
            );

            // Aim for 140 characters of crate names, so that categories are visually similar
            let mut text_length = cat.top.iter().map(|c| c.short_name().len() as u32 + 1).sum::<u32>();
            for c in recently_updated.expect("recently_updated_crates_in_category") {
                if seen.insert(c.clone()) {
                    if let Ok(Ok(c)) = timeout_at(deadline, self.crates.rich_crate_version_async(&c)).await {
                        text_length += c.short_name().len() as u32 + 1;
                        cat.top.push(c);
                        if text_length >= 140 {
                            break;
                        }
                    }
                }
            }
        }})
    }

    /// A crate can be in multiple categories, so `seen` ensures every crate is shown only once
    /// across all categories.
    fn make_all_categories<'z, 's: 'z>(&'s self, root: &'static CategoryMap, seen: &'z mut HashSet<Origin>, deadline: Instant) -> std::pin::Pin<Box<dyn 'z + Send + Future<Output=Vec<HomeCategory>>>> {
        Box::pin(async move {
            if root.is_empty() {
                return Vec::new();
            }

            let mut c = Vec::new();
            for (_, cat) in root.iter() {
                if matches!(cat.slug.as_str(), "std" | "uncategorized" | "cryptography::cryptocurrencies") {
                    continue;
                }

                if stopped() { return Vec::new(); }
                // depth first - important!
                let (sub, own_pop) = futures::join!(
                        self.make_all_categories(&cat.sub, seen, deadline),
                        self.crates.category_crate_count(&cat.slug));
                let own_pop = own_pop.unwrap_or((0, 0.)).0 as usize; // FIXME: use despammed value?

                c.push(HomeCategory {
                    // make container as popular as its best child (already sorted), because homepage sorts by top-level only
                    pop: sub.get(0).map(|c| c.pop).unwrap_or(0).max(own_pop),
                    dl: sub.get(0).map(|c| c.dl).unwrap_or(0),
                    top: Vec::with_capacity(8),
                    sub,
                    cat,
                })
            }
            c.sort_unstable_by(|a, b| b.pop.cmp(&a.pop));

            let mut c = futures::future::join_all(c.into_iter().map(|cat| async move {
                let top = self.crates.top_crates_in_category(&cat.cat.slug).await;
                (cat, top)
            })).await;

            // mark seen from least popular (assuming they're more specific)
            for (cat, top) in c.iter_mut().rev() {
                let top: Vec<_> = top.as_ref().map(|v| v.as_slice())
                    .map_err(|e| eprintln!("top fail: {e}")).unwrap_or_default()
                    .iter()
                    .take(35)
                    .filter(|c| seen.get(c).is_none())
                    .take(8)
                    .cloned().collect();

                let dl = futures::stream::iter(top.clone())
                    .map(|c| async move {self.crates.downloads_per_month_or_equivalent(&c).await})
                    .buffer_unordered(8)
                    .fold(0, |sum, dl| async move {
                        sum + dl.ok().flatten().unwrap_or(0)
                    }).await;

                let top_resolved = futures::future::join_all(
                    top.into_iter()
                    .filter(|c| seen.insert(c.clone()))
                    .map(|c| async move {
                        timeout_at(deadline, self.crates.rich_crate_version_async(&c)).await?
                    })).await;

                for c in top_resolved {
                    match c {
                        Ok(c) => cat.top.push(c),
                        Err(e) => eprintln!("catstream: {e} (in {})", cat.cat.slug),
                    }
                }
                cat.dl = dl.max(cat.dl);
            }

            let mut ranked = c.into_iter().map(|(c, _)| (c.cat.slug.as_str(), (c.dl * c.pop, c))).collect::<HashMap<_, _>>();

            // this is artificially inflated by popularity of syn/quote in serde
            if let Some(pmh) = ranked.get_mut("development-tools::procedural-macro-helpers") {
                pmh.0 /= 32;
            }

            for &(slug, adjust) in CATEGORY_RANK_ADJUST.iter() {
                if let Some(c) = ranked.get_mut(slug) {
                    c.0 = (c.0 as f64 * adjust) as usize;
                }
            }

            // these categories are easily confusable, so keep them together
            Self::avg_pair(&mut ranked, "hardware-support", "embedded");
            Self::avg_pair(&mut ranked, "parser-implementations", "parsing");
            Self::avg_pair(&mut ranked, "games", "game-development");
            Self::avg_pair(&mut ranked, "web-programming", "wasm");
            Self::avg_pair(&mut ranked, "asynchronous", "concurrency");
            Self::avg_pair(&mut ranked, "rendering", "multimedia");
            Self::avg_pair(&mut ranked, "emulators", "simulation");
            Self::avg_pair(&mut ranked, "value-formatting", "template-engine");
            Self::avg_pair(&mut ranked, "database-implementations", "database");
            Self::avg_pair(&mut ranked, "command-line-interface", "command-line-utilities");

            let mut c = ranked.into_values().collect::<Vec<_>>();
            c.sort_unstable_by(|a, b| b.0.cmp(&a.0));

            c.into_iter().map(|(_, c)| c).collect()
        })
    }

    fn avg_pair(ranked: &mut HashMap<&str, (usize, HomeCategory)>, a: &str, b: &str) {
        if let Some(&(a_rank, _)) = ranked.get(a) {
            let b_rank = ranked.get(b).expect("sibling category").0;
            ranked.get_mut(a).expect("avg cat").0 = (a_rank * 17 + b_rank * 15) / 32;
            ranked.get_mut(b).expect("avg cat").0 = (a_rank * 15 + b_rank * 17) / 32;
        }
    }

    pub fn last_modified(&self, allver: &RichCrate) -> DateTime<Utc> {
        *allver.versions().iter().map(|a| &a.created_at).max().expect("no versions?")
    }

    /// for the feed
    pub fn now(&self) -> String {
        chrono::Utc::now().to_rfc3339()
    }

    pub fn all_contributors<'c>(&self, krate: &'c RichCrateVersion) -> Option<Vec<CrateAuthor<'c>>> {
        self.block(self.crates.all_contributors(krate))
            .map(|(mut a, mut o, ..)| {
                a.append(&mut o);
                a
            })
            .ok()
    }

    pub fn recently_updated_crates(&self) -> Vec<(RichCrate, ArcRichCrateVersion)> {
        self.block(async {
            futures::stream::iter(self.crates
                .notable_recently_updated_crates(30).await
                .expect("recent crates")
                .into_iter())
                .map(move |(o, _)| async move {
                    futures::try_join!(self.crates.rich_crate_async(&o), self.crates.rich_crate_version_async(&o)).ok()
                })
                .buffer_unordered(8)
                .filter_map(|x| async move {x})
                .collect::<Vec<_>>().await
            })
    }

    fn block<O>(&self, f: impl Future<Output = O>) -> O {
        let _g = self.handle.enter();
        futures::executor::block_on(f)
    }

    pub fn page(&self) -> Page {
        Page {
            title: "Lib.rs — home for Rust crates".to_owned(),
            description: Some("List of Rust libraries and applications. An unofficial experimental opinionated alternative to crates.io".to_owned()),
            alternate: Some("https://lib.rs/atom.xml".to_string()),
            alternate_type: Some("application/atom+xml"),
            canonical: Some("https://lib.rs".to_string()),
            noindex: false,
            search_meta: true,
            critical_css_data: Some(include_str!("../../style/public/home.css")),
            critical_css_dev_url: Some("/home.css"),
            ..Default::default()
        }
    }
}
