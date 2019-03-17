#![allow(unused_imports)]
use crate::Page;
use categories::Category;
use categories::CategoryMap;
use categories::CATEGORIES;
use failure;
use kitchen_sink::stopped;
use kitchen_sink::{CrateData, KitchenSink};
use rayon::prelude::*;
use rich_crate::Origin;
use rich_crate::RichCrateVersion;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;

/// The list on the homepage looks flat, but it's actually a tree.
///
/// Each category contains list of top/most relevant crates in it.
pub struct HomeCategory {
    pub pop: usize,
    pub cat: &'static Category,
    pub sub: Vec<HomeCategory>,
    pub top: Vec<RichCrateVersion>,
    dl: usize,
}

/// Computes data used on the home page on https://crates.rs/
pub struct HomePage<'a> {
    crates: &'a KitchenSink,
}

impl<'a> HomePage<'a> {
    pub fn new(crates: &'a KitchenSink) -> Result<Self, failure::Error> {
        Ok(Self { crates })
    }

    /// List of all categories, sorted, with their most popular and newest crates.
    pub fn all_categories(&self) -> Vec<HomeCategory> {
        let seen = &mut HashSet::with_capacity(5000);
        let mut all = self.make_all_categories(&CATEGORIES.root, seen);
        self.add_updated_to_all_categories(&mut all, seen);
        all
    }

    /// Add most recently updated crates to the list of top crates in each category
    fn add_updated_to_all_categories(&self, cats: &mut [HomeCategory], seen: &mut HashSet<Origin>) {
        // it's not the same order as before, but that's fine, it adds more variety
        for cat in cats {
            // depth first
            self.add_updated_to_all_categories(&mut cat.sub, seen);

            let new: Vec<_> =
                self.crates.recently_updated_crates_in_category(&cat.cat.slug).unwrap().into_iter().filter(|c| seen.get(&c).is_none()).take(3).collect();
            let new: Vec<_> = new.into_par_iter().with_max_len(1).filter_map(|c| self.crates.rich_crate_version(&c, CrateData::Full).ok()).collect();
            for c in &new {
                seen.insert(c.origin().to_owned());
            }
            cat.top.extend(new);
        }
    }

    /// A crate can be in multiple categories, so `seen` ensures every crate is shown only once
    /// across all categories.
    fn make_all_categories(&self, root: &'static CategoryMap, seen: &mut HashSet<Origin>) -> Vec<HomeCategory> {
        if root.is_empty() {
            return Vec::new();
        }
        let mut c: Vec<_> = root
            .iter()
            .take_while(|_| !stopped())
            .map(|(_, cat)| {
                // depth first - important!
                let sub = self.make_all_categories(&cat.sub, seen);
                let own_pop = self.crates.category_crate_count(&cat.slug).unwrap_or(0) as usize;

                HomeCategory {
                    // make container as popular as its best child (already sorted), because homepage sorts by top-level only
                    pop: sub.get(0).map(|c| c.pop).unwrap_or(0).max(own_pop),
                    dl: sub.get(0).map(|c| c.dl).unwrap_or(0),
                    top: Vec::with_capacity(5),
                    sub,
                    cat,
                }
            })
            .collect();
        c.sort_by(|a, b| b.pop.cmp(&a.pop));

        // mark seen from least popular (assuming they're more specific)
        for cat in c.iter_mut().rev() {
            let mut dl = 0;
            let top: Vec<_> = self
                .crates
                .top_crates_in_category(&cat.cat.slug)
                .unwrap()
                .iter()
                .take(35)
                .filter(|(c, _)| seen.get(c).is_none())
                .take(7)
                .cloned()
                .map(|(c, d)| {
                    dl += d as usize;
                    c
                })
                .collect();
            cat.top.par_extend(top.into_par_iter().with_max_len(1).filter_map(|c| self.crates.rich_crate_version(&c, CrateData::Full).ok()));
            for c in &cat.top {
                seen.insert(c.origin().to_owned());
            }
            cat.dl = dl.max(cat.dl);
        }

        let mut ranked = c.into_iter().map(|c| (c.cat.slug.as_str(), (c.dl * c.pop, c))).collect::<HashMap<_,_>>();

        // this is artificially inflated by popularity of syn/quote in serde
        if let Some(pmh) = ranked.get_mut("development-tools::procedural-macro-helpers") {
            pmh.0 /= 32;
        }

        // move cryptocurrencies out of cryptography for the homepage, so that cryptocurrencies are sorted by their own popularity
        if let Some(cryptocurrencies) = ranked.get_mut("cryptography").and_then(|(_,c)| c.sub.pop()) {
            ranked.insert(cryptocurrencies.cat.slug.as_str(), (cryptocurrencies.dl * cryptocurrencies.pop, cryptocurrencies));
        }

        // these categories are easily confusable, so keep them together
        Self::avg_pair(&mut ranked, "hardware-support", "embedded");
        Self::avg_pair(&mut ranked, "parser-implementations", "parsing");
        Self::avg_pair(&mut ranked, "games", "game-engines");
        Self::avg_pair(&mut ranked, "web-programming", "wasm");
        Self::avg_pair(&mut ranked, "asynchronous", "concurrency");
        Self::avg_pair(&mut ranked, "rendering", "multimedia");
        Self::avg_pair(&mut ranked, "emulators", "simulation");
        Self::avg_pair(&mut ranked, "value-formatting", "template-engine");
        Self::avg_pair(&mut ranked, "database-implementations", "database");
        Self::avg_pair(&mut ranked, "command-line-interface", "command-line-utilities");

        let mut c = ranked.into_iter().map(|(_,v)| v).collect::<Vec<_>>();
        c.sort_by(|a, b| b.0.cmp(&a.0));

        c.into_iter().map(|(_,c)| c).collect()
    }

    fn avg_pair(ranked: &mut HashMap<&str, (usize, HomeCategory)>, a: &str, b: &str) {
        if let Some(&(a_rank, _)) = ranked.get(a) {
            let b_rank = ranked.get(b).expect("sibling category").0;
            ranked.get_mut(a).unwrap().0 = (a_rank * 17 + b_rank * 15) / 32;
            ranked.get_mut(b).unwrap().0 = (a_rank * 15 + b_rank * 17) / 32;
            println!("averaged {} {} {} {}", a, a_rank, b, b_rank);
        }
    }

    pub fn page(&self) -> Page {
        Page {
            title: "Crates.rs — home for Rust crates".to_owned(),
            description: Some("List of Rust libraries and applications. An unofficial experimental opinionated alternative to crates.io".to_owned()),
            item_name: None,
            item_description: None,
            keywords: None,
            created: None,
            alternate: None,
            canonical: None,
            noindex: false,
            search_meta: true,
            critical_css_data: Some(include_str!("../../style/public/home.css")),
        }
    }
}
