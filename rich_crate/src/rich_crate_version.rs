use crate::Author;
use crate::Markup;
use crate::Origin;
use crate::Readme;
use cargo_toml::{Dependency, Manifest, Package, Product};
pub use cargo_toml::{DepsSet, Edition, FeatureSet, MaintenanceStatus, TargetDepsSet};
use categories::Categories;
use crates_index::Version;
use repo_url::Repo;
use render_readme::Renderer;
use semver;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use udedokei;

pub use parse_cfg::ParseError as CfgErr;
pub use parse_cfg::{Cfg, Target};

#[derive(Debug, Clone)]
pub struct RichCrateVersion {
    origin: Origin,
    index: Version,
    derived: Derived,
    authors: Vec<Author>,
    readme: Option<Readme>,
    lib_file: Option<String>,
    repo: Option<Repo>,
    path_in_repo: Option<String>,
    has_buildrs: bool,
    has_code_of_conduct: bool,
    has_examples: bool,
    has_tests: bool,
    has_benches: bool,
    has_badges: bool,
    maintenance: MaintenanceStatus,

    // Manifest content
    package: Package,
    lib: Option<Product>,
    bin: Vec<Product>,
    features: FeatureSet,
    target: TargetDepsSet,
    direct_dependencies: DepsSet,
    direct_build_dependencies: DepsSet,
    direct_dev_dependencies: DepsSet,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Include {
    Cleaned,
    AuthoritativeOnly,
    RawCargoTomlOnly,
}

/// Data for a specific version of a crate.
///
/// Crates.rs uses this only for the latest version of a crate.
impl RichCrateVersion {
    pub fn new(index: Version, mut manifest: Manifest, derived: Derived, readme: Option<Readme>,
        lib_file: Option<String>, path_in_repo: Option<String>, has_buildrs: bool, has_code_of_conduct: bool) -> Self
    {
        let package = manifest.package.take().expect("package");
        let mut s = Self {
            origin: Origin::from_crates_io_name(index.name()),
            repo: package.repository.as_ref().and_then(|r| Repo::new(r).ok()),
            authors: package.authors.iter().map(|a| Author::new(a)).collect(),
            index,
            package,
            readme,
            has_buildrs,
            has_code_of_conduct,
            derived,
            path_in_repo,
            lib_file,
            lib: manifest.lib,
            bin: manifest.bin,
            has_examples: !manifest.example.is_empty(),
            has_tests: !manifest.test.is_empty(),
            has_benches: !manifest.bench.is_empty(),
            has_badges: manifest.badges.appveyor.is_some() ||
                manifest.badges.circle_ci.is_some() ||
                manifest.badges.gitlab.is_some() ||
                manifest.badges.travis_ci.is_some() ||
                manifest.badges.codecov.is_some() ||
                manifest.badges.coveralls.is_some(),
            maintenance: manifest.badges.maintenance.status,
            features: manifest.features,
            target: manifest.target,
            direct_dependencies: manifest.dependencies,
            direct_build_dependencies: manifest.build_dependencies,
            direct_dev_dependencies: manifest.dev_dependencies,
        };
        s.override_bad_categories();
        s
    }

    #[inline]
    pub fn homepage(&self) -> Option<&str> {
        self.package.homepage.as_ref().map(|s| s.as_ref())
    }

    pub fn documentation(&self) -> Option<&str> {
        self.package.documentation.as_ref().map(|s| s.as_ref())
    }

    pub fn edition(&self) -> Edition {
        self.package.edition
    }

    pub fn has_own_keywords(&self) -> bool {
        !self.package.keywords.is_empty()
    }

    pub fn has_own_categories(&self) -> bool {
        !self.package.categories.is_empty()
    }

    pub fn has_categories(&self) -> bool {
        !self.package.categories.is_empty() || self.derived.categories.as_ref().map_or(false, |c| !c.is_empty())
    }

    /// Finds preferred capitalization for the name
    pub fn capitalized_name(&self) -> String {
        let mut first_capital = String::with_capacity(self.short_name().len());
        let mut ch = self.short_name().chars();
        if let Some(f) = ch.next() {
            first_capital.extend(f.to_uppercase());
            first_capital.extend(ch.map(|c| if c == '_' {' '} else {c}));
        }

        let mut words = HashMap::with_capacity(100);
        {
            let name = self.short_name().to_lowercase();
            let shouty = self.short_name().to_uppercase();
            let mut add_words = |s: &str| {
                for s in s.split(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_').filter(|&w| w != name && w.eq_ignore_ascii_case(&name)) {
                    let mut points = 2;
                    if name.len() > 2 {
                        if s[1..] != name[1..] {
                            points += 1;
                        }
                        if s != first_capital && s != shouty {
                            points += 1;
                        }
                    }
                    if let Some(count) = words.get_mut(s) {
                        *count += points;
                        continue;
                    }
                    words.insert(s.to_string(), points);
                }
            };
            if let Some(r) = self.readme() {
                let s = Renderer::new(None).visible_text(&r.markup);
                add_words(&s);
            }
            add_words(self.short_name());
            if let Some(s) = self.description() {add_words(s);}
            if let Some(s) = self.alternative_description() {add_words(s);}
            if let Some(s) = &self.derived.github_name {add_words(s);}
            if let Some(s) = self.homepage() {add_words(s);}
            if let Some(s) = self.documentation() {add_words(s);}
            if let Some(r) = self.repository() {add_words(r.url.as_str());}
        }

        if let Some((name, _)) = words.into_iter().max_by_key(|&(_, v)| v) {
            name
        } else {
            first_capital
        }
    }

    pub fn category_slugs(&self, include: Include) -> impl Iterator<Item = Cow<'_, str>> {
        match include {
            Include::Cleaned => Categories::fixed_category_slugs(if let Some(ref assigned_categories) = self.derived.categories {
                &assigned_categories
            } else {
                &self.package.categories
            }),
            Include::AuthoritativeOnly => Categories::fixed_category_slugs(&self.package.categories),
            Include::RawCargoTomlOnly => {
                let tmp: Vec<_> = self.package.categories.iter().map(From::from).collect();
                tmp
            },
        }
        .into_iter()
    }

    pub fn license(&self) -> Option<&str> {
        self.package.license.as_ref().map(|s| s.as_str())
    }

    pub fn license_name(&self) -> Option<&str> {
        self.package.license.as_ref().map(|s| match s.as_str() {
            "" => "(unspecified)",
            "MIT OR Apache-2.0" | "MIT/Apache-2.0" | "MIT / Apache-2.0" => "MIT/Apache",
            "Apache-2.0/ISC/MIT" => "MIT/Apache/ISC",
            "BSD-3-Clause AND Zlib" => "BSD+Zlib",
            "CC0-1.0" => "CC0",
            s => s,
        })
    }

    pub fn license_file(&self) -> Option<&str> {
        self.package.license_file.as_ref().map(|s| s.as_str())
    }

    /// Either original keywords or guessed ones
    pub fn keywords(&self, include: Include) -> impl Iterator<Item = &str> {
        match include {
            Include::RawCargoTomlOnly => &self.package.keywords,
            Include::AuthoritativeOnly => {
                if self.package.keywords.is_empty() { self.derived.github_keywords.as_ref() } else { None }.unwrap_or(&self.package.keywords)
            },
            Include::Cleaned => self.derived.keywords.as_ref().unwrap_or(&self.package.keywords),
        }
        .iter()
        .map(|s| s.as_str())
    }

    /// Globally unique URL-like string identifying source & the crate within that source
    #[inline]
    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    pub fn docs_rs_url(&self) -> Option<String> {
        Some(format!("https://docs.rs/{}/{}/{}", self.short_name(), self.version(), self.short_name()))
    }

    /// Readable name
    #[inline]
    pub fn short_name(&self) -> &str {
        self.index.name()
    }

    /// Without trailing '.' to match website's style
    pub fn description(&self) -> Option<&str> {
        self.package.description.as_ref().map(|d| {
            let d = d.as_str().trim();
            if d.contains(". ") {d} // multiple sentences, leave them alone
            else {d.trim_end_matches('.')}
        })
    }

    /// Currently from github
    pub fn alternative_description(&self) -> Option<&str> {
        self.derived.github_description.as_ref().map(|s| s.as_str())
    }

    /// Only explicitly-specified authors
    pub fn authors(&self) -> &[Author] {
        &self.authors
    }

    #[inline]
    pub fn repository(&self) -> Option<&Repo> {
        self.repo.as_ref()
    }

    pub fn repository_http_url(&self) -> Option<(&Repo, Cow<'_, str>)> {
        self.repository().map(|repo| {
            let relpath = self.path_in_repo.as_ref().map(|s| s.as_str()).unwrap_or("");
            (repo, repo.canonical_http_url(relpath))
        })
    }

    pub fn readme(&self) -> Option<&Readme> {
        self.readme.as_ref()
    }

    /// Contents of the `src/lib.rs` from the crate, if available
    pub fn lib_file(&self) -> Option<&str> {
        self.lib_file.as_ref().map(|s| s.as_str())
    }

    pub fn lib_file_markdown(&self) -> Option<Markup> {
        self.lib_file.as_ref().and_then(|code| {
            let out = extract_doc_comments(code);
            if !out.trim_start().is_empty() {
                Some(Markup::Markdown(out))
            } else {
                None
            }
        })
    }

    pub fn has_buildrs(&self) -> bool {
        self.has_buildrs || self.package.build.is_some()
    }

    pub fn has_code_of_conduct(&self) -> bool {
        self.has_code_of_conduct
    }

    pub fn has_examples(&self) -> bool {
        self.has_examples
    }

    pub fn has_tests(&self) -> bool {
        self.has_tests
    }

    pub fn has_benches(&self) -> bool {
        self.has_benches
    }

    pub fn has_badges(&self) -> bool {
        self.has_badges
    }

    pub fn maintenance(&self) -> MaintenanceStatus {
        self.maintenance
    }

    pub fn links(&self) -> Option<&str> {
        self.package.links.as_ref().map(|s| s.as_str())
    }

    #[inline]
    pub fn version(&self) -> &str {
        self.index.version()
    }

    pub fn version_semver(&self) -> Result<semver::Version, semver::SemVerError> {
        semver::Version::parse(self.version())
    }

    pub fn is_yanked(&self) -> bool {
        self.index.is_yanked()
    }

    pub fn has_lib(&self) -> bool {
        !self.is_proc_macro() && (self.lib_file.is_some() || self.lib.is_some())
    }

    pub fn has_bin(&self) -> bool {
        !self.bin.is_empty()
    }

    // has cargo-prefixed bin
    pub fn has_cargo_bin(&self) -> bool {
        // we get binaries normalized, so no need to check for package name
        self.bin.iter().any(|b| b.name.as_ref().map_or(false, |n| n.starts_with("cargo-")))
    }

    pub fn is_proc_macro(&self) -> bool {
        self.lib.as_ref().map_or(false, |lib| lib.proc_macro)
    }

    pub fn is_app(&self) -> bool {
        self.has_bin() && !self.is_proc_macro() && !self.has_lib()
    }

    /// Does it use nightly-only features
    pub fn is_nightly(&self) -> bool {
        self.derived.is_nightly
    }

    pub fn is_no_std(&self) -> bool {
        self.category_slugs(Include::RawCargoTomlOnly).any(|c| c == "no-std")
            || self.keywords(Include::RawCargoTomlOnly).any(|k| k == "no-std" || k == "no_std")
            || self.features().iter().any(|(k,_)| k == "no-std" || k == "no_std")
    }

    pub fn is_sys(&self) -> bool {
        !self.has_bin() &&
            self.has_buildrs() &&
            !self.is_proc_macro() &&
            (self.links().is_some() ||
                (
                    self.short_name().ends_with("-sys") ||
                        self.short_name().ends_with("_sys") ||
                        self.category_slugs(Include::RawCargoTomlOnly).any(|c| c == "external-ffi-bindings")
                    // _dll suffix is a false positive
                ))
    }

    pub fn has_runtime_deps(&self) -> bool {
        !self.direct_dependencies.is_empty() || self.target.values().any(|target| !target.dependencies.is_empty())
    }

    pub fn features(&self) -> &BTreeMap<String, Vec<String>> {
        &self.features
    }

    /// Runtime, dev, build
    pub fn direct_dependencies(&self) -> Result<(Vec<RichDep>, Vec<RichDep>, Vec<RichDep>), CfgErr> {
        fn to_dep((name, dep): (&String, &Dependency)) -> (String, RichDep) {
            let package = dep.package().unwrap_or(&name).to_owned();
            (package.clone(), RichDep {
                package,
                dep: dep.clone(),
                only_for_features: Vec::new(),
                only_for_targets: Vec::new(),
                with_features: Vec::new(),
            })
        }
        let mut normal: BTreeMap<String, RichDep> = self.direct_dependencies.iter().map(to_dep).collect();
        let mut build: BTreeMap<String, RichDep> = self.direct_build_dependencies.iter().map(to_dep).collect();
        let mut dev: BTreeMap<String, RichDep> = self.direct_dev_dependencies.iter().map(to_dep).collect();

        fn add_targets(dest: &mut BTreeMap<String, RichDep>, src: &DepsSet, target: &str) -> Result<(), CfgErr> {
            for (name, dep) in src {
                use std::collections::btree_map::Entry::*;
                let package = dep.package().unwrap_or(&name);
                match dest.entry(package.to_string()) {
                    Vacant(e) => {
                        e.insert(RichDep {
                            package: package.to_string(),
                            dep: dep.clone(),
                            only_for_targets: vec![target.parse()?],
                            only_for_features: Vec::new(),
                            with_features: Vec::new(),
                        });
                    },
                    _ => {}, // don't add platform info to existing cross-platform deps
                }
            }
            Ok(())
        }
        for (ref target, ref plat) in &self.target {
            add_targets(&mut normal, &plat.dependencies, target)?;
            add_targets(&mut build, &plat.build_dependencies, target)?;
            add_targets(&mut dev, &plat.dev_dependencies, target)?;
        }

        // Don't display deps twice if they're required anyway
        for dep in normal.keys() {
            dev.remove(dep);
            build.remove(dep);
        }
        for dep in build.keys() {
            dev.remove(dep);
        }

        let default_features = self.features().get("default")
            .map(|d| d.iter().collect::<HashSet<_>>())
            .unwrap_or_default();

        for (for_feature, wants) in self.features().into_iter().filter(|(n,_)| *n != "default") {
            for depstr in wants {
                let mut depstr = depstr.splitn(2, '/');
                let name = depstr.next().expect("name should be there");
                let with_feature = depstr.next();

                if let Some(dep) = normal.get_mut(name) {
                    let enabled = default_features.get(for_feature).is_some();
                    if enabled {
                        if let Some(with_feature) = with_feature {
                            if !dep.dep.req_features().iter().any(|f| f == with_feature)
                                && !dep.with_features.iter().any(|f| f == with_feature)
                            {
                                dep.with_features.push(with_feature.to_string())
                            }
                        }
                    }
                    dep.only_for_features.push((for_feature.to_string(), enabled));
                }
            }
        }
        fn convsort(dep: BTreeMap<String, RichDep>) -> Vec<RichDep> {
            let mut dep: Vec<_> = dep.into_iter().map(|(_, dep)| dep).collect();
            dep.sort_by(|a,b| {
                a.dep.optional().cmp(&b.dep.optional())
                .then(a.only_for_targets.is_empty().cmp(&b.only_for_targets.is_empty()))
                .then(a.package.cmp(&b.package))
            });
            dep
        }
        Ok((convsort(normal), convsort(dev), convsort(build)))
    }

    fn override_bad_categories(&mut self) {
        for cat in &mut self.package.categories {
            if cat == "localization" {
                // nobody knows the difference
                *cat = "internationalization".to_string();
            }
            if cat == "parsers" {
                if self.direct_dependencies.keys().any(|k| k == "nom" || k == "peresil" || k == "combine") ||
                    self.package.keywords.iter().any(|k| match k.to_ascii_lowercase().as_ref() {
                        "asn1" | "tls" | "idl" | "crawler" | "xml" | "nom" | "json" | "logs" | "elf" | "uri" | "html" | "protocol" | "semver" | "ecma" |
                        "chess" | "vcard" | "exe" | "fasta" => true,
                        _ => false,
                    })
                {
                    *cat = "parser-implementations".into();
                }
            }
            if cat == "cryptography" || cat == "database" || cat == "rust-patterns" || cat == "development-tools" {
                if self.package.keywords.iter().any(|k| k == "bitcoin" || k == "ethereum" || k == "ledger" || k == "exonum" || k == "blockchain") {
                    *cat = "cryptography::cryptocurrencies".into();
                }
            }
            if cat == "games" {
                if self.package.keywords.iter().any(|k| {
                    k == "game-dev" || k == "game-development" || k == "gamedev" || k == "framework" || k == "utilities" || k == "parser" || k == "api"
                }) {
                    *cat = "game-engines".into();
                }
            }
            if cat == "science" {
                if self.package.keywords.iter().any(|k| k == "neural-network" || k == "machine-learning" || k == "deep-learning") {
                    *cat = "science::ml".into();
                } else if self.package.keywords.iter().any(|k| {
                    k == "math" || k == "calculus" || k == "algebra" || k == "linear-algebra" || k == "mathematics" || k == "maths" || k == "number-theory"
                }) {
                    *cat = "science::math".into();
                }
            }
        }
    }

    pub fn language_stats(&self) -> &udedokei::Stats {
        &self.derived.language_stats
    }

    /// compressed (whole tarball) and decompressed (extracted files only)
    #[inline]
    pub fn crate_size(&self) -> (usize, usize) {
        (self.derived.crate_compressed_size as usize, self.derived.crate_decompressed_size as usize)
    }
}

pub struct RichDep {
    pub package: String,
    pub dep: Dependency,
    /// it's optional, used only for a platform
    pub only_for_targets: Vec<Target>,
    /// it's optional, used only if parent crate's feature is enabled
    pub only_for_features: Vec<(String, bool)>,
    /// When used, these features of this dependency are enabled
    pub with_features: Vec<String>,
}

impl RichDep {
    pub fn for_features(&self) -> &[(String, bool)] {
        &self.only_for_features
    }

    pub fn is_optional(&self) -> bool {
        !self.only_for_features.is_empty()
    }

    pub fn add_target(&mut self, target: &str) -> Result<(), CfgErr> {
        self.only_for_targets.push(
            target.parse().unwrap_or_else(|_| {
                Target::Cfg(Cfg::Equal("target".to_string(), target.to_string()))
            }),
        );
        Ok(())
    }
}

/// Metadata guessed
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Derived {
    pub categories: Option<Vec<String>>,
    pub keywords: Option<Vec<String>>,
    pub github_keywords: Option<Vec<String>>,
    pub github_name: Option<String>,
    pub github_description: Option<String>,
    pub language_stats: udedokei::Stats,
    pub crate_compressed_size: u32,
    pub crate_decompressed_size: u32,
    pub is_nightly: bool,
}

fn extract_doc_comments(code: &str) -> String {
    let mut out = String::with_capacity(code.len() / 2);
    let mut is_in_block_mode = false;
    for l in code.lines() {
        let l = l.trim_start();
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
