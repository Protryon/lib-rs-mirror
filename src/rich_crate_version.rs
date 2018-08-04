use crates_index::Version;
use Author;
use Readme;
use Origin;
use semver;
use repo_url::Repo;
use cargo_toml::TomlManifest;
use cargo_toml::TomlDependency;
pub use cargo_toml::TomlDepsSet;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::collections::HashSet;
use categories::Categories;

pub use parse_cfg::{Target, Cfg};
pub use parse_cfg::ErrorKind as CfgErr;

#[derive(Debug, Clone)]
pub struct RichCrateVersion {
    origin: Origin,
    index: Version,
    manifest: TomlManifest,
    derived: Derived,
    authors: Vec<Author>,
    readme: Result<Option<Readme>, ()>,
    lib_file: Option<String>,
    repo: Option<Repo>,
    path_in_repo: Option<String>,
    has_buildrs: bool,
}

/// Data for a specific version of a crate.
///
/// Crates.rs uses this only for the latest version of a crate.
impl RichCrateVersion {
    pub fn new(index: Version, manifest: TomlManifest, derived: Derived, readme: Result<Option<Readme>, ()>, lib_file: Option<String>, path_in_repo: Option<String>, has_buildrs: bool) -> Self {
        Self {
            origin: Origin::from_crates_io_name(index.name()),
            repo: manifest.package.repository.as_ref().and_then(|r| Repo::new(r).ok()),
            authors: manifest.package.authors.iter().map(|a| Author::new(a)).collect(),
            index, manifest, readme, has_buildrs,
            derived,
            path_in_repo,
            lib_file,
        }
    }

    pub fn homepage(&self) -> Option<&str> {
        self.manifest.package.homepage.as_ref().map(|s| s.as_ref())
    }

    pub fn documentation(&self) -> Option<&str> {
        self.manifest.package.documentation.as_ref().map(|s| s.as_ref())
    }

    pub fn has_categories(&self) -> bool {
        !self.manifest.package.categories.is_empty() || self.derived.categories.as_ref().map_or(false, |c| !c.is_empty())
    }

    pub fn category_slugs(&self) -> impl Iterator<Item = Cow<str>> {
        Categories::fixed_category_slugs(if let Some(ref assigned_categories) = self.derived.categories {
            &assigned_categories
        } else {
            &self.manifest.package.categories
        })
    }

    pub fn raw_category_slugs(&self) -> impl Iterator<Item = Cow<str>> {
        Categories::fixed_category_slugs(&self.manifest.package.categories)
    }

    pub fn license(&self) -> Option<&str> {
        self.manifest.package.license.as_ref().map(|s| s.as_str())
    }

    pub fn license_file(&self) -> Option<&str> {
        self.manifest.package.license_file.as_ref().map(|s| s.as_str())
    }

    /// Either original keywords or guessed ones
    pub fn keywords(&self) -> impl Iterator<Item = &str> {
        self.derived.keywords.as_ref()
            .unwrap_or(&self.manifest.package.keywords)
            .iter().map(|s| s.as_ref())
    }

    /// Only keywords specified by the crate author. May be empty.
    pub fn raw_keywords(&self) -> impl Iterator<Item = &str> {
        self.manifest.package.keywords.iter().map(|s| s.as_ref())
    }

    /// Globally unique URL-like string identifying source & the crate within that source
    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    pub fn crates_io_url(&self) -> Option<String> {
        Some(format!("https://crates.io/crates/{}", self.short_name()))
    }

    pub fn docs_rs_url(&self) -> Option<String> {
        Some(format!("https://docs.rs/{}/{}/{}", self.short_name(), self.version(), self.short_name()))
    }

    /// Readable name
    pub fn short_name(&self) -> &str {
        self.index.name()
    }

    /// Without trailing '.' to match website's style
    pub fn description(&self) -> Option<&str> {
        self.manifest.package.description.as_ref().map(|d| d.as_str().trim_right_matches('.').trim())
    }

    /// Only explicitly-specified authors
    pub fn authors(&self) -> &[Author] {
        &self.authors
    }

    pub fn repository(&self) -> Option<&Repo> {
        self.repo.as_ref()
    }

    pub fn repository_link(&self) -> Option<(Cow<str>, &str)> {
        self.repository().map(|repo| {
            let relpath = self.path_in_repo.as_ref().map(|s| s.as_str()).unwrap_or("");
            (repo.canonical_http_url(relpath), repo.site_link_label())
        })
    }

    pub fn readme(&self) -> Result<Option<&Readme>, ()> {
        self.readme.as_ref().map(|r| r.as_ref()).map_err(|_|())
    }

    /// Contents of the `src/lib.rs` from the crate, if available
    pub fn lib_file(&self) -> Option<&str> {
        self.lib_file.as_ref().map(|s| s.as_str())
    }

    pub fn has_buildrs(&self) -> bool {
        self.has_buildrs || self.manifest.package.build.is_some()
    }

    pub fn links(&self) -> Option<&str> {
        self.manifest.package.links.as_ref().map(|s| s.as_str())
    }

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
        self.lib_file.is_some() || self.manifest.lib.is_some()
    }

    pub fn has_bin(&self) -> bool {
        !self.manifest.bin.is_empty()
    }

    pub fn is_app(&self) -> bool {
        self.has_bin() && !self.has_lib()
    }

    pub fn is_sys(&self) -> bool {
        !self.has_bin() &&
        self.has_buildrs() &&
        (self.links().is_some() || (
            self.short_name().ends_with("-sys") ||
            self.short_name().ends_with("_sys") ||
            self.category_slugs().any(|c| c == "external-ffi-bindings")
            // _dll suffix is a false positive
        ))
    }

    pub fn has_runtime_deps(&self) -> bool {
        !self.manifest.dependencies.is_empty()
    }

    pub fn features(&self) -> &BTreeMap<String, Vec<String>> {
        &self.manifest.features
    }

    /// Runtime, dev, build
    pub fn dependencies(&self) -> Result<(Vec<RichDep>, Vec<RichDep>, Vec<RichDep>), CfgErr> {
        fn to_dep((name, dep): (&String, &TomlDependency)) -> (String, RichDep) {
            (name.to_owned(), RichDep {
                name: name.to_owned(),
                dep: dep.clone(),
                only_for_features: Vec::new(),
                only_for_targets: Vec::new(),
                with_features: Vec::new(),
            })
        }
        let mut normal: BTreeMap<String, RichDep> = self.manifest.dependencies.iter().map(to_dep).collect();
        let mut dev: BTreeMap<String, RichDep> = self.manifest.dev_dependencies.iter().map(to_dep).collect();
        let mut build: BTreeMap<String, RichDep> = self.manifest.build_dependencies.iter().map(to_dep).collect();
        // Don't display deps twice if they're required anyway
        for dep in normal.keys() {
            dev.remove(dep);
            build.remove(dep);
        }
        for dep in build.keys() {
            dev.remove(dep);
        }
        fn add_targets(dest: &mut BTreeMap<String, RichDep>, src: &TomlDepsSet, target: &str) -> Result<(), CfgErr> {
            for (k, v) in src {
                use std::collections::btree_map::Entry::*;
                match dest.entry(k.to_string()) {
                    Vacant(e) => {
                        e.insert(RichDep {
                            name: k.to_owned(),
                            dep: v.to_owned(),
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
        for (ref target, ref plat) in &self.manifest.target {
            add_targets(&mut normal, &plat.dependencies, target)?;
            add_targets(&mut dev, &plat.dev_dependencies, target)?;
            add_targets(&mut build, &plat.build_dependencies, target)?;
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
                            if !dep.dep.req_features().iter().any(|f| f == with_feature) &&
                               !dep.with_features.iter().any(|f| f == with_feature) {
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
                .then(a.name.cmp(&b.name))
            });
            dep
        }
        Ok((convsort(normal), convsort(dev), convsort(build)))
    }
}

pub struct RichDep {
    pub name: String,
    pub dep: TomlDependency,
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

    pub fn add_target(&mut self, target: &str) -> Result<(), CfgErr> {
        self.only_for_targets.push(target.parse()
            .unwrap_or_else(|_| Target::Cfg(Cfg::Equal("target".to_string(), target.to_string()))));
        Ok(())
    }
}

/// Metadata guessed
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Derived {
    pub categories: Option<Vec<String>>,
    pub keywords: Option<Vec<String>>,
}
