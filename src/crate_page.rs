use semver;
use chrono::prelude::*;
use chrono::Duration;
use kitchen_sink::KitchenSink;
use categories::Category;
use render_readme::Renderer;
use render_readme::Markup;
use templates;
use rich_crate::RichDep;
use rich_crate::Readme;
use rich_crate::RichCrate;
use rich_crate::RichCrateVersion;
use download_graph::DownloadsGraph;
use kitchen_sink::CrateAuthor;
use categories::CATEGORIES;
use std::collections::HashMap;
use std::collections::HashSet;
use std::hash::Hash;
use urler::Urler;
use Page;

/// Data sources used in `crate_page.rs.html`
pub struct CratePage<'a> {
    pub all: &'a RichCrate,
    pub ver: &'a RichCrateVersion,
    pub kitchen_sink: &'a KitchenSink,
    pub markup: &'a Renderer,
}

/// Helper used to find most "interesting" versions
pub struct VersionGroup<'a> {
    pub ver: Version<'a>,
    pub downloads: usize,
    pub count: usize,
}

pub struct Version<'a> {
    pub downloads: usize,
    pub num: &'a str,
    pub semver: semver::Version,
    pub yanked: bool,
    pub created_at: DateTime<FixedOffset>,
}

///
#[derive(Debug, Default)]
struct ReleaseCounts {
    total: usize,
    stable: usize,
    major: usize,
    major_recent: usize,
    patch: usize,
    unstable: usize,
    breaking: usize,
    breaking_recent: usize,
}

impl<'a> CratePage<'a> {
    pub fn page(&self, url: &Urler) -> Page {
        let keywords = self.ver.keywords().collect::<Vec<_>>().join(", ");
        Page {
            title: self.page_title(),
            keywords: if keywords != "" {Some(keywords)} else {None},
            created: Some(self.date_created()),
            description: self.ver.description().map(|d| format!("{} | Rust package at Crates.rs", d)),
            item_name: Some(self.ver.short_name().to_string()),
            item_description: self.ver.description().map(|d| d.to_string()),
            alternate: self.ver.crates_io_url(),
            canonical: Some(url.krate(&self.ver)),
            noindex: self.ver.is_yanked(),
            alt_critical_css: None,
        }
    }
    pub fn page_title(&self) -> String {
        let kind = if self.ver.has_bin() {
            if self.ver.category_slugs().any(|s| s == "development-tools::cargo-plugins") {
                "Rust/Cargo add-on"
            } else if self.ver.category_slugs().any(|s| s == "development-tools::build-utils" || s == "development-tools") {
                "utility for Rust"
            } else if self.ver.category_slugs().any(|s| s == "emulators") {
                "Rust emulator"
            } else if self.ver.category_slugs().any(|s| s == "command-line-utilities") {
                "command-line utility in Rust"
            } else if self.ver.is_app() {
                "Rust application"
            } else {
                "Rust utility"
            }
        } else if self.ver.is_sys() {
            "system library interface for Rust"
        } else if let Some((_, cat)) = self.top_category() {
            &cat.title
        } else if self.ver.has_lib() {
            "Rust library"
        } else {
            "Rust crate"
        };
        let mut name_capital = String::new();
        let mut ch = self.ver.short_name().chars();
        if let Some(f) = ch.next() {
            name_capital.extend(f.to_uppercase());
            name_capital.extend(ch);
        }

        if self.ver.is_yanked() {
            format!("{} {} [deprecated] — {}", name_capital, self.ver.version(), kind)
        } else {
            format!("{} — {}", name_capital, kind)
        }
    }

    pub fn name_underscore_parts(&self) -> impl Iterator<Item = &str> {
        self.ver.short_name().split('_')
    }

    pub fn render_markdown_str(&self, s: &str) -> templates::Html<String> {
        templates::Html(self.markup.markdown_str(s, true))
    }

    pub fn render_lib_intro(&self) -> Option<templates::Html<String>> {
        if let Some(lib) = self.ver.lib_file() {
            let mut out = String::new();
            for l in lib.lines() {
                let l = l.trim_left();
                if l.starts_with("//!") {
                    out.push_str(&l[3..]);
                    out.push('\n');
                }
            }
            if !out.is_empty() {
                let docs_url = self.ver.docs_rs_url();
                let base = docs_url.as_ref().map(|u| (u.as_str(),u.as_str()));
                return Some(templates::Html(self.markup.page(&Markup::Markdown(out), base, self.nofollow())));
            }
        }
        return None;
    }

    pub fn is_readme_short(&self) -> bool {
        if let Ok(Some(ref r)) = self.ver.readme() {
            match r.markup {
                Markup::Markdown(ref s) | Markup::Rst(ref s) => s.len() < 1000,
            }
        } else {
            true
        }
    }

    pub fn render_readme(&self, readme: &Readme) -> templates::Html<String> {
        let urls = match (readme.base_url.as_ref(), readme.base_image_url.as_ref()) {
            (Some(l), Some(i)) => Some((l.as_str(),i.as_str())),
            (Some(l), None) => Some((l.as_str(),l.as_str())),
            _ => None,
        };
        templates::Html(self.markup.page(&readme.markup, urls, self.nofollow()))
    }

    pub fn nofollow(&self) -> bool {
        // TODO: take multiple factors into account, like # of contributors, author reputation, dependents
        self.all.downloads_recent() < 100
    }

    pub fn all_contributors(&self) -> (Vec<CrateAuthor<'a>>, Vec<CrateAuthor<'a>>, bool, usize, bool) {
        let (authors, owners, co_owned, contributors) = self.kitchen_sink.all_contributors(&self.ver);
        let period_after_authors = !owners.is_empty() && contributors == 0;
        (authors, owners, co_owned, contributors, period_after_authors)
    }

    pub fn format(date: &DateTime<FixedOffset>) -> String {
        date.format("%b %e, %Y").to_string()
    }

    pub fn format_month(date: &DateTime<FixedOffset>) -> String {
        date.format("%b %Y").to_string()
    }

    pub fn dependencies(&self) -> Option<(Vec<RichDep>, Vec<RichDep>, Vec<RichDep>)> {
        self.ver.dependencies().ok()
    }

    pub fn top_versions(&self) -> impl Iterator<Item = VersionGroup<'a>> {
        let mut top = self.make_top_versions(self.all_versions());
        if top.len() > 5 {
            top.swap_remove(4); // move last to 5th pos, so that first release is always seen
            top.truncate(5);
        }
        top.into_iter()
    }

    pub fn is_version_new(&self, ver: &Version, nth: usize) -> bool {
        nth == 0 /*latest*/ && ver.created_at.with_timezone(&Utc) > Utc::now() - Duration::weeks(1)
    }

    pub fn has_runtime_deps(&self, ) -> bool {
        self.ver.links().is_some() || self.ver.has_runtime_deps()
    }

    fn group_versions<K, I>(keep_first_n: usize, all: I) -> Vec<VersionGroup<'a>>
        where I: Iterator<Item = (K, VersionGroup<'a>)>, K: Eq + Hash
    {
        use std::collections::hash_map::Entry::*;
        let mut grouped = HashMap::<(K, bool), VersionGroup<'a>>::new();
        for (i, (key, v)) in all.enumerate() {
            let key = (key, i < keep_first_n);
            match grouped.entry(key) {
                Occupied(mut e) => {
                    let old = e.get_mut();
                    old.count += v.count;
                    old.downloads += v.downloads;
                    if old.ver.semver < v.ver.semver &&
                        (old.ver.yanked || !v.ver.yanked) &&
                        (old.ver.semver.is_prerelease() || !v.ver.semver.is_prerelease()) {
                        old.ver = v.ver;
                    }
                },
                Vacant(e) => {
                    e.insert(v);
                },
            };
        }
        let mut grouped: Vec<_> = grouped.into_iter().map(|(_,v)| v).collect();
        grouped.sort_by(|a,b| b.ver.semver.cmp(&a.ver.semver));
        grouped
    }

    fn make_top_versions<I>(&self, all: I) -> Vec<VersionGroup<'a>>
        where I: Iterator<Item = Version<'a>>
    {
        let grouped1 = Self::group_versions(0, all.map(|ver| {
            let key = (
                ver.created_at.year(), ver.created_at.month(), ver.created_at.day(),

                // semver exposes major bumps, specially for 0.x
                if ver.semver.major == 0 {ver.semver.minor+1} else {0}, ver.semver.major,
                // exposes minor changes
                if ver.semver.major == 0 {ver.semver.patch+1} else {0}, ver.semver.minor,
            );
            (key, VersionGroup {
                count: 1,
                downloads: ver.downloads,
                ver,
            })
        }));

        let grouped2 = if grouped1.len() > 5 {
            Self::group_versions(1, grouped1.into_iter().map(|v| {
                ((v.ver.created_at.year(), v.ver.created_at.month(),
                  // semver exposes major bumps, specially for 0.x
                  if v.ver.semver.major == 0 {v.ver.semver.minor+1} else {0}, v.ver.semver.major,
                ), v)
            }))
        } else {
            grouped1
        };

        if grouped2.len() > 8 {
            Self::group_versions(2, grouped2.into_iter().map(|v|{
                ((v.ver.created_at.year(),
                  v.ver.created_at.month()/4,
                  v.ver.semver.major,
                ), v)
            }))
        } else {
            grouped2
        }
    }

    /// String describing how often breaking changes are made
    pub fn version_stats_summary(&self) -> String {
        self.version_stats().summary()
    }

    /// Counts minor and major releases for the summary
    fn version_stats(&self) -> ReleaseCounts {
        let mut cnt = ReleaseCounts::default();
        let mut prev: Option<Version<'a>> = None;
        let mut all: Vec<_> = self.all_versions().collect();
        all.sort_by(|a,b| a.semver.cmp(&b.semver));
        cnt.total = all.len();
        let recent = *all.iter().map(|d| &d.created_at)
            .max().expect("no versions") - Duration::weeks(40);
        for v in all {
            if v.semver.major == 0 {
                cnt.unstable += 1;
                if let Some(ref prev) = prev {
                    if v.semver.minor != prev.semver.minor {
                        cnt.breaking += 1;
                        if v.created_at >= recent {
                            cnt.breaking_recent += 1;
                        }
                    }
                }
            } else {
                if !v.semver.is_prerelease() {
                    cnt.stable += 1;
                }
                if let Some(ref prev) = prev {
                    if v.semver.major != prev.semver.major {
                        cnt.major += 1;
                        if v.created_at >= recent {
                            cnt.major_recent += 1;
                        }
                    }
                }
            }
            if let Some(ref prev) = prev {
                if v.semver.major == prev.semver.major &&
                   v.semver.minor == prev.semver.minor &&
                   (v.semver.patch != prev.semver.patch || v.semver.pre != prev.semver.pre) {
                    cnt.patch += 1;
                }
            }
            prev = Some(v);
        }
        cnt
    }

    /// Most relevant category for the crate and rank in that category
    pub fn top_category(&self) -> Option<(u32, &Category)> {
        self.kitchen_sink.top_category(&self.all).and_then(|(top, slug)|{
            CATEGORIES.from_slug(slug).last().map(|c| {
                (top, c)
            })
        })
    }

    /// docs.rs link, if available
    pub fn api_reference_url(&self) -> Option<String> {
        if self.kitchen_sink.has_docs_rs(self.ver.short_name(), self.ver.version()) {
            Some(format!("https://docs.rs/{}", self.ver.short_name()))
        } else {
            None
        }
    }

    /// Most relevant keyword for this crate and rank in listing for that keyword
    pub fn top_keyword(&self) -> Option<(u32, String)> {
        self.kitchen_sink.top_keyword(&self.all)
    }

    /// Categories and subcategories, but deduplicated
    /// so that they look neater in breadcrumbs
    pub fn category_slugs_unique(&self) -> Vec<Vec<&Category>> {
        let mut seen = HashSet::new();
        self.ver.category_slugs().map(|slug| {
            CATEGORIES.from_slug(slug).filter(|c| {
                if seen.get(&c.slug).is_some() {
                    return false;
                }
                seen.insert(&c.slug);
                true
            }).collect()
        })
        .filter(|v: &Vec<_>| !v.is_empty())
        .collect()
    }

    pub fn date_created(&self) -> String {
        self.most_recent_version().created_at.format("%Y-%m-%d").to_string()
    }

    pub fn most_recent_version(&self) -> Version<'a> {
        self.all_versions().max_by(|a,b| a.created_at.cmp(&b.created_at)).expect("no versions?")
    }

    pub fn all_versions(&self) -> impl Iterator<Item = Version<'a>> {
        self.all.versions().map(|v| Version {
            yanked: v.yanked,
            downloads: v.downloads,
            num: &v.num,
            semver: semver::Version::parse(&v.num).expect("semver parse"),
            created_at: DateTime::parse_from_rfc3339(&v.created_at).expect("created_at parse"),
        })
    }

    pub fn published_date(&self) -> DateTime<FixedOffset> {
        let min_iso_date = self.all.versions().map(|v| &v.created_at).min().expect("any version in the crate");
        DateTime::parse_from_rfc3339(min_iso_date).expect("created_at parse")
    }

    /// Data for weekly breakdown of recent downloads
    pub fn download_graph(&self) -> DownloadsGraph {
        DownloadsGraph::new(self.all.weekly_downloads(), self.ver.has_bin())
    }
}


impl ReleaseCounts {

    /// Judge how often the crate makes breaking or stable releases
    pub fn summary(&self) -> String {
        // TODO take yanked into account
        // show (n this year|n last year)
        let (n,label,n2,label2,majorinfo) = if self.stable > 0 {
            let breaking = self.major_recent > 2 && self.major * 2 >= self.stable;
            if breaking {
                (self.major, "major breaking", 0, "", false)
            } else {
                (self.stable, "stable", self.major, "major", self.major > 2.max(self.stable/16))
            }
        } else {
            let very_breaking = self.unstable > 2 &&
                (self.breaking_recent > 3 || self.breaking * 2 >= self.unstable);
            if very_breaking {
                (self.breaking, "breaking", 0, "", false)
            } else {
                let bad = self.breaking_recent > 1.max(self.unstable/9) && self.breaking > 2.max(self.unstable/8);
                let good = self.patch * 2 >= self.total;
                (self.unstable, if !bad && good {""} else {"unstable"}, self.breaking, "breaking", bad)
            }
        };
        if n == self.total || (n > 7 && n * 10 >= self.total * 8) {
            if majorinfo {
                format!("{} {} release{} ({} {})", n, label, if n==1 {""} else {"s"}, n2, label2)
            } else {
                format!("{} {} release{}", n, label, if n==1 {""} else {"s"},)
            }
        }
        else if n * 3 >= self.total * 2 {
            format!("{} release{} ({})", self.total, if self.total==1 {""} else {"s"}, label)
        }
        else {
            format!("{} release{} ({} {})", self.total, if self.total==1 {""} else {"s"}, n, label)
        }
    }
}
