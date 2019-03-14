mod scorer;
use render_readme::Handle;
use render_readme::NodeData;
use rich_crate::Edition;
use rich_crate::Author;
use rich_crate::CrateVersion;
use cargo_toml::MaintenanceStatus;
use rich_crate::CrateOwner;
use semver::Version as SemVer;
pub use self::scorer::*;
use chrono::prelude::*;

/// Only changes when a new version is released
pub struct CrateVersionInputs<'a> {
    pub versions: &'a [CrateVersion],
    pub description: &'a str,
    pub readme: Option<&'a Handle>,
    pub owners: &'a [CrateOwner],
    pub authors: &'a [Author],
    pub edition: Edition,
    pub is_app: bool,
    pub has_build_rs: bool,
    pub has_links: bool,
    pub has_documentation_link: bool,
    pub has_homepage_link: bool,
    pub has_repository_link: bool,
    pub has_keywords: bool,
    pub has_categories: bool,
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

    // (relative) weight of dependencies?

    // rust loc
    // #[test] cases
    // assert! calls
    // comments ratio (normalized to project size)

    // look for deprecated in the description
}

/// Changes over time, but doesn't depend on crate's own ranking
pub struct CrateTemporalInputs {
    /// 1.0 fresh, 0.0 totally outdated and deprecated
    pub dependency_freshness: Vec<f32>,
    pub recent_downloads: u32,
    pub recent_downloads_minus_most_downloaded_user: u32,
    pub has_docs_rs: bool,

    // low priority, because it's unranked! it'll be re-evaluated later
    pub number_of_reverse_deps: u32,

    // most recent commit
    // avg time issues are left unanswered?
}

/// Crate's own base ranking influences these rankings
pub struct CrateContextInputs {
    pub crate_score_context_free: f64,
    pub owner_pageranks: Vec<f32>,
    pub reverse_deps_rankings: Vec<f32>,
}

pub struct Env {
    pub max_recent_downloads: u32,
    pub max_crates: u32,
}

fn cargo_toml_score(cr: &CrateVersionInputs) -> Score {
    let mut s = Score::new();

    s.frac("description len", 20, (cr.description.len() as f64 / 300.).min(1.));

    // build.rs slows compilation down, so better not use it unless necessary (links means a sys create, so somewhat justified)
    s.n("build.rs", 10, if !cr.has_build_rs && !cr.has_links {10} else if cr.has_links {5} else {0});

    // users report examples are super valuable
    s.has("has_examples", 50, cr.has_examples);
    // probably less buggy than if winging it
    s.has("has_tests", 50, cr.has_tests);
    // probably optimized
    s.has("has_benches", 10, cr.has_benches);

    // docs are very important (TODO: this may be redundant with docs.rs)
    s.has("has_documentation_link", 30, cr.has_documentation_link);
    s.has("has_homepage_link", 30, cr.has_homepage_link);

    // we care about being able to analyze
    s.has("has_repository_link", 20, cr.has_repository_link);

    // helps crates.rs show crate in the right place
    s.has("has_keywords", 10, cr.has_keywords);
    s.has("has_categories", 5, cr.has_categories);

    // probably non-trivial crate
    s.has("has_features", 5, cr.has_features);

    // it's the best practice, may help building old versions of the project
    // s.has("has_lockfile", 5, cr.has_lockfile);
    // assume it's CI, which helps improve quality
    s.has("has_badges", 20, cr.has_badges);

    // not official
    // s.has("has_changelog", 5, cr.has_changelog);

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
        cr.license.contains("MIT") || cr.license.contains("BSD") || cr.license.contains("Apache") || cr.license.contains("CC0")
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

fn is_badge_url(url: &str) -> bool {
    let url = url.trim_start_matches("http://").trim_start_matches("https://")
        .trim_start_matches("www.")
        .trim_start_matches("flat.")
        .trim_start_matches("images.")
        .trim_start_matches("img.")
        .trim_start_matches("api.")
        .trim_start_matches("ci.")
        .trim_start_matches("build.");
    url.starts_with("appveyor.com") ||
    url.starts_with("badge.") ||
    url.starts_with("badgen.") ||
    url.starts_with("badges.") ||
    url.starts_with("codecov.io") ||
    url.starts_with("coveralls.io") ||
    url.starts_with("docs.rs") ||
    url.starts_with("gitlab.com") ||
    url.starts_with("isitmaintained.com") ||
    url.starts_with("meritbadge") ||
    url.starts_with("microbadger") ||
    url.starts_with("ohloh.net") ||
    url.starts_with("openhub.net") ||
    url.starts_with("repostatus.org") ||
    url.starts_with("shields.io") ||
    url.starts_with("snapcraft.io") ||
    url.starts_with("spearow.io") ||
    url.starts_with("travis-ci.") ||
    url.starts_with("zenodo.org") ||
    url.ends_with("?branch=master") ||
    url.ends_with("/pipeline.svg") ||
    url.ends_with("/coverage.svg") ||
    url.ends_with("/build.svg") ||
    url.ends_with("badge.svg") ||
    url.ends_with("badge.png")
}

fn fill_props(node: &Handle, props: &mut MarkupProps, mut in_code: bool) {
    match node.data {
        NodeData::Text {ref contents} => {
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
        NodeData::Element {ref name, ref attrs, ..} => {
            match name.local.get(..).unwrap() {
                "img" => {
                    if let Some(src) = attrs.borrow().iter().find(|a| a.name.local.get(..).unwrap() == "src") {
                        if is_badge_url(&src.value) {
                            return; // don't count badges
                        }
                    }
                    props.images += 1;
                    return;
                },
                "li" | "tr" => props.list_or_table_rows += 1,
                "a" => {
                    if let Some(href) = attrs.borrow().iter().find(|a| a.name.local.get(..).unwrap() == "href") {
                        if is_badge_url(&href.value) {
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

fn readme_score(readme: Option<&Handle>) -> Score {
    let mut s = Score::new();
    let mut props = Default::default();
    if let Some(readme) = readme {
        fill_props(readme, &mut props, false);
    }
    s.frac("text length", 75, (props.text_len as f64 /3000.).min(1.0));
    s.frac("code length", 100, (props.code_len as f64 /2000.).min(1.0));
    s.has("has code", 30, props.code_len > 150 && props.pre_blocks > 0); // people really like seeing a code example
    s.n("code blocks", 25, props.pre_blocks * 5);
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

fn authors_score(authors: &[Author], owners: &[CrateOwner]) -> Score {
    let mut s = Score::new();
    s.n("bus factor", 5, owners.len() as u32);
    s.n("more than one owner", 8, owners.len() > 1);
    s.n("authors", 5, authors.len() as u32);
    s
}

pub fn crate_score_version(cr: &CrateVersionInputs) -> Score {
    let mut score = Score::new();

    score.group("Cargo.toml", 2, cargo_toml_score(cr));
    score.group("README", 4, readme_score(cr.readme));
    score.group("Versions", 4, versions_score(cr.versions));
    score.group("Authors/Owners", 3, authors_score(cr.authors, cr.owners));

    score
}

// pub fn crate_score_temporal(inputs: &CrateTemporalInputs) -> Score {
//     let mut score = Score::new();

//     score
// }

// pub fn crate_score_contextual(inputs: &CrateContextInputs) -> Score {
//     let mut score = Score::new();

//     score
// }

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
".into()), None, false);
    let mut p = Default::default();
    fill_props(&dom, &mut p, false);
    assert_eq!(p.images, 1);
    assert_eq!(p.sections, 1);
    assert_eq!(p.list_or_table_rows, 2);
    assert_eq!(p.pre_blocks, 1);
    assert_eq!(p.code_len, 5);
    assert_eq!(p.text_len, 28);
}
