use semver_parser;
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
use rich_crate::Include;
use rich_crate::RepoHost;
use download_graph::DownloadsGraph;
use kitchen_sink::CrateAuthor;
use categories::CATEGORIES;
use std::borrow::Cow;
use std::collections::HashMap;
use std::collections::HashSet;
use std::hash::Hash;
use urler::Urler;
use url::Url;
use Page;
use locale::Numeric;
use std::fmt::Display;
use semver::Version as SemVer;

/// Data sources used in `crate_page.rs.html`
pub struct CratePage<'a> {
    pub all: &'a RichCrate,
    pub ver: &'a RichCrateVersion,
    pub kitchen_sink: &'a KitchenSink,
    pub markup: &'a Renderer,
    pub top_keyword: Option<(u32, String)>,
    pub all_contributors: (Vec<CrateAuthor<'a>>, Vec<CrateAuthor<'a>>, bool, usize),
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
    pub semver: SemVer,
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

pub(crate) struct Contributors<'a> {
    pub authors: Vec<CrateAuthor<'a>>,
    pub owners: Vec<CrateAuthor<'a>>,
    pub co_owned: bool,
    pub contributors: usize,
    pub period_after_authors: bool,
    pub contributors_as_a_team: bool,
}


impl<'a> CratePage<'a> {
    pub fn page(&self, url: &Urler) -> Page {
        let keywords = self.ver.keywords(Include::Cleaned).collect::<Vec<_>>().join(", ");
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
        let slugs: Vec<_> = self.ver.category_slugs(Include::Cleaned).collect();
        let kind = if self.ver.has_bin() {
            if slugs.iter().any(|s| s == "development-tools::cargo-plugins") {
                "Rust/Cargo add-on"
            } else if slugs.iter().any(|s| s == "development-tools::build-utils" || s == "development-tools") {
                "utility for Rust"
            } else if slugs.iter().any(|s| s == "emulators") {
                "Rust emulator"
            } else if slugs.iter().any(|s| s == "command-line-utilities") {
                "command-line utility in Rust"
            } else if self.ver.is_app() {
                "Rust application"
            } else {
                "Rust utility"
            }
        } else if self.ver.is_sys() {
            "system library interface for Rust"
        } else if let Some(cat) = slugs.get(0).and_then(|slug| CATEGORIES.from_slug(slug).last()) {
            &cat.title
        } else if self.ver.has_lib() {
            "Rust library"
        } else {
            "Rust crate"
        };
        let name_capital = Self::capitalized(self.ver.short_name());

        if self.ver.is_yanked() {
            format!("{} {} [deprecated] — {}", name_capital, self.ver.version(), kind)
        } else {
            format!("{} — {}", name_capital, kind)
        }
    }

    pub fn is_build_or_dev(&self) -> (bool, bool) {
        self.kitchen_sink.dependents_stats_of(self.ver)
            .map(|d| {
                let is_build = d.build.0 > (d.runtime.0 + d.runtime.1 + 5) * 2;
                let is_dev = !is_build && d.dev > (d.runtime.0 * 2 + d.runtime.1 + d.build.0 * 2 + d.build.1 + 5);
                (is_build, is_dev)
            })
            .unwrap_or((false, false))
    }

    pub fn dependents_stats(&self) -> Option<(usize, usize)> {
        self.kitchen_sink.dependents_stats_of(self.ver)
        .map(|d| {
            (d.runtime.0 + d.runtime.1 + d.build.0 + d.build.1 + d.dev,
             d.direct)
        })
        .filter(|d| {
            d.0 > 0
        })
    }

    pub fn capitalized(name: &str) -> String {
        let mut name_capital = String::new();
        let mut ch = name.chars();
        if let Some(f) = ch.next() {
            name_capital.extend(f.to_uppercase());
            name_capital.extend(ch);
        }
        name_capital
    }

    /// If true, there are many other crates with this keyword. Populated first.
    pub fn keywords_populated(&self) -> Option<Vec<(String, bool)>> {
        let k = self.kitchen_sink.keywords_populated(self.ver);
        if k.is_empty() {None} else {Some(k)}
    }

    pub fn parent_crate(&self) -> Option<RichCrateVersion> {
        self.kitchen_sink.parent_crate(self.ver)
    }

    pub fn render_markdown_str(&self, s: &str) -> templates::Html<String> {
        templates::Html(self.markup.markdown_str(s, true))
    }

    pub fn render_lib_intro(&self) -> Option<templates::Html<String>> {
        if let Some(lib) = self.ver.lib_file() {
            let out = extract_doc_comments(lib);
            if !out.trim().is_empty() {
                let docs_url = self.ver.docs_rs_url();
                let base = docs_url.as_ref().map(|u| (u.as_str(),u.as_str()));
                return Some(templates::Html(self.markup.page(&Markup::Markdown(out), base, self.nofollow())));
            }
        }
        None
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

    pub(crate) fn all_contributors(&self) -> Contributors {
        let (ref authors, ref owners, co_owned, contributors) = self.all_contributors;
        let period_after_authors = !owners.is_empty() && contributors == 0;
        let contributors_as_a_team = authors.last().map_or(false, |last| last.likely_a_team());
        Contributors {
            authors: authors.clone(),
            owners: owners.clone(),
            co_owned, contributors,
            contributors_as_a_team,
            period_after_authors,
        }
    }

    pub fn format_number(&self, num: impl Display) -> String {
        Numeric::english().format_int(num)
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

    pub fn up_to_date_class(&self, richdep: &RichDep) -> &str {
        let (matches_latest, pop) = richdep.dep.req().parse().ok()
            .map(|req| self.kitchen_sink.version_popularity(&richdep.name, req))
            .unwrap_or((false, 0.));
        match pop {
            x if x >= 0.5 && matches_latest => "top",
            x if x >= 0.75 || matches_latest => "common",
            x if x >= 0.25 => "outdated",
            _ => "obsolete",
        }
    }

    /// The rule is - last displayed digit may change (except 0.x)
    pub fn pretty_print_req(&self, reqstr: &str) -> String {
        if let Ok(req) = semver_parser::range::parse(reqstr) {
            if req.predicates.len() == 1 {
                let pred = &req.predicates[0];
                if pred.pre.is_empty() {
                    use semver_parser::range::Op::*;
                    use semver_parser::range::WildcardVersion;
                    match pred.op {
                        Tilde | Compatible | Wildcard(_) => { // There's no `Wildcard(*)`
                            let detailed = pred.op == Tilde || pred.op == Wildcard(WildcardVersion::Patch);
                            return if detailed || pred.major == 0 {
                                if detailed || pred.patch.map_or(false, |p| p > 0) {
                                    format!("{}.{}.{}", pred.major, pred.minor.unwrap_or(0), pred.patch.unwrap_or(0))
                                } else {
                                    format!("{}.{}", pred.major, pred.minor.unwrap_or(0))
                                }
                            } else {
                                format!("{}.{}", pred.major, pred.minor.unwrap_or(0))
                            }
                        },
                        _ => {}
                    }
                }
            }
        }
        reqstr.to_string()
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
        self.kitchen_sink.top_category(&self.ver).and_then(|(top, slug)|{
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

    fn url_domain(url: &str) -> Option<Cow<'static, str>> {
        Url::parse(url).ok()
        .and_then(|url| {
            url.host_str()
            .and_then(|host| {
                if host.ends_with(".github.io") {
                    Some("github.io".into())
                } else if host.ends_with(".githubusercontent.com") {
                    None
                } else {
                    Some(host.trim_left_matches("www.").to_string().into())
                }
            })
        })
    }

    /// `(url, label)`
    pub fn homepage_link(&self) -> Option<(&str, Cow<str>)> {
        self.ver.homepage()
        .map(|url| {
            let label = Self::url_domain(url)
                .map(|host| {
                    let docs_on_same_host = self.ver.documentation()
                        .and_then(Self::url_domain)
                        .map_or(false, |doc_host| doc_host == host);

                    if docs_on_same_host {
                        Cow::Borrowed("Home") // there will be verbose label on docs link, so repeating it would be noisy
                    } else {
                        format!("Home ({})", host).into()
                    }
                })
                .unwrap_or_else(|| "Homepage".into());
            (url, label)
        })
    }

    /// `(url, label)`
    pub fn documentation_link(&self) -> Option<(&str, Cow<str>)> {
        self.ver.documentation()
        .map(|url| {
            let label = Self::url_domain(url)
                .map(|host| {
                    if host == "docs.rs" {
                        "API Reference".into()
                    } else {
                        Cow::Owned(format!("Documentation ({})", host))
                    }
                })
                .unwrap_or_else(|| "Documentation".into());
            (url, label)
        })
    }

    /// `(url, label)`
    pub fn repository_link(&self) -> Option<(Cow<str>, String)> {
        self.ver.repository_http_url().map(|(repo, url)| {
            let label_prefix = repo.site_link_label();
            let label = match repo.host() {
                RepoHost::GitHub(ref host) | RepoHost::GitLab(ref host)| RepoHost::BitBucket(ref host) => {
                    format!("{} ({})", label_prefix, host.owner)
                },
                RepoHost::Other => {
                    Self::url_domain(&url)
                        .map(|host| format!("{} ({})", label_prefix, host))
                        .unwrap_or_else(|| label_prefix.to_string())
                }
            };
            (url, label)
        })
    }


    /// Most relevant keyword for this crate and rank in listing for that keyword
    pub fn top_keyword(&self) -> Option<(u32, String)> {
        self.top_keyword.clone()
    }

    /// Categories and subcategories, but deduplicated
    /// so that they look neater in breadcrumbs
    pub fn category_slugs_unique(&self) -> Vec<Vec<&Category>> {
        let mut seen = HashSet::new();
        self.ver.category_slugs(Include::Cleaned).map(|slug| {
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
            semver: SemVer::parse(&v.num).expect("semver parse"),
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

    pub fn related_crates(&self) -> Option<Vec<RichCrateVersion>> {
        self.kitchen_sink.related_crates(&self.ver)
        .map_err(|e| eprintln!("related crates fail: {}", e))
        .ok()
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

fn extract_doc_comments(code: &str) -> String {
    let mut out = String::with_capacity(code.len()/2);
    let mut is_in_block_mode = false;
    for l in code.lines() {
        let l = l.trim_left();
        if is_in_block_mode {
            if let Some(offset) = l.find("*/") {
                is_in_block_mode = false;
                out.push_str(&l[0..offset]);
            } else {
                out.push_str(l);
            }
            out.push('\n');
        } else if l.starts_with("/*!") && !l.contains("*/") {
            is_in_block_mode = true;
            let rest = &l[3..];
            out.push_str(rest);
            if !rest.trim().is_empty() {
                out.push('\n');
            }
        } else if l.starts_with("//!") {
            out.push_str(&l[3..]);
            out.push('\n');
        }
    }
    out
}

#[test]
fn parse() {
    assert_eq!("hello\nworld", extract_doc_comments("/*!\nhello\nworld */").trim());
}
