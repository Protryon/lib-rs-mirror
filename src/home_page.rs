#![allow(unused_imports)]
use kitchen_sink::stopped;
use std::path::PathBuf;
use categories::Category;
use categories::CategoryMap;
use std::collections::HashMap;
use std::collections::HashSet;
use rich_crate::Origin;
use rich_crate::RichCrateVersion;
use categories::CATEGORIES;
use kitchen_sink::{KitchenSink, CrateData};
use failure;
use Page;
use rayon::prelude::*;

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
        Ok(Self {
            crates,
        })
    }

    /// List of all categories, sorted, with their most popular and newest crates.
    pub fn all_categories(&self) -> Vec<HomeCategory> {
        let seen =  &mut HashSet::with_capacity(5000);
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

            let new: Vec<_> = self.crates.recently_updated_crates_in_category(&cat.cat.slug).unwrap()
                .into_iter()
                .filter(|c| {
                    seen.get(&c).is_none()
                })
                .take(3)
                .collect();
            let new: Vec<_> = new.into_par_iter()
                .filter_map(|c| {
                    self.crates.rich_crate_version(&c, CrateData::Full).ok()
                })
                .collect();
            for c in &new {
                seen.insert(c.origin().to_owned());
            }
            cat.top.extend(new);
        }
    }

    /// A crate can be in multiple categories, so `seen` ensures every crate is shown only once
    /// across all categories.
    fn make_all_categories(&self, root: &'static CategoryMap, seen: &mut HashSet<Origin>) -> Vec<HomeCategory> {
        let mut c: Vec<_> = root.iter().take_while(|_| !stopped()).map(|(_, cat)| {
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
        }).collect();
        c.sort_by(|a,b| b.pop.cmp(&a.pop));

        // mark seen from least popular (assuming they're more specific)
        for cat in c.iter_mut().rev() {
            let mut dl = 0;
            let top: Vec<_> = self.crates.top_crates_in_category(&cat.cat.slug).unwrap()
                .iter()
                .take(35)
                .filter(|(c,_)| {
                    seen.get(c).is_none()
                })
                .take(7)
                .cloned()
                .map(|(c, d)| {
                    dl += d as usize;
                    c
                })
                .collect();
            cat.top.par_extend(top.into_par_iter()
                .filter_map(|c| {
                    self.crates.rich_crate_version(&c, CrateData::Full).ok()
                }));
            for c in &cat.top {
                seen.insert(c.origin().to_owned());
            }
            cat.dl = dl.max(cat.dl);
        }

        c.sort_by(|a,b| (b.dl * b.pop).cmp(&(a.dl * a.pop)));
        c
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
            alt_critical_css: Some("../style/public/home.css".to_owned()),
        }
    }
}
