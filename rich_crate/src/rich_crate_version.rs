use crate::Author;
use crate::Markup;
use crate::Origin;
use crate::Readme;
use cargo_toml::{Dependency, Manifest, Package, OptionalFile};
pub use cargo_toml::{DepsSet, Edition, Resolver, FeatureSet, MaintenanceStatus, TargetDepsSet};
use repo_url::Repo;
use std::borrow::Cow;
use std::collections::BTreeMap;
use ahash::HashSet;

pub use parse_cfg::{Cfg, Target};

#[derive(Debug, Clone)]
pub struct RichCrateVersion {
    origin: Origin,
    derived: Derived,
    authors: Vec<Author>,
    repo: Option<Repo>,

    // Manifest content
    manifest: Manifest,
}

/// Data for a specific version of a crate.
///
/// Crates.rs uses this only for the latest version of a crate.
impl RichCrateVersion {
    pub fn new(origin: Origin, manifest: Manifest, derived: Derived) -> Self {
        let package = manifest.package();
        if let Origin::GitHub { .. } = &origin {
            assert!(package.repository().is_some());
        }
        let s = Self {
            origin,
            repo: package.repository().and_then(|r| Repo::new(r).ok()),
            authors: match package.authors() {
                [one] => one.split(',').map(Author::new).collect(), // common mistake to use comma-separated string
                rest => rest.iter().map(|a| Author::new(a)).collect(),
            },
            derived,
            manifest,
        };
        if let Origin::GitHub { .. } = &s.origin {
            debug_assert!(s.repo.is_some());
        }
        s
    }

    fn package(&self) -> &Package {
        self.manifest.package()
    }

    #[inline]
    pub fn homepage(&self) -> Option<&str> {
        self.package().homepage()
    }

    pub fn documentation(&self) -> Option<&str> {
        self.package().documentation()
    }

    pub fn edition(&self) -> Edition {
        self.package().edition()
    }

    pub fn has_own_keywords(&self) -> bool {
        !self.package().keywords().is_empty()
    }

    pub fn has_own_categories(&self) -> bool {
        !self.package().categories().is_empty()
    }

    pub fn manifest_raw_categories(&self) -> &[String] {
        self.package().categories()
    }

    /// Finds preferred capitalization for the name
    pub fn capitalized_name(&self) -> &str {
        &self.derived.capitalized_name
    }

    pub fn category_slugs(&self) -> &[Box<str>] {
        &self.derived.categories
    }

    pub fn license(&self) -> Option<&str> {
        self.package().license()
    }

    pub fn license_name(&self) -> Option<&str> {
        self.package().license().map(|s| match s {
            "" => "(unspecified)",
            "MIT OR Apache-2.0" | "MIT/Apache-2.0" | "MIT / Apache-2.0" => "MIT/Apache",
            "Apache-2.0/ISC/MIT" => "MIT/Apache/ISC",
            "BSD-3-Clause AND Zlib" => "BSD+Zlib",
            "CC0-1.0" => "CC0",
            s => s,
        })
    }

    pub fn license_file(&self) -> Option<&str> {
        self.package().license_file()
    }

    /// Either original keywords or guessed ones
    pub fn keywords(&self) -> &[String] {
        &self.derived.keywords
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
        &self.package().name
    }

    /// Without trailing '.' to match website's style
    pub fn description(&self) -> Option<&str> {
        self.package().description().map(|d| {
            let d = d.trim();
            if d.contains(". ") {d} // multiple sentences, leave them alone
            else {d.trim_end_matches('.')}
        })
        .filter(|&d| d != self.short_name()) // spams
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
            let relpath = self.derived.path_in_repo.as_deref().unwrap_or("");
            (repo, repo.canonical_http_url(relpath, self.derived.vcs_info_git_sha1.map(|s| hex::encode(s)).as_deref()))
        })
    }

    /// The path may be from vcs_info (not trusted)
    pub fn has_path_in_repo(&self) -> bool {
        self.derived.path_in_repo.is_some()
    }

    pub fn readme(&self) -> Option<&Readme> {
        self.derived.readme.as_ref()
    }

    pub fn readme_raw_path(&self) -> Option<&str> {
        self.package().readme().as_ref()
    }

    /// Contents of the `src/lib.rs` from the crate, if available
    pub fn lib_file(&self) -> Option<&str> {
        self.derived.lib_file.as_deref()
    }

    /// Contents of the `src/main.rs` from the crate, if available
    pub fn bin_file(&self) -> Option<&str> {
        self.derived.bin_file.as_deref()
    }

    pub fn lib_file_markdown(&self) -> Option<Markup> {
        self.derived.lib_file.as_ref().and_then(|code| {
            let out = extract_doc_comments(code);
            if !out.trim_start().is_empty() {
                Some(Markup::Markdown(out))
            } else {
                None
            }
        })
    }

    pub fn has_buildrs(&self) -> bool {
        self.derived.has_buildrs || matches!(self.package().build, Some(OptionalFile::Path(_) | OptionalFile::Flag(true)))
    }

    pub fn has_code_of_conduct(&self) -> bool {
        self.derived.has_code_of_conduct
    }

    pub fn has_examples(&self) -> bool {
        !self.manifest.example.is_empty()
    }

    pub fn has_tests(&self) -> bool {
        !self.manifest.test.is_empty()
    }

    pub fn has_benches(&self) -> bool {
        !self.manifest.bench.is_empty()
    }

    pub fn has_badges(&self) -> bool {
        self.manifest.badges.appveyor.is_some() ||
            self.manifest.badges.circle_ci.is_some() ||
            self.manifest.badges.gitlab.is_some() ||
            self.manifest.badges.codecov.is_some() ||
            self.manifest.badges.coveralls.is_some()
    }

    pub fn maintenance(&self) -> MaintenanceStatus {
        self.manifest.badges.maintenance.status
    }

    pub fn links(&self) -> Option<&str> {
        self.manifest.links()
    }

    #[inline]
    pub fn version(&self) -> &str {
        self.package().version()
    }

    pub fn version_semver(&self) -> Result<semver::Version, semver::Error> {
        semver::Version::parse(self.version())
    }

    pub fn is_yanked(&self) -> bool {
        self.derived.is_yanked
    }

    pub fn lib_name(&self) -> &str {
        self.manifest.lib.as_ref()
            .and_then(|l| l.name.as_deref())
            .unwrap_or_else(|| self.short_name())
    }

    pub fn has_lib(&self) -> bool {
        !self.is_proc_macro() && (self.derived.lib_file.is_some() || self.manifest.lib.is_some())
    }

    pub fn has_bin(&self) -> bool {
        self.manifest.has_bin()
    }

    pub fn bin_names(&self) -> Vec<&str> {
        self.manifest.bin.iter().map(|bin| {
            bin.name.as_deref().unwrap_or_else(|| self.short_name())
        }).collect()
    }

    // has cargo-prefixed bin
    pub fn has_cargo_bin(&self) -> bool {
        self.manifest.has_cargo_bin()
    }

    pub fn is_proc_macro(&self) -> bool {
        self.manifest.is_proc_macro()
    }

    pub fn is_app(&self) -> bool {
        self.has_bin() && !self.is_proc_macro() && !self.has_lib()
    }

    /// Does it use nightly-only features
    pub fn is_nightly(&self) -> bool {
        self.derived.is_nightly
    }

    pub fn is_no_std(&self) -> bool {
        self.package().categories().iter().any(|c| c == "no-std") ||
            self.package().keywords().iter().any(|k| k == "no-std" || k == "no_std") ||
            self.features().iter().any(|(k, _)| k == "no-std" || k == "no_std")
    }

    pub fn is_sys(&self) -> bool {
        self.manifest.is_sys(self.has_buildrs())
    }

    pub fn has_runtime_deps(&self) -> bool {
        !self.manifest.dependencies.is_empty() || self.manifest.target.values().any(|target| !target.dependencies.is_empty())
    }

    pub fn features(&self) -> &BTreeMap<String, Vec<String>> {
        &self.manifest.features
    }

    /// Runtime, dev, build
    pub fn direct_dependencies(&self) -> (Vec<RichDep>, Vec<RichDep>, Vec<RichDep>) {
        self.manifest.direct_dependencies()
    }

    pub fn language_stats(&self) -> &udedokei::Stats {
        &self.derived.language_stats
    }

    pub fn explicit_msrv(&self) -> Option<&str> {
        self.package().rust_version()
    }

    /// compressed (whole tarball) and decompressed (extracted files only)
    #[inline]
    pub fn crate_size(&self) -> (u64, u64) {
        (self.derived.crate_compressed_size as u64, self.derived.crate_decompressed_size as u64)
    }
}

pub trait ManifestExt {
    fn direct_dependencies(&self) -> (Vec<RichDep>, Vec<RichDep>, Vec<RichDep>);
    fn has_bin(&self) -> bool;
    fn has_cargo_bin(&self) -> bool;
    fn is_proc_macro(&self) -> bool;
    fn is_sys(&self, has_buildrs: bool) -> bool;
    fn links(&self) -> Option<&str>;
}
impl ManifestExt for Manifest {
    fn is_proc_macro(&self) -> bool {
        self.lib.as_ref().map_or(false, |lib| lib.proc_macro)
    }

    fn has_bin(&self) -> bool {
        !self.bin.is_empty()
    }

    fn has_cargo_bin(&self) -> bool {
        // we get binaries normalized, so no need to check for package name
        self.bin.iter().any(|b| b.name.as_ref().map_or(false, |n| n.starts_with("cargo-")))
    }

    fn links(&self) -> Option<&str> {
        self.package().links()
    }

    fn is_sys(&self, has_buildrs: bool) -> bool {
        let name = &self.package().name;
        !self.has_bin() &&
            has_buildrs &&
            !self.is_proc_macro() &&
            (self.links().is_some() ||
                (
                    name.ends_with("-sys") ||
                        name.ends_with("_sys") ||
                        self.package().categories().iter().any(|c| c == "external-ffi-bindings")
                    // _dll suffix is a false positive
                ))
    }

    /// run dev build
    fn direct_dependencies(&self) -> (Vec<RichDep>, Vec<RichDep>, Vec<RichDep>) {
        fn to_dep((name, dep): (&String, &Dependency)) -> (String, RichDep) {
            let package = dep.package().unwrap_or(name);
            (package.into(), RichDep {
                package: package.into(),
                user_alias: name.as_str().into(),
                dep: dep.clone(),
                only_for_features: BTreeMap::new(),
                only_for_targets: Vec::new(),
                with_features: Vec::new(),
            })
        }
        let mut normal: BTreeMap<String, RichDep> = self.dependencies.iter().map(to_dep).collect();
        let mut build: BTreeMap<String, RichDep> = self.build_dependencies.iter().map(to_dep).collect();
        let mut dev: BTreeMap<String, RichDep> = self.dev_dependencies.iter().map(to_dep).collect();

        fn add_targets(dest: &mut BTreeMap<String, RichDep>, src: &DepsSet, target: &str) {
            for (name, dep) in src {
                use std::collections::btree_map::Entry::*;
                let package = dep.package().unwrap_or(name);
                if let Vacant(e) = dest.entry(package.to_string()) {
                    let mut only_for_targets = Vec::new();
                    if let Ok(target) = target.parse().map_err(|e| log::warn!("Bad target '{}': {}", target, e)) {
                        only_for_targets.push(target);
                    }
                    e.insert(RichDep {
                        package: package.into(),
                        user_alias: name.as_str().into(),
                        dep: dep.clone(),
                        only_for_targets,
                        only_for_features: BTreeMap::new(),
                        with_features: Vec::new(),
                    });
                    // otherwise don't add platform info to existing cross-platform deps
                }
            }
        }
        for (target, plat) in &self.target {
            add_targets(&mut normal, &plat.dependencies, target);
            add_targets(&mut build, &plat.build_dependencies, target);
            add_targets(&mut dev, &plat.dev_dependencies, target);
        }

        // Don't display deps twice if they're required anyway
        for dep in normal.keys() {
            dev.remove(dep);
            build.remove(dep);
        }
        for dep in build.keys() {
            dev.remove(dep);
        }

        let default_features = self.features.get("default")
            .map(|d| d.iter().collect::<HashSet<_>>())
            .unwrap_or_default();

        for (for_feature, wants) in self.features.iter().filter(|(n, _)| *n != "default") {
            for depstr in wants {
                let mut depstr = depstr.splitn(2, '/');
                let name = depstr.next().expect("name should be there");
                let with_feature = depstr.next();

                if let Some(dep) = normal.get_mut(name).or_else(|| build.get_mut(name)) {
                    let enabled = default_features.get(for_feature).is_some();
                    if enabled {
                        if let Some(with_feature) = with_feature {
                            if !dep.dep.req_features().iter().any(|f| f == with_feature)
                                && !dep.with_features.iter().any(|f| &**f == with_feature)
                            {
                                dep.with_features.push(with_feature.into())
                            }
                        }
                    }
                    *dep.only_for_features.entry(for_feature.as_str().into()).or_insert(false) |= enabled;
                }
            }
        }
        fn convsort(dep: BTreeMap<String, RichDep>) -> Vec<RichDep> {
            let mut dep: Vec<_> = dep.into_iter()
                .map(|(_, mut dep)| {
                    if dep.only_for_features.is_empty() && dep.user_alias != dep.package {
                        dep.only_for_features.insert(dep.user_alias.clone(), false);
                    }
                    dep
                })
                .collect();
            dep.sort_by(|a,b| {
                a.dep.optional().cmp(&b.dep.optional())
                .then(a.only_for_targets.is_empty().cmp(&b.only_for_targets.is_empty()))
                .then(a.package.cmp(&b.package))
            });
            dep
        }
        (convsort(normal), convsort(dev), convsort(build))
    }
}

pub struct RichDep {
    pub package: Box<str>,
    pub user_alias: Box<str>,
    pub dep: Dependency,
    /// it's optional, used only for a platform
    pub only_for_targets: Vec<Target>,
    /// it's optional, used only if parent crate's feature is enabled
    pub only_for_features: BTreeMap<Box<str>, bool>,
    /// When used, these features of this dependency are enabled
    pub with_features: Vec<String>,
}

impl RichDep {
    pub fn for_features(&self) -> impl Iterator<Item = (&str, bool)> + '_ {
        self.only_for_features.iter().map(|(k, v)| (&**k, *v))
    }

    pub fn is_optional(&self) -> bool {
        !self.only_for_features.is_empty()
    }

    pub fn add_target(&mut self, target: &str) {
        self.only_for_targets.push(
            target.parse().unwrap_or_else(|_| {
                Target::Cfg(Cfg::Equal("target".to_string(), target.to_string()))
            }),
        );
    }
}

/// Metadata guessed
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Derived {
    pub categories: Vec<Box<str>>,
    pub keywords: Vec<String>,
    pub path_in_repo: Option<String>,
    pub language_stats: udedokei::Stats,
    pub crate_compressed_size: u32,
    pub crate_decompressed_size: u32,
    pub is_nightly: bool,
    pub capitalized_name: String,
    pub readme: Option<Readme>,
    pub lib_file: Option<String>,
    pub has_buildrs: bool,
    pub has_code_of_conduct: bool,
    pub is_yanked: bool,
    #[serde(default, skip_serializing_if="Option::is_none")]
    pub bin_file: Option<String>,
    #[serde(default, skip_serializing_if="Option::is_none")]
    pub vcs_info_git_sha1: Option<[u8; 20]>,
}

/// Metadata guessed
#[derive(Debug, Clone, Default)]
pub struct CrateVersionSourceData {
    pub github_keywords: Option<Vec<String>>,
    pub github_description: Option<String>,
    pub language_stats: udedokei::Stats,
    pub crate_compressed_size: u32,
    pub crate_decompressed_size: u32,
    pub is_nightly: bool,
    pub capitalized_name: String,
    pub readme: Option<Readme>,
    pub lib_file: Option<String>,
    /// src/main.rs
    pub bin_file: Option<String>,
    pub path_in_repo: Option<String>,
    pub vcs_info_git_sha1: Option<[u8; 20]>,
    pub has_buildrs: bool,
    pub has_code_of_conduct: bool,
    pub is_yanked: bool,
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
        } else if let Some(doccomment) = l.strip_prefix("//!") {
            out.push_str(doccomment);
            out.push('\n');
        }
    }
    out
}

#[test]
fn parse() {
    assert_eq!("hello\nworld", extract_doc_comments("/*!\nhello\nworld */").trim());
}
