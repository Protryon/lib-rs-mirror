mod scorer;
pub use self::scorer::*;
use cargo_toml::MaintenanceStatus;
use chrono::prelude::*;
use render_readme::Handle;
use render_readme::NodeData;
use rich_crate::Author;
use rich_crate::CrateOwner;
use rich_crate::CrateVersion;
use rich_crate::Edition;
use semver::Version as SemVer;

/// Only changes when a new version is released
pub struct CrateVersionInputs<'a> {
    pub versions: &'a [CrateVersion],
    pub description: &'a str,
    pub readme: Option<&'a Handle>,
    pub owners: &'a [CrateOwner],
    pub authors: &'a [Author],
    pub contributors: Option<u32>, // based on source history
    pub edition: Edition,
    pub is_app: bool,
    pub has_build_rs: bool,
    pub has_code_of_conduct: bool,
    pub has_links: bool,
    pub has_documentation_link: bool,
    pub has_homepage_link: bool,
    pub has_repository_link: bool,
    pub has_verified_repository_link: bool,
    pub has_keywords: bool,
    pub has_own_categories: bool,
    pub has_features: bool,
    pub has_examples: bool,
    pub has_benches: bool,
    pub has_tests: bool,
    // pub has_lockfile: bool,
    // pub has_changelog: bool,
    pub license: &'a str,
    pub has_badges: bool,
    pub maintenance: MaintenanceStatus,
    pub is_nightly: bool,

    pub total_code_lines: u32,
    pub rust_comment_lines: u32,
    pub rust_code_lines: u32,
    // (relative) weight of dependencies?

    // rust loc
    // #[test] cases
    // assert! calls
    // comments ratio (normalized to project size)

    // look for deprecated in the description
}

/// Changes over time, but doesn't depend on crate's own ranking
pub struct CrateTemporalInputs<'a> {
    pub versions: &'a [CrateVersion],
    // 1.0 fresh, 0.0 totally outdated and deprecated
    pub dependency_freshness: Vec<f32>,
    pub downloads_per_month: u32,
    /// Looking at downloads of direct dependencies.
    /// This way internal derive/impl/core crates that have one big user get 0 here.
    pub downloads_per_month_minus_most_downloaded_user: u32,
    pub is_app: bool,
    pub has_docs_rs: bool,
    pub is_nightly: bool,

    // low priority, because it's unranked! it'll be re-evaluated later
    pub number_of_direct_reverse_deps: u32,
    /// use max(runtime, dev, build), because the crate is going to be one of these kinds
    pub number_of_indirect_reverse_deps: u32,
    /// Includes non-optional (i.e. it's the upper bound, not just the optional ones)
    pub number_of_indirect_reverse_optional_deps: u32,

    /// Whether Debian has packaged this crate
    pub is_in_debian: bool,
    // most recent commit
    // avg time issues are left unanswered?
    // pub crate_score_context_free: f64,
    // pub owner_pageranks: Vec<f32>,
    // pub reverse_deps_rankings: Vec<f32>,
}

pub struct Env {
    pub max_recent_downloads: u32,
    pub max_crates: u32,
}

fn cargo_toml_score(cr: &CrateVersionInputs<'_>) -> Score {
    let mut s = Score::new();

    s.frac("description len", 30, (cr.description.len() as f64 / 300.).min(1.));

    // build.rs slows compilation down, so better not use it unless necessary (links means a sys create, so somewhat justified)
    s.n("build.rs", 10, if !cr.has_build_rs && !cr.has_links {10} else if cr.has_links {5} else {0});

    // users report examples are super valuable
    s.has("has_examples", 50, cr.has_examples);
    // probably less buggy than if winging it
    s.has("has_tests", 50, cr.has_tests);
    s.has("has_code_of_conduct", 15, cr.has_code_of_conduct);
    // probably optimized
    s.has("has_benches", 10, cr.has_benches);

    // docs are very important (TODO: this may be redundant with docs.rs)
    s.has("has_documentation_link", 30, cr.has_documentation_link);
    s.has("has_homepage_link", 30, cr.has_homepage_link);

    // we care about being able to analyze
    s.has("has_repository_link", 8, cr.has_repository_link);
    s.has("has_verified_repository_link", 10, cr.has_verified_repository_link);

    // helps lib.rs show crate in the right place
    s.has("has_keywords", 10, cr.has_keywords);
    s.has("has_categories", 5, cr.has_own_categories);

    // probably non-trivial crate
    s.has("has_features", 5, cr.has_features);

    // it's the best practice, may help building old versions of the project
    // s.has("has_lockfile", 5, cr.has_lockfile);
    // assume it's CI, which helps improve quality
    // TODO: detect travis, etc. without badges, since they're deprecated now
    s.has("has_badges", 5, cr.has_badges);

    s.n("maintenance status", 30, match cr.maintenance {
        MaintenanceStatus::ActivelyDeveloped => 30,
        MaintenanceStatus::Experimental => 25,
        MaintenanceStatus::None => 20,
        MaintenanceStatus::PassivelyMaintained => 10,
        MaintenanceStatus::AsIs => 5,
        MaintenanceStatus::LookingForMaintainer => 4,
        MaintenanceStatus::Deprecated => 0,
    });

    // TODO: being nightly should be a negative score
    s.has("works on stable", 20, !cr.is_nightly);
    // fresh
    s.has("2018 edition", 5, cr.edition != Edition::E2015);

    // license proliferation is bad
    s.has("useful license", 10, if cr.is_app {
        // for end-user apps assume user freedom > developer freedom
        cr.license.contains("GPL") || cr.license.contains("CC-BY-SA") || cr.license.contains("MPL")
    } else {
        // for libs assume developer freedom > user freedom
        cr.license.contains("MIT") || cr.license.contains("BSD") ||
        cr.license.contains("Apache") || cr.license.contains("CC0") ||
        cr.license.contains("IJG") || cr.license.contains("Zlib")
    });

    s
}

#[derive(Default)]
struct MarkupProps {
    text_len: usize,
    code_len: usize,
    list_or_table_rows: u16,
    images: u16,
    pre_blocks: u16,
    sections: u16,
}

fn fill_props(node: &Handle, props: &mut MarkupProps, mut in_code: bool) {
    match node.data {
        NodeData::Text { ref contents } => {
            let len = contents.borrow().trim().len();
            if len > 0 {
                if in_code {
                    props.code_len += len + 1; // +1 to account for separators that were trimmed
                } else {
                    props.text_len += len + 1;
                }
            }
            return; // has no children
        },
        NodeData::Element { ref name, ref attrs, .. } => {
            match name.local.get(..).unwrap() {
                "img" => {
                    if let Some(src) = attrs.borrow().iter().find(|a| a.name.local.get(..).unwrap() == "src") {
                        if render_readme::is_badge_url(&src.value) {
                            return; // don't count badges
                        }
                    }
                    props.images += 1;
                    return;
                },
                "li" | "tr" => props.list_or_table_rows += 1,
                "a" => {
                    if let Some(href) = attrs.borrow().iter().find(|a| a.name.local.get(..).unwrap() == "href") {
                        if render_readme::is_badge_url(&href.value) {
                            return; // don't count badge image children
                        }
                    }
                },
                "pre" => {
                    in_code = true;
                    props.pre_blocks += 1;
                },
                "code" => in_code = true,
                "h1" | "h2" | "h3" | "h4" | "h5" => props.sections += 1,
                _ => {},
            }
        },
        _ => {},
    }
    for child in node.children.borrow().iter() {
        fill_props(child, props, in_code);
    }
}

fn readme_score(readme: Option<&Handle>, is_app: bool) -> Score {
    let mut s = Score::new();
    let mut props = Default::default();
    if let Some(readme) = readme {
        fill_props(readme, &mut props, false);
    }
    s.frac("text length", 75, (props.text_len as f64 / 3000.).min(1.0));
    // code examples are not expected for apps
    s.frac("code length", if is_app { 25 } else { 100 }, (props.code_len as f64 / 2000.).min(1.0));
    s.n("code blocks", if is_app { 15 } else { 25 }, props.pre_blocks * 5);
    s.has("has code", if is_app { 10 } else { 30 }, props.code_len > 150 && props.pre_blocks > 0); // people really like seeing a code example
    s.n("images", 35, props.images * 25); // I like pages with logos
    s.n("sections", 30, props.sections * 4);
    s.n("list or table rows", 25, props.list_or_table_rows * 2);
    s
}

fn versions_score(ver: &[CrateVersion]) -> Score {
    let mut s = Score::new();
    let semver = ver.iter().filter(|s| !s.yanked).filter_map(|s| SemVer::parse(&s.num).ok()).collect::<Vec<_>>();
    s.has("more than one release", 20, semver.len() > 1);

    if semver.is_empty() { // all yanked
        return s;
    }

    let oldest = ver.iter().map(|v| &v.created_at).min().and_then(|s| s.parse::<DateTime<Utc>>().ok());
    let newest = ver.iter().map(|v| &v.created_at).max().and_then(|s| s.parse::<DateTime<Utc>>().ok());
    if let (Some(oldest), Some(newest)) = (oldest, newest) {
        s.n("development history", 40, (newest - oldest).num_days() / 11);

        s.has("not ancient", 10, newest.year() > 2016); // such old Rust crates are all suspicious
    }

    // don't count 0.0.x
    s.n("number of non-experimental releases", 15, semver.iter().filter(|v| (v.major > 0 || v.minor > 0) && v.pre.is_empty()).count() as u32);

    // patch releases are correlated with stable, polished code
    s.n("patch releases", 20, 4 * semver.iter().filter(|v| v.major > 0 && v.patch > 0).count() as u32);
    s.n("a high patch release", 10, semver.iter().map(|v| v.patch as u32).max().unwrap_or(0));
    // for 0.x crates it's hard to knwo what is a patch release
    s.has("an unstable patch/feature release", 8, semver.iter().any(|v| v.major == 0 && v.patch > 1));
    // careful release process is a sign of maturity
    s.has("a prerelease", 5, semver.iter().any(|v| !v.pre.is_empty()));
    s.has("a stable release", 10, semver.iter().any(|v| v.major > 0 && v.major < 20));
    s.has("yanked", 2, ver.iter().any(|v| v.yanked)); // author cares to remove bad versions
    s
}

fn authors_score(authors: &[Author], owners: &[CrateOwner], contributors: Option<u32>) -> Score {
    let mut s = Score::new();
    s.n("more than one owner", 3, owners.len() > 1);
    s.n("bus factor", 4, owners.len() as u32);
    s.n("authors", 5, authors.len() as u32);
    if let Some(contributors) = contributors {
        s.frac("contributors", 7, (contributors as f64 / 7.).min(1.));
    }
    s
}

fn code_score(cr: &CrateVersionInputs<'_>) -> Score {
    let mut s = Score::new();
    s.has("Non-trivial", 1, cr.total_code_lines > 700); // usually trivial/toy programs
    s.has("Non-giant", 1, cr.total_code_lines < 80000); // these should be split into crates
    s.frac("Rust LoC", 2, (cr.rust_code_lines as f64 / 5000.).min(1.)); // prefer substantial projects (and ignore vendored non-Rust code)
    s.frac("Comments", 2, (10. * cr.rust_comment_lines as f64 / (3000. + cr.rust_code_lines as f64)).min(1.)); // it's easier to keep small project commented
    s
}

pub fn crate_score_version(cr: &CrateVersionInputs<'_>) -> Score {
    let mut score = Score::new();

    score.group("Cargo.toml", 2, cargo_toml_score(cr));
    score.group("README", 5, readme_score(cr.readme, cr.is_app));
    score.group("Code", 2, code_score(cr));
    score.group("Versions", 5, versions_score(cr.versions));
    score.group("Authors/Owners", 3, authors_score(cr.authors, cr.owners, cr.contributors));

    score
}

pub fn crate_score_temporal(cr: &CrateTemporalInputs<'_>) -> Score {
    let mut score = Score::new();
    // if it's bin+lib, treat it as a lib.
    let is_app_only = cr.is_app && cr.number_of_direct_reverse_deps == 0;

    let newest = cr.versions.iter().max_by_key(|v| &v.created_at).expect("at least 1 ver?");
    let freshness_score = match newest.created_at.parse::<DateTime<Utc>>() {
        Ok(latest_date) => {
            // Assume higher versions, and especially patch versions, mean the crate is more mature
            // and needs fewer updates
            let version_stability_interval = match SemVer::parse(&newest.num) {
                Ok(ref ver) if ver.patch > 3 && ver.major > 0 => 700,
                Ok(ref ver) if ver.patch > 3 => 450,
                Ok(ref ver) if ver.patch > 0 => 300,
                Ok(ref ver) if ver.major > 0 => 200,
                Ok(ref ver) if ver.minor > 3 => 140,
                _ => 80,
            };
            let expected_update_interval = version_stability_interval.min(cr.versions.len() as i64 * 50) / if cr.is_nightly { 4 } else { 1 };
            let age = (Utc::now() - latest_date).num_days();
            let days_past_expiration_date = (age - expected_update_interval).max(0);
            // score decays for a ~year after the crate should have been updated
            let decay_days = expected_update_interval/2 + if cr.is_nightly { 30 } else if is_app_only {300} else {200};
            (decay_days - days_past_expiration_date).max(0) as f64 / (decay_days as f64)
        },
        Err(e) => {
            eprintln!("Release time parse error: {}", e);
            0.
        },
    };
    score.frac("Freshness of latest release", 10, freshness_score);
    score.frac("Freshness of deps", 10, cr.dependency_freshness.iter()
        .map(|d| 0.2 + d * 0.8) // one bad dep shouldn't totally kill the score
        .product::<f32>());

    // Low numbers are just bots/noise.
    let downloads = (cr.downloads_per_month as f64 - 100.).max(0.) + 100.;
    let downloads_cleaned = (cr.downloads_per_month_minus_most_downloaded_user as f64 / if is_app_only { 1. } else { 2. } - 50.).max(0.) + 50.;
    // distribution of downloads follows power law.
    // apps have much harder to get high download numbers.
    let pop = (downloads.log2() - 6.6) / (if is_app_only { 5. } else { 6. });
    let pop_cleaned = downloads_cleaned.log2() - 5.6;
    assert!(pop > 0.);
    assert!(pop_cleaned > 0.);
    // FIXME: max should be based on the most downloaded crate?
    score.score_f("Downloads", 5., pop);
    score.score_f("Downloads (cleaned)", 18., pop_cleaned);

    // if it's new, it doesn't have to have many downloads.
    // if it's aging, it'd better have more users
    score.has("Any traction", 2, (cr.downloads_per_month as f64 * freshness_score) > 1000.);

    // Score added in an unusual way, because not being in Debian is not neccessarily a bad thing
    // (e.g. a crate may be for Windows or Mac only)
    if cr.is_in_debian {
        score.has("Debian endorsement", 2, true);
    }

    // Don't expect apps to have rev deps (omitting these entirely proprtionally increases importance of other factors)
    if !is_app_only {
        score.score_f("Direct rev deps", 10., (cr.number_of_direct_reverse_deps as f64).sqrt());
        let indirect = 1. + cr.number_of_indirect_reverse_optional_deps as f64 / 4.;
        score.score_f("Indirect rev deps", 6., indirect.log2());
    }

    if !is_app_only || cr.has_docs_rs {
        score.has("docs.rs", 1, cr.has_docs_rs);
    }
    score
}

pub struct OverallScoreInputs {
    // ratio of peak active users to current active users (1.0 = everyone active, 0.0 = all dead)
    pub former_glory: Option<f64>,
    pub is_proc_macro: bool,
    pub is_sys: bool,
    pub is_sub_component: bool,
    pub is_autopublished: bool,
    pub is_deprecated: bool,
    pub is_crates_io_published: bool,
    pub is_yanked: bool,
    pub is_squatspam: bool,
    pub is_unwanted_category: bool,
}

pub fn combined_score(base_score: Score, temp_score: Score, f: &OverallScoreInputs) -> f64 {
    let mut score = (base_score.total() + temp_score.total()) * 0.5;

    if let Some(former_glory) = f.former_glory {
        score *= former_glory;
    }

    // there's usually a non-macro/non-sys sibling
    if f.is_proc_macro || f.is_sys {
        score *= 0.9;
    }

    if f.is_sub_component  {
        score *= 0.9;
    }

    if f.is_autopublished {
        score *= 0.8;
    }

    if f.is_deprecated {
        score *= 0.2;
    }

    if f.is_unwanted_category {
        score *= 0.5;
    }

    if !f.is_crates_io_published {
        // installation and usage of other crate sources is more limited
        score *= 0.75;
    }

    // k bye
    if f.is_yanked || f.is_squatspam {
        score *= 0.001;
    }

    score
}

#[test]
fn test_readme_score() {
    let ren = render_readme::Renderer::new(None);
    let dom = ren.page_node(&render_readme::Markup::Markdown("# hello world [link](http://hrefval)
![img](imgsrc)
![badg](http://travis-ci.org/badge.svg)

```
code
```

* list
* items
".into()), None, render_readme::Links::Ugc, None);
    let mut p = Default::default();
    fill_props(&dom, &mut p, false);
    assert_eq!(p.images, 1);
    assert_eq!(p.sections, 1);
    assert_eq!(p.list_or_table_rows, 2);
    assert_eq!(p.pre_blocks, 1);
    assert_eq!(p.code_len, 5);
    assert_eq!(p.text_len, 28);
}
