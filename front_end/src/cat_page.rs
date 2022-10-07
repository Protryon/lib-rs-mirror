use ahash::HashSetExt;
use kitchen_sink::MaintenanceStatus;
use crate::templates;
use crate::Page;
use categories::Category;
use categories::CATEGORIES;
use anyhow::Error;
use futures::stream::StreamExt;
use kitchen_sink::ArcRichCrateVersion;
use kitchen_sink::KitchenSink;
use render_readme::Renderer;
use rich_crate::RichCrateVersion;
use ahash::HashSet;
use std::time::Duration;
use tokio::time::timeout_at;
use tokio::time::Instant;

/// Data for category page template
pub struct CatPage<'a> {
    pub cat: &'a Category,
    pub keywords: Vec<String>,
    pub crates: Vec<(ArcRichCrateVersion, u32)>,
    pub related: Vec<String>,
    pub markup: &'a Renderer,
    pub count: usize,
}

impl<'a> CatPage<'a> {
    pub async fn new(cat: &'a Category, crates: &'a KitchenSink, markup: &'a Renderer) -> Result<CatPage<'a>, Error> {
        let deadline = Instant::now() + Duration::from_secs(20);

        let (count, keywords, related) = futures::join!(
            crates.category_crate_count(&cat.slug),
            crates.top_keywords_in_category(cat),
            crates.related_categories(&cat.slug),
        );
        Ok(Self {
            count: count?.0 as usize,
            keywords: keywords?,
            related: related?,
            crates: futures::stream::iter(crates
                .top_crates_in_category(&cat.slug).await?.iter().cloned())
                .map(|o| async move {
                    let crate_timeout = Instant::now() + Duration::from_secs(5);
                    let c = match timeout_at(deadline.min(crate_timeout), crates.rich_crate_version_stale_is_ok(&o)).await {
                        Ok(Ok(c)) => c,
                        Err(e) => {
                            eprintln!("Skipping in cat {:?} because timed out {}", o, e);
                            return None;
                        },
                        Ok(Err(e)) => {
                            eprintln!("Skipping in cat {:?} because {}", o, e);
                            return None;
                        },
                    };
                    if c.is_yanked() || c.maintenance() == MaintenanceStatus::Deprecated {
                        return None;
                    }
                    let d = match crates.downloads_per_month_or_equivalent(&o).await {
                        Ok(d) => d.unwrap_or(0) as u32,
                        Err(e) => {
                            eprintln!("Skipping {:?} because dl {}", o, e);
                            return None;
                        },
                    };
                    Some((c, d))
                })
                .buffered(8)
                .filter_map(|c| async move {c})
                .collect::<Vec<_>>().await,
            cat,
            markup,
        })
    }

    pub fn has_subcategories_and_siblings(&self) -> bool {
        if self.cat.slug == "cryptography" {
            return false; // hides cryptocrrencies
        }
        !self.cat.sub.is_empty() || !self.cat.siblings.is_empty()
    }

    pub fn subcategories_and_siblings(&self) -> impl Iterator<Item = &Category> {
        self.cat.sub.values().chain(self.cat.siblings.iter().flat_map(|slug| CATEGORIES.from_slug(slug).0.into_iter()))
    }

    /// Used to render descriptions
    pub fn render_maybe_markdown_str(&self, s: &str) -> templates::Html<String> {
        crate::render_maybe_markdown_str(s, &self.markup, false, None)
    }

    /// For color of the version
    ///
    /// It tries to guess which versions seem "unstable".
    ///
    /// TODO: Merge with the better version history analysis from the individual crate page.
    pub fn version_class(&self, c: &RichCrateVersion) -> &str {
        let v = c.version_semver().unwrap();
        match (v.major, v.minor, v.patch, !v.pre.is_empty()) {
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
        self.related.iter().map(|slug| CATEGORIES.from_slug(slug).0.into_iter().filter(|c| seen.insert(&c.slug)).collect()).filter(|v: &Vec<_>| !v.is_empty()).collect()
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
        let mut c: Vec<_> = CATEGORIES.from_slug(&self.cat.slug).0;
        c.pop();
        c
    }
}
