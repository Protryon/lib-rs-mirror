use crate::templates;
use crate::Page;
use categories::Category;
use categories::CATEGORIES;
use failure::Error;
use kitchen_sink::KitchenSink;
use rayon::prelude::*;
use render_readme::Renderer;
use rich_crate::RichCrateVersion;
use std::collections::HashSet;

/// Data for category page template
pub struct CatPage<'a> {
    pub cat: &'a Category,
    pub keywords: Vec<String>,
    pub crates: Vec<(RichCrateVersion, u32)>,
    pub related: Vec<String>,
    pub markup: &'a Renderer,
    pub count: usize,
}

impl<'a> CatPage<'a> {
    pub async fn new(cat: &'a Category, crates: &'a KitchenSink, markup: &'a Renderer) -> Result<CatPage<'a>, Error> {
        Ok(Self {
            count: crates.category_crate_count(&cat.slug)? as usize,
            keywords: crates.top_keywords_in_category(cat)?,
            related: crates.related_categories(&cat.slug)?,
            crates: crates
                .top_crates_in_category(&cat.slug).await?
                .par_iter()
                .with_max_len(1)
                .filter_map(|o| {
                    let c = match crates.rich_crate_version(&o) {
                        Ok(c) => c,
                        Err(e) => {
                            eprintln!("Skipping {:?} because {}", o, e);
                            return None;
                        },
                    };
                    if c.is_yanked() {
                        return None;
                    }
                    let d = match crates.downloads_per_month_or_equivalent(&o) {
                        Ok(d) => d.unwrap_or(0) as u32,
                        Err(e) => {
                            eprintln!("Skipping {:?} because dl {}", o, e);
                            return None;
                        },
                    };
                    Some((c, d))
                })
                .collect::<Vec<_>>(),
            cat,
            markup,
        })
    }

    pub fn has_subcategories_and_siblings(&self) -> bool {
        !self.cat.sub.is_empty() || !self.cat.siblings.is_empty()
    }

    pub fn subcategories_and_siblings(&self) -> impl Iterator<Item = &Category> {
        self.cat.sub.values().chain(self.cat.siblings.iter().flat_map(|slug| CATEGORIES.from_slug(slug)))
    }

    /// Used to render descriptions
    pub fn render_markdown_str(&self, s: &str) -> templates::Html<String> {
        templates::Html(self.markup.markdown_str(s, false, None))
    }

    /// For color of the version
    ///
    /// It tries to guess which versions seem "unstable".
    ///
    /// TODO: Merge with the better version history analysis from the individual crate page.
    pub fn version_class(&self, c: &RichCrateVersion) -> &str {
        let v = c.version_semver().unwrap();
        match (v.major, v.minor, v.patch, v.is_prerelease()) {
            (1..=15, _, _, false) => "stable",
            (0, m, p, false) if m >= 2 && p >= 3 => "stable",
            (m, ..) if m >= 1 => "okay",
            (0, 1, p, _) if p >= 10 => "okay",
            (0, 3..=10, p, _) if p > 0 => "okay",
            _ => "unstable",
        }
    }

    pub fn description(&self) -> &str {
        self.cat.description.trim_end_matches('.')
    }

    /// "See also" feature
    pub fn related_categories(&self) -> Vec<Vec<&Category>> {
        let mut seen = HashSet::with_capacity(self.related.len());
        self.related.iter().map(|slug| CATEGORIES.from_slug(slug).filter(|c| seen.insert(&c.slug)).collect()).filter(|v: &Vec<_>| !v.is_empty()).collect()
    }

    /// Nicely rounded number of downloads
    ///
    /// To show that these numbers are just approximate.
    pub fn downloads(&self, num: u32) -> (String, &str) {
        match num {
            a @ 0..=99 => (format!("{}", a), ""),
            a @ 0..=500 => (format!("{}", a / 10 * 10), ""),
            a @ 0..=999 => (format!("{}", a / 50 * 50), ""),
            a @ 0..=9999 => (format!("{}.{}", a / 1000, a % 1000 / 100), "K"),
            a @ 0..=999_999 => (format!("{}", a / 1000), "K"),
            a => (format!("{}.{}", a / 1_000_000, a % 1_000_000 / 100_000), "M"),
        }
    }

    /// Metadata about the category
    pub fn page(&self) -> Page {
        Page {
            title: format!("{} — list of Rust libraries/crates", self.cat.standalone_name()),
            description: Some(self.cat.description.clone()),
            item_name: Some(self.cat.name.clone()),
            item_description: Some(self.cat.short_description.clone()),
            keywords: Some(self.keywords.join(", ")),
            noindex: false,
            search_meta: true,
            ..Default::default()
        }
    }

    /// For breadcrumbs
    pub fn parent_categories(&self) -> Vec<&Category> {
        let mut c: Vec<_> = CATEGORIES.from_slug(&self.cat.slug).collect();
        c.pop();
        c
    }
}
