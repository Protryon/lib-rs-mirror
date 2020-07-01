use kitchen_sink::ArcRichCrateVersion;
use kitchen_sink::RevDependencies;
use crate::download_graph::DownloadsGraph;
use crate::templates;
use crate::urler::Urler;
use crate::Page;
use categories::Category;
use categories::CATEGORIES;
use chrono::prelude::*;
use chrono::Duration;
use futures::future::Future;
use futures::stream::StreamExt;
use kitchen_sink::CResult;
use kitchen_sink::CrateAuthor;
use kitchen_sink::DepInfMap;
use kitchen_sink::{DepTy, KitchenSink, Origin};
use locale::Numeric;
use render_readme::Links;
use render_readme::Renderer;
use rich_crate::Readme;
use rich_crate::RepoHost;
use rich_crate::RichCrate;
use rich_crate::RichCrateVersion;
use rich_crate::RichDep;
use semver::Version as SemVer;
use semver_parser;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::collections::HashSet;
use std::f64::consts::PI;
use std::fmt::Display;
use std::hash::Hash;
use tokio::runtime::Handle;
use udedokei::LanguageExt;
use udedokei::{Language, Lines, Stats};
use crate::url_domain;

pub struct CrateLicense {
    pub origin: Origin,
    pub license: Box<str>,
    pub optional: bool,
}

pub struct CrateSizes {
    pub tarball: usize,
    pub uncompressed: usize,
    pub minimal: DepsSize,
    pub typical: DepsSize,
}

/// Data sources used in `crate_page.rs.html`
pub struct CratePage<'a> {
    pub all: &'a RichCrate,
    pub ver: &'a RichCrateVersion,
    pub kitchen_sink: &'a KitchenSink,
    pub markup: &'a Renderer,
    pub top_keyword: Option<(u32, String)>,
    pub all_contributors: (Vec<CrateAuthor<'a>>, Vec<CrateAuthor<'a>>, bool, usize),
    /// own, deps (tarball, uncompressed source); last one is sloc
    pub sizes: Option<CrateSizes>,
    pub lang_stats: Option<(usize, Stats)>,
    pub viral_license: Option<CrateLicense>,
    top_category: Option<(u32, &'static Category)>,
    is_build_or_dev: (bool, bool),
    handle: Handle,
    api_reference_url: Option<String>,
    former_glory: Option<(f64, u32)>,
    dependents_stats: Option<&'a RevDependencies>,
    changelog_url: Option<String>,
    related_crates: Option<Vec<Origin>>,
    keywords_populated: Vec<(String, bool)>,
    parent_crate: Option<ArcRichCrateVersion>,
    downloads_per_month_or_equivalent: Option<usize>,
    has_docs_rs: bool,
    pub has_reviews: bool,
}

/// Helper used to find most "interesting" versions
#[derive(Debug)]
pub struct VersionGroup<'a> {
    pub ver: Version<'a>,
    pub count: usize,
}

#[derive(Debug)]
pub struct Version<'a> {
    pub num: &'a str,
    pub semver: SemVer,
    pub yanked: bool,
    pub created_at: DateTime<FixedOffset>,
}

///
#[derive(Debug, Default)]
struct ReleaseCounts {
    total: u32,
    stable: u32,
    major: u32,
    major_recent: u32,
    patch: u32,
    unstable: u32,
    breaking: u32,
    breaking_recent: u32,
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
    pub async fn new(all: &'a RichCrate, ver: &'a RichCrateVersion, kitchen_sink: &'a KitchenSink, markup: &'a Renderer) -> CResult<CratePage<'a>> {
        let (top_category, parent_crate, changelog_url, keywords_populated, (related_crates, downloads_per_month_or_equivalent)) = futures::join!(
            kitchen_sink.top_category(ver),
            kitchen_sink.parent_crate(ver),
            kitchen_sink.changelog_url(ver),
            kitchen_sink.keywords_populated(ver),
            async {
                let downloads_per_month_or_equivalent = kitchen_sink.downloads_per_month_or_equivalent(ver.origin()).await.ok().and_then(|x| x);
                let related_crates = Self::make_related_crates(kitchen_sink, ver, downloads_per_month_or_equivalent).await;
                (related_crates, downloads_per_month_or_equivalent)
            },
        );
        let top_category = top_category
            .and_then(|(top, slug)| CATEGORIES.from_slug(slug).0.last().map(|&c| (top, c)));

        let (is_build_or_dev, parent_crate, dependents_stats, former_glory, top_keyword, all_contributors) = futures::try_join!(
            async { Ok(kitchen_sink.is_build_or_dev(ver.origin()).await?) },
            async {
                Ok(if let Some(origin) = parent_crate {
                    kitchen_sink.rich_crate_version_async(&origin).await.ok()
                } else {
                    None
                })
            },
            async {
                Ok(kitchen_sink.crates_io_dependents_stats_of(all.origin()).await?)
            },
            kitchen_sink.former_glory(all.origin()),
            kitchen_sink.top_keyword(all),
            kitchen_sink.all_contributors(ver),
        )?;

        let deps = kitchen_sink.all_dependencies_flattened(ver);
        let has_docs_rs = kitchen_sink.has_docs_rs(ver.origin(), ver.short_name(), ver.version()).await;
        let api_reference_url = if has_docs_rs {
            Some(format!("https://docs.rs/{}", ver.short_name()))
        } else {
            None
        };
        let has_reviews = !kitchen_sink.reviews_for_crate(ver.origin()).is_empty();
        let mut page = Self {
            top_keyword,
            all_contributors,
            all,
            ver,
            kitchen_sink,
            markup,
            sizes: None,
            lang_stats: None,
            viral_license: None,
            top_category,
            is_build_or_dev,
            handle: Handle::current(),
            api_reference_url,
            former_glory,
            dependents_stats,
            changelog_url,
            related_crates,
            keywords_populated,
            parent_crate,
            downloads_per_month_or_equivalent,
            has_reviews,
            has_docs_rs,
        };
        let (sizes, lang_stats, viral_license) = page.crate_size_and_viral_license(deps?).await?;
        page.sizes = Some(sizes);
        page.viral_license = viral_license;

        let total = lang_stats.langs.iter().filter(|(lang, _)| lang.is_code()).map(|(_, lines)| lines.code).sum::<u32>();
        page.lang_stats = Some((total as usize, lang_stats));
        Ok(page)
    }

    pub fn page(&self, url: &Urler) -> Page {
        let keywords = self.ver.keywords().join(", ");
        Page {
            title: self.page_title(),
            keywords: if keywords != "" { Some(keywords) } else { None },
            created: self.date_created_string(),
            description: self.ver.description().map(|d| format!("{} | Rust/Cargo package", d)),
            item_name: Some(self.ver.short_name().to_string()),
            item_description: self.ver.description().map(|d| d.to_string()),
            alternate: url.crates_io_crate(self.ver.origin()),
            canonical: Some(format!("https://lib.rs{}", url.crate_abs_path_by_origin(self.ver.origin()))),
            noindex: self.ver.is_yanked(),
            search_meta: false,
            ..Default::default()
        }
    }

    pub fn page_title(&self) -> String {
        let slugs: Vec<_> = self.ver.category_slugs().collect();
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
        } else if let Some(cat) = slugs.get(0).and_then(|slug| CATEGORIES.from_slug(slug).0.last().copied()) {
            &cat.title
        } else if self.ver.has_lib() {
            "Rust library"
        } else {
            "Rust crate"
        };
        let name_capital = self.ver.capitalized_name();

        if self.ver.is_yanked() || self.former_glory.map_or(false, |(f, _)| f < 0.3) {
            format!("{} {} [deprecated]", name_capital, self.ver.version())
        } else {
            format!("{} — {}", name_capital, kind)
        }
    }

    pub fn changelog_url(&self) -> Option<&str> {
        self.changelog_url.as_deref()
    }

    pub fn is_build_or_dev(&self) -> (bool, bool) {
        self.is_build_or_dev
    }

    pub fn dependents_stats(&self) -> Option<DepsStatsResult> {
        let d = self.dependents_stats?;
        let d = DepsStatsResult {
            deps: d.runtime.def as u32 + d.runtime.opt as u32 + d.build.def as u32 + d.build.opt as u32 + d.dev as u32,
            direct: d.direct.all() as u32,
            name: d.rev_dep_names.iter().next(),
            former_glory: self.former_glory.map(|(f, _)| f).unwrap_or(1.),
        };
        if d.deps == 0 {
            return None;
        }
        Some(d)
    }

    /// If true, there are many other crates with this keyword. Populated first.
    pub fn keywords_populated(&self) -> Option<&[(String, bool)]> {
        if !self.keywords_populated.is_empty() {
            Some(&self.keywords_populated)
        } else {
            None
        }
    }

    pub fn parent_crate(&self) -> Option<&RichCrateVersion> {
        self.parent_crate.as_deref()
    }

    pub fn render_markdown_str(&self, s: &str) -> templates::Html<String> {
        templates::Html(self.markup.markdown_str(s, true, Some(self.ver.short_name())))
    }

    pub fn render_lib_intro(&self) -> Option<templates::Html<String>> {
        self.ver.lib_file_markdown().map(|markup| {
            let docs_url = self.ver.docs_rs_url();
            let base = docs_url.as_ref().map(|u| (u.as_str(), u.as_str()));
            let (html, warnings) = self.markup.page(&markup, base, self.nofollow(), Some(self.ver.short_name()));
            if !warnings.is_empty() {
                eprintln!("{} lib: {:?}", self.ver.short_name(), warnings);
            }
            templates::Html(html)
        })
    }

    pub fn is_readme_short(&self) -> bool {
        self.kitchen_sink.is_readme_short(self.ver.readme().as_ref().map(|r| &r.markup))
    }

    pub fn has_no_readme_or_lib(&self) -> bool {
        self.ver.readme().is_none() && self.ver.lib_file_markdown().is_none()
    }

    pub fn render_readme(&self, readme: &Readme) -> templates::Html<String> {
        let urls = match (readme.base_url.as_ref(), readme.base_image_url.as_ref()) {
            (Some(l), Some(i)) => Some((l.as_str(), i.as_str())),
            (Some(l), None) => Some((l.as_str(), l.as_str())),
            _ => None,
        };
        let (html, warnings) = self.markup.page(&readme.markup, urls, self.nofollow(), Some(self.ver.short_name()));
        if !warnings.is_empty() {
            eprintln!("{} readme: {:?}", self.ver.short_name(), warnings);
        }
        templates::Html(html)
    }

    pub fn nofollow(&self) -> Links {
        // TODO: take multiple factors into account, like # of contributors, author reputation, dependents
        if self.downloads_per_month_or_equivalent.unwrap_or(0) < 100 {
            Links::Ugc
        } else {
            Links::FollowUgc
        }
    }

    fn block<O>(&self, f: impl Future<Output = O>) -> O {
        self.handle.enter(|| futures::executor::block_on(f))
    }

    pub(crate) fn all_contributors(&self) -> Contributors<'_> {
        let (ref authors, ref owners, co_owned, contributors) = self.all_contributors;
        let period_after_authors = !owners.is_empty() && contributors == 0;
        let contributors_as_a_team = authors.last().map_or(false, |last| last.likely_a_team());
        Contributors { authors: authors.clone(), owners: owners.clone(), co_owned, contributors, contributors_as_a_team, period_after_authors }
    }

    pub fn format_number(&self, num: impl Display) -> String {
        Numeric::english().format_int(num)
    }

    pub fn format_knumber(&self, num: usize) -> (String, &'static str) {
        let (num, unit) = match num {
            0..=899 => (num, ""),
            0..=8000 => return (format!("{}", ((num + 250) / 500) as f64 * 0.5), "K"), // 3.5K
            0..=899_999 => ((num + 500) / 1000, "K"),
            0..=9_999_999 => return (format!("{}", ((num + 250_000) / 500_000) as f64 * 0.5), "M"), // 3.5M
            _ => ((num + 500_000) / 1_000_000, "M"),                                                // 10M
        };
        (Numeric::english().format_int(num), unit)
    }

    pub fn format_kbytes(&self, bytes: usize) -> String {
        let (num, unit) = match bytes {
            0..=100_000 => ((bytes + 999) / 1000, "KB"),
            0..=800_000 => ((bytes + 3999) / 5000 * 5, "KB"),
            0..=9_999_999 => return format!("{}MB", ((bytes + 250_000) / 500_000) as f64 * 0.5),
            _ => ((bytes + 500_000) / 1_000_000, "MB"),
        };
        format!("{}{}", Numeric::english().format_int(num), unit)
    }

    fn format_number_frac(num: f64) -> String {
        if num > 0.05 && num < 10. && num.fract() > 0.09 && num.fract() < 0.9 {
            if num < 3. {
                format!("{:.1}", num)
            } else {
                format!("{}", (num * 2.).round() / 2.)
            }
        } else {
            Numeric::english().format_int(if num > 500. {
                (num / 10.).round() * 10.
            } else if num > 100. {
                (num / 5.).round() * 5.
            } else {
                num.round()
            })
        }
    }

    pub fn format_kbytes_range(&self, a: usize, b: usize) -> String {
        let min_bytes = a.min(b);
        let max_bytes = a.max(b);

        // if the range is small, just display the upper number
        if min_bytes * 4 > max_bytes * 3 || max_bytes < 250_000 {
            return self.format_kbytes(max_bytes);
        }

        let (denom, unit) = match max_bytes {
            0..=800_000 => (1000., "KB"),
            _ => (1_000_000., "MB"),
        };
        let mut low_val = min_bytes as f64 / denom;
        let high_val = max_bytes as f64 / denom;
        if low_val > 1. && high_val > 10. {
            low_val = low_val.round(); // spread is so high that precision of low end isn't relevant
        }
        format!("{}–{}{}", Self::format_number_frac(low_val), Self::format_number_frac(high_val), unit)
    }

    /// Display number 0..1 as percent
    pub fn format_fraction(&self, num: f64) -> String {
        if num < 1.9 {
            format!("{:0.1}%", num)
        } else {
            format!("{}%", Numeric::english().format_int(num.round() as usize))
        }
    }

    pub fn format(date: &DateTime<FixedOffset>) -> String {
        date.format("%b %e, %Y").to_string()
    }

    pub fn format_month(date: &DateTime<FixedOffset>) -> String {
        date.format("%b %Y").to_string()
    }

    pub fn non_dep_features(&self) -> Option<Vec<String>> {
        let f = self.ver.features();
        let tmp: Vec<_> = f.iter().filter_map(|(k,v)| {
            let non_dep = k != "default" && v.iter().all(|feature| f.get(feature).is_some());
            if non_dep {
                Some(k.to_owned())
            } else {
                None
            }
        }).collect();

        if !tmp.is_empty() {
            Some(tmp)
        } else {
            None
        }
    }

    pub fn direct_dependencies(&self) -> Option<(Vec<RichDep>, Vec<RichDep>, Vec<RichDep>)> {
        self.ver.direct_dependencies().ok()
    }

    pub fn up_to_date_class(&self, richdep: &RichDep) -> &str {
        if richdep.dep.req() == "*" || !richdep.dep.is_crates_io() {
            return "common";
        }
        self.block(async {
            if let Ok(req) = richdep.dep.req().parse() {
                if let Ok(Some((matches_latest, pop))) = self.kitchen_sink.version_popularity(&richdep.package, &req).await {
                    return match pop {
                        x if x >= 0.5 && matches_latest => "top",
                        x if x >= 0.75 || matches_latest => "common",
                        x if x >= 0.25 => "outdated",
                        _ => "obsolete",
                    };
                }
            }
            "obsolete"
        })
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
                        Tilde | Compatible | Wildcard(_) => {
                            // There's no `Wildcard(*)`
                            let detailed = pred.op == Tilde || pred.op == Wildcard(WildcardVersion::Patch);
                            return if detailed || pred.major == 0 {
                                if detailed || pred.patch.map_or(false, |p| p > 0) {
                                    format!("{}.{}.{}", pred.major, pred.minor.unwrap_or(0), pred.patch.unwrap_or(0))
                                } else {
                                    format!("{}.{}", pred.major, pred.minor.unwrap_or(0))
                                }
                            } else {
                                format!("{}.{}", pred.major, pred.minor.unwrap_or(0))
                            };
                        },
                        _ => {},
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

    pub fn is_version_new(&self, ver: &Version<'_>, nth: usize) -> bool {
        nth == 0 /*latest*/ && ver.created_at.with_timezone(&Utc) > Utc::now() - Duration::weeks(1)
    }

    pub fn has_runtime_deps(&self) -> bool {
        self.ver.links().is_some() || self.ver.has_runtime_deps()
    }

    fn group_versions<K, I>(keep_first_n: usize, all: I) -> Vec<VersionGroup<'a>>
    where
        I: Iterator<Item = (K, VersionGroup<'a>)>,
        K: Eq + Hash,
    {
        use std::collections::hash_map::Entry::*;
        let mut grouped = HashMap::<(K, bool), VersionGroup<'a>>::new();
        for (i, (key, v)) in all.enumerate() {
            let key = (key, i < keep_first_n);
            match grouped.entry(key) {
                Occupied(mut e) => {
                    let old = e.get_mut();
                    old.count += v.count;
                    if old.ver.semver < v.ver.semver && (old.ver.yanked || !v.ver.yanked) && (old.ver.semver.is_prerelease() || !v.ver.semver.is_prerelease()) {
                        old.ver = v.ver;
                    }
                },
                Vacant(e) => {
                    e.insert(v);
                },
            };
        }
        let mut grouped: Vec<_> = grouped.into_iter().map(|(_, v)| v).collect();
        grouped.sort_by(|a, b| b.ver.semver.cmp(&a.ver.semver));
        grouped
    }

    fn make_top_versions<I>(&self, all: I) -> Vec<VersionGroup<'a>>
    where I: Iterator<Item = Version<'a>> {
        let grouped1 = Self::group_versions(
            0,
            all.map(|ver| {
                let key = (
                    ver.created_at.year(),
                    ver.created_at.month(),
                    ver.created_at.day(),
                    // semver exposes major bumps, specially for 0.x
                    if ver.semver.major == 0 { ver.semver.minor + 1 } else { 0 },
                    ver.semver.major,
                    // exposes minor changes
                    if ver.semver.major == 0 { ver.semver.patch + 1 } else { 0 },
                    ver.semver.minor,
                );
                (key, VersionGroup { count: 1, ver })
            }),
        );
        let grouped2 = if grouped1.len() > 5 {
            Self::group_versions(
                1,
                grouped1.into_iter().map(|v| {
                    (
                        (
                            v.ver.created_at.year(),
                            v.ver.created_at.month(),
                            // semver exposes major bumps, specially for 0.x
                            if v.ver.semver.major == 0 { v.ver.semver.minor + 1 } else { 0 },
                            v.ver.semver.major,
                        ),
                        v,
                    )
                }),
            )
        } else {
            grouped1
        };

        if grouped2.len() > 8 {
            Self::group_versions(2, grouped2.into_iter().map(|v| ((v.ver.created_at.year(), v.ver.created_at.month() / 4, v.ver.semver.major), v)))
        } else {
            grouped2
        }
    }

    /// String describing how often breaking changes are made
    pub fn version_stats_summary(&self) -> Option<(String, Option<String>)> {
        self.version_stats().map(|v| v.summary())
    }

    /// Counts minor and major releases for the summary
    fn version_stats(&self) -> Option<ReleaseCounts> {
        let mut cnt = ReleaseCounts::default();
        let mut prev: Option<Version<'a>> = None;
        let mut all: Vec<_> = self.all_versions().filter(|v| !v.yanked).collect();
        all.sort_by(|a, b| a.semver.cmp(&b.semver));
        cnt.total = all.len() as u32;
        let recent = *all.iter().map(|d| &d.created_at).max()? - Duration::weeks(40);
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
                    (v.semver.patch != prev.semver.patch || v.semver.pre != prev.semver.pre)
                {
                    cnt.patch += 1;
                }
            }
            prev = Some(v);
        }
        Some(cnt)
    }

    /// Most relevant category for the crate and rank in that category
    pub fn top_category(&self) -> Option<(u32, &'static Category)> {
        self.top_category
    }

    /// docs.rs link, if available
    pub fn api_reference_url(&self) -> Option<&str> {
        self.api_reference_url.as_deref()
    }

    /// `(url, label)`
    pub fn homepage_link(&self) -> Option<(&str, Cow<'_, str>)> {
        self.ver.homepage().map(|url| {
            let label = url_domain(url)
                .map(|host| {
                    let docs_on_same_host = self.ver.documentation().and_then(url_domain).map_or(false, |doc_host| doc_host == host);

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
    pub fn documentation_link(&self) -> Option<(&str, Cow<'_, str>)> {
        self.ver.documentation().map(|url| {
            let label = url_domain(url)
                .map(|host| if host == "docs.rs" { "API Reference".into() } else { Cow::Owned(format!("Documentation ({})", host)) })
                .unwrap_or_else(|| "Documentation".into());
            (url, label)
        })
    }

    /// `(url, label)`
    pub fn repository_link(&self) -> Option<(Cow<'_, str>, String)> {
        if let Some((repo, url)) = self.ver.repository_http_url() {
            let label_prefix = repo.site_link_label();
            let label = match repo.host() {
                RepoHost::GitHub(ref host) | RepoHost::GitLab(ref host) | RepoHost::BitBucket(ref host) => format!("{} ({})", label_prefix, host.owner),
                RepoHost::Other => url_domain(&url).map(|host| format!("{} ({})", label_prefix, host)).unwrap_or_else(|| label_prefix.to_string()),
            };
            Some((url, label))
        } else if self.has_docs_rs {
            // crates without a repo get docs.rs' HTTP crate file viewer link
            Some((format!("https://docs.rs/crate/{}/{}/source/", self.ver.short_name(), self.ver.version()).into(), "Source".into()))
        } else {
            None
        }
    }

    /// Most relevant keyword for this crate and rank in listing for that keyword
    pub fn top_keyword(&self) -> Option<(u32, String)> {
        self.top_keyword.clone()
    }

    /// Categories and subcategories, but deduplicated
    /// so that they look neater in breadcrumbs
    pub fn category_slugs_unique(&self) -> Vec<Vec<&Category>> {
        let mut seen = HashSet::new();
        self.ver
            .category_slugs()
            .map(|slug| {
                CATEGORIES
                    .from_slug(slug).0.into_iter()
                    .filter(|c| {
                        if seen.get(&c.slug).is_some() {
                            return false;
                        }
                        seen.insert(&c.slug);
                        true
                    })
                    .collect()
            })
            .filter(|v: &Vec<_>| !v.is_empty())
            .collect()
    }

    pub fn date_created(&self) -> Option<DateTime<FixedOffset>> {
        self.most_recent_version().map(|v| v.created_at)
    }

    pub fn date_created_string(&self) -> Option<String> {
        self.date_created().map(|v| v.format("%Y-%m-%d").to_string())
    }

    fn most_recent_version(&self) -> Option<Version<'a>> {
        self.all_versions().max_by(|a, b| a.created_at.cmp(&b.created_at))
    }

    pub fn all_versions(&self) -> impl Iterator<Item = Version<'a>> {
        self.all.versions().iter().filter_map(|v| Some(Version {
            yanked: v.yanked,
            num: &v.num,
            semver: SemVer::parse(&v.num).map_err(|e| eprintln!("semver parse {} {:?}", e, v.num)).ok()?,
            created_at: DateTime::parse_from_rfc3339(&v.created_at).expect("created_at parse"),
        }))
    }

    pub fn published_date(&self) -> DateTime<FixedOffset> {
        let min_iso_date = self.all.versions().iter().map(|v| &v.created_at).min().expect("any version in the crate");
        DateTime::parse_from_rfc3339(min_iso_date).expect("created_at parse")
    }

    /// Data for weekly breakdown of recent downloads
    pub fn download_graph(&self, width: usize, height: usize) -> Option<DownloadsGraph> {
        Some(DownloadsGraph::new(self.kitchen_sink.weekly_downloads(self.all, 16).ok()?, self.ver.has_bin(), width, height))
    }

    pub fn downloads_per_month(&self) -> Option<usize> {
        self.block(self.kitchen_sink.downloads_per_month(self.all.origin())).ok().and_then(|x| x)
    }

    pub fn github_stargazers_and_watchers(&self) -> Option<(u32, u32)> {
        self.block(self.kitchen_sink.github_stargazers_and_watchers(self.all.origin())).ok().and_then(|x| x)
    }

    pub fn related_crates(&self) -> Option<&[Origin]> {
        self.related_crates.as_deref()
    }

    async fn make_related_crates(kitchen_sink: &KitchenSink, ver: &RichCrateVersion, downloads_per_month_or_equivalent: Option<usize>) -> Option<Vec<Origin>> {
        // require some level of downloads to avoid recommending spam
        // but limit should be relative to the current crate, so that minor crates
        // get related suggestions too

        let dl = downloads_per_month_or_equivalent.unwrap_or(100);
        let min_recent_downloads = (dl as u32 / 2).min(200);
        kitchen_sink.related_crates(ver, min_recent_downloads).await.map_err(|e| eprintln!("related crates fail: {}", e)).ok()
    }

    /// data for piechart
    pub fn langs_chart(&self, stats: &Stats, width_px: u32) -> Option<LanguageStats> {
        let mut res: Vec<_> = stats.langs.iter().filter(|(lang, lines)| lines.code > 0 && lang.is_code()).map(|(a, b)| (*a, *b)).collect();
        if !res.is_empty() {
            res.sort_by_key(|(_, lines)| lines.code);
            let biggest = res.last().cloned().unwrap();
            let total = res.iter().map(|(_, lines)| lines.code).sum::<u32>();
            if biggest.0 != Language::Rust || biggest.1.code < total * 9 / 10 {
                let mut remaining_px = width_px;
                let mut remaining_lines = total;
                let min_width = 3;
                Some(
                    res.into_iter()
                        .map(|(lang, lines)| {
                            let width = (lines.code * remaining_px / remaining_lines.max(1)).max(min_width);
                            let xpos = width_px - remaining_px;
                            remaining_px = remaining_px.saturating_sub(width);
                            remaining_lines -= lines.code;
                            (lang, lines, (xpos, width))
                        })
                        .rev()
                        .collect(),
                )
            } else {
                None // if crate is 90% Rust, don't bother with stats
            }
        } else {
            None
        }
    }

    pub fn svg_path_for_slice(start: u32, len: u32, total: u32, diameter: u32) -> String {
        fn coords(val: u32, total: u32, radius: f64) -> (f64, f64) {
            ((2. * PI * val as f64 / total as f64).sin() * radius + radius, (PI + 2. * PI * val as f64 / total as f64).cos() * radius + radius)
        }
        let radius = diameter / 2;
        let big_arc = len > total / 2;
        let end = coords(start + len, total, radius as f64);
        let start = coords(start, total, radius as f64);
        format!(
            "M {startx:.2} {starty:.2} A {radius} {radius} 0 {arcflag} 1 {endx:.2} {endy:.2} L {radius} {radius}",
            startx = start.0.round(),
            starty = start.1.round(),
            radius = radius,
            arcflag = if big_arc { "1" } else { "0" },
            endx = end.0,
            endy = end.1,
        )
    }

    /// analyze dependencies checking their weight and their license
    async fn crate_size_and_viral_license(&self, deps: DepInfMap) -> CResult<(CrateSizes, Stats, Option<CrateLicense>)> {
        let mut viral_license: Option<CrateLicense> = None;
        let tmp: Vec<_> = futures::stream::iter(deps)
            .map(|(name, (depinf, semver))| async move {
                if depinf.ty == DepTy::Dev {
                    return None;
                }
                if &*name == "clippy" && !depinf.default {
                    return None; // nobody will enable it
                }

                let krate = match self.get_crate_of_dependency(&name, ()).await {
                    Ok(k) => k,
                    Err(e) => {
                        eprintln!("bad dep not counted: {}", e);
                        return None;
                    },
                };

                let commonality = self.kitchen_sink.index.version_global_popularity(&name, &semver).await.expect("depsstats").unwrap_or(0.);
                let is_heavy_build_dep = match &*name {
                    "bindgen" | "clang-sys" | "cmake" | "cc" if depinf.default => true, // you deserve full weight of it
                    _ => false,
                };

                // if optional, make it look less problematic (indirect - who knows, maybe platform-specific?)
                let weight = if depinf.default {1.} else if depinf.direct {0.25} else {0.15} *
                // if it's common, it's more likelty to be installed anyway,
                // so it's likely to be less costly to add it
                (1. - commonality) *
                // proc-macros are mostly build-time deps
                if krate.is_proc_macro() {0.1} else {1.} *
                // Build deps aren't a big deal [but still include some in case somebody did something awful]
                if depinf.ty == DepTy::Runtime || is_heavy_build_dep {1.} else {0.1};

                // count only default ones
                let weight_minimal = if depinf.default {1.} else {0.} *
                // overestimate commonality
                (1. - commonality).powi(2) *
                if krate.is_proc_macro() {0.} else {1.} *
                if depinf.ty == DepTy::Runtime {1.} else {0.};

                Some((depinf, krate, weight, weight_minimal))
            })
            .buffer_unordered(8)
            .filter_map(|x| async {x})
            .collect::<Vec<_>>().await;

        let mut main_lang_stats = self.ver.language_stats().clone();
        let mut main_crate_size = self.ver.crate_size();
        let mut deps_size_typical = DepsSize::default();
        let mut deps_size_minimal = DepsSize::default();

        tmp.into_iter().for_each(|(depinf, krate, weight, weight_minimal)| {
            if let Some(dep_license) = krate.license() {
                // if the parent crate itself is copyleft
                // then there's no need to raise alerts about copyleft dependencies.
                if viral_license.is_some() || compare_virality(dep_license, self.ver.license()) == Ordering::Greater {
                    let prev_license = viral_license.as_ref().map(|c| &*c.license);
                    let virality = compare_virality(dep_license, prev_license);
                    let existing_is_optional = viral_license.as_ref().map_or(false, |d| d.optional);
                    let new_is_optional = !depinf.default || depinf.ty == DepTy::Build;
                    // Prefer showing non-optional, direct dependency
                    if virality
                        .then(if !new_is_optional && existing_is_optional {Ordering::Greater} else {Ordering::Equal})
                        .then(if depinf.direct {Ordering::Greater} else {Ordering::Equal})
                        == Ordering::Greater {
                        viral_license = Some(CrateLicense {
                            origin: krate.origin().clone(),
                            optional: new_is_optional,
                            license: dep_license.into(),
                        });
                    }
                }
            }

            let crate_size = krate.crate_size();
            let tarball_weighed = (crate_size.0 as f32 * weight) as usize;
            let uncompr_weighed = (crate_size.1 as f32 * weight) as usize;
            let tarball_weighed_minimal = (crate_size.0 as f32 * weight_minimal) as usize;
            let uncompr_weighed_minimal = (crate_size.1 as f32 * weight_minimal) as usize;
            let crate_stats = krate.language_stats();

            if depinf.direct && Self::is_same_project(&self.ver, &krate) {
                main_crate_size.0 += tarball_weighed;
                main_crate_size.1 += uncompr_weighed;

                for (&lang, val) in &crate_stats.langs {
                    let e = main_lang_stats.langs.entry(lang).or_insert_with(Lines::default);
                    e.code += (val.code as f32 * weight) as u32;
                    e.comments += (val.comments as f32 * weight) as u32;
                }
            } else {
                let sloc = crate_stats.langs.iter().filter(|(l, _)| l.is_code()).map(|(_, v)| v.code).sum::<u32>();

                deps_size_typical.tarball += tarball_weighed;
                deps_size_typical.uncompressed += uncompr_weighed;
                deps_size_typical.lines += (sloc as f32 * weight) as usize;
                deps_size_minimal.tarball += tarball_weighed_minimal;
                deps_size_minimal.uncompressed += uncompr_weighed_minimal;
                deps_size_minimal.lines += (sloc as f32 * weight_minimal) as usize;
            }
        });

        Ok((CrateSizes {
            tarball: main_crate_size.0,
            uncompressed: main_crate_size.1,
            minimal: deps_size_minimal,
            typical: deps_size_typical,
        }, main_lang_stats, viral_license))
    }

    async fn get_crate_of_dependency(&self, name: &str, _semver: ()) -> CResult<ArcRichCrateVersion> {
        // FIXME: caching doesn't hold multiple versions, so fetchnig of precise old versions is super expensive
        self.kitchen_sink.rich_crate_version_async(&Origin::from_crates_io_name(name)).await

        // let krate = self.kitchen_sink.index.crate_by_name(&Origin::from_crates_io_name(name))?;
        // let ver = krate.versions()
        //     .iter().rev()
        //     .find(|k| SemVer::parse(k.version()).ok().map_or(false, |v| &v == semver))
        //     .unwrap_or_else(|| krate.latest_version());
        // self.kitchen_sink.rich_crate_version_from_index(ver)
    }

    fn is_same_project(one: &RichCrateVersion, two: &RichCrateVersion) -> bool {
        match (one.repository(), two.repository()) {
            (Some(a), Some(b)) if a.host == b.host => true,
            _ => false,
        }
    }
}

pub struct DepsStatsResult<'a> {
    pub deps: u32,
    pub direct: u32,
    pub name: Option<&'a str>,
    pub former_glory: f64,
}

#[derive(Debug, Copy, Clone, Default)]
pub struct DepsSize {
    pub tarball: usize,
    pub uncompressed: usize,
    pub lines: usize,
}

type LanguageStats = Vec<(Language, Lines, (u32, u32))>;

fn plural(n: u32) -> &'static str {
    if n == 1 { "" } else { "s" }
}

impl ReleaseCounts {
    /// Judge how often the crate makes breaking or stable releases
    pub fn summary(&self) -> (String, Option<String>) {
        // show (n this year|n last year)
        let (n, label, n2, label2, majorinfo) = if self.stable > 0 {
            let breaking = self.major_recent > 2 && self.major * 2 >= self.stable;
            if breaking {
                (self.major, "major breaking", 0, "", false)
            } else {
                (self.stable, "stable", self.major, "major", self.major > 2.max(self.stable / 16))
            }
        } else {
            let very_breaking = self.unstable > 2 && (self.breaking_recent > 3 || self.breaking * 2 >= self.unstable);
            if very_breaking {
                (self.breaking, "breaking", 0, "", false)
            } else {
                let bad = self.breaking_recent > 1.max(self.unstable / 9) && self.breaking > 2.max(self.unstable / 8);
                let good = self.patch * 2 >= self.total;
                (self.unstable, if !bad && good { "" } else { "unstable" }, self.breaking, "breaking", bad)
            }
        };
        if n == self.total || (n > 7 && n * 10 >= self.total * 8) {
            if majorinfo {
                (format!("{} {} release{}", n, label, plural(n)), Some(format!("({} {})", n2, label2)))
            } else {
                (format!("{} {} release{}", n, label, plural(n)), None)
            }
        } else if n * 3 >= self.total * 2 {
            (format!("{} release{}", self.total, plural(self.total)), Some(format!("({})", label)))
        } else {
            (format!("{} release{}", self.total, plural(self.total)), if label != "" { Some(format!("({} {})", n, label)) } else { None })
        }
    }
}

fn is_permissive_license(l: &str) -> bool {
    l.starts_with("MIT") || l.starts_with("Apache") || l.starts_with("BSD") || l.starts_with("Zlib") ||
    l.starts_with("IJG") || l.starts_with("CC0") || l.starts_with("ISC") || l.starts_with("FTL")
}

// FIXME: this is very lousy parsing, but crates-io allows non-SPDX syntax, so I can't use a proper SPDX parser.
fn virality_score(license: &str) -> u8 {
    license.split("AND").filter_map(|l| {
        l.split('/').flat_map(|l| l.split("OR"))
        .filter_map(|l| {
            let l = l.trim_start();
            if is_permissive_license(l) {
                Some(0)
            } else if l.starts_with("AGPL") {
                Some(6)
            } else if l.starts_with("GPL") {
                Some(5)
            } else if l.starts_with("CC-") {
                Some(4)
            } else if l.starts_with("LGPL") {
                Some(2)
            } else if l.starts_with("GFDL") || l.starts_with("MPL") {
                Some(1)
            } else {
                None
            }
        }).min()
    }).max().unwrap_or(0)
}

#[test]
fn test_vir() {
    assert_eq!(6, virality_score("AGPL AND MIT"));
    assert_eq!(0, virality_score("FTL / GPL-2.0"));
    assert_eq!(0, virality_score("AGPL OR MIT"));
    assert_eq!(0, virality_score("MIT/Apache/LGPL"));
    assert_eq!(1, virality_score("MPL/LGPL"));
    assert_eq!(2, virality_score("Apache/MIT AND LGPL"));
    assert_eq!(Ordering::Greater, compare_virality("LGPL", Some("MIT")));
    assert_eq!(Ordering::Greater, compare_virality("AGPL", Some("LGPL")));
    assert_eq!(Ordering::Equal, compare_virality("GPL-3.0", Some("GPL-2.0")));
    assert_eq!(Ordering::Less, compare_virality("CC0-1.0 OR GPL", Some("MPL-2.0")));
}

fn compare_virality(license: &str, other_license: Option<&str>) -> Ordering {
    let score = virality_score(license);
    let other_score = other_license.map(virality_score).unwrap_or(0);
    score.cmp(&other_score)
}

#[test]
fn counts() {
    let r = ReleaseCounts { total: 10, stable: 0, major: 0, major_recent: 0, patch: 0, unstable: 9, breaking: 9, breaking_recent: 0 };
    assert_eq!(r.summary(), ("9 breaking releases".to_string(), None));
    let r = ReleaseCounts { total: 25, stable: 3, major: 0, major_recent: 0, patch: 7, unstable: 2, breaking: 1, breaking_recent: 2 };
    assert_eq!(r.summary(), ("25 releases".to_string(), Some("(3 stable)".to_string())));
    let r = ReleaseCounts { total: 27, stable: 0, major: 1, major_recent: 0, patch: 23, unstable: 13, breaking: 1, breaking_recent: 0 };
    assert_eq!(r.summary(), ("27 releases".to_string(), None));
}

#[test]
fn pie() {
    assert_eq!("M 10.00 5.00 A 5 5 0 0 1 5.00 10.00 L 5 5", CratePage::svg_path_for_slice(1, 1, 4, 10));
    assert_eq!("M 28.00 0.00 A 28 28 0 1 1 28.00 0.00 L 28 28", CratePage::svg_path_for_slice(0, 10, 10, 56));
}
