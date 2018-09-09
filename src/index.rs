use std::collections::HashMap;
use std::collections::HashSet;
use lazyonce::LazyOnce;
use rich_crate::Origin;
use std::path::PathBuf;
use crates_index;
use crates_index::Crate;
use crates_index::Version;
use std::sync::{Arc, Mutex};
use std::iter;
use KitchenSinkErr;
use KitchenSink;
use semver::VersionReq;
use semver::Version as SemVer;
use std::sync::RwLock;
use deps_stats::DepsStats;

pub struct Index {
    crates: HashMap<Origin, Crate>,
    cache: RwLock<HashMap<(Box<str>, Features), ArcDepSet>>,
    deps_stats: LazyOnce<DepsStats>,
}

impl Index {
    pub fn new_default() -> Result<Self, KitchenSinkErr> {
        Ok(Self::new(KitchenSink::data_path()?.join("index")))
    }

    pub fn new(path: PathBuf) -> Self {
        let index = crates_index::Index::new(path);
        let crates = index.crates()
                .map(|c| (Origin::from_crates_io_name(c.name()), c))
                .collect();
        Self {
            cache: RwLock::new(HashMap::with_capacity(5000)),
            deps_stats: LazyOnce::new(),
            crates,
        }
    }

    /// All crates available in the index
    ///
    /// It returns only a thin and mostly useless data from the index itself,
    /// so `rich_crate`/`rich_crate_version` is needed to do more.
    pub fn crates(&self) -> &HashMap<Origin, Crate> {
        &self.crates
    }

    pub fn deps_stats(&self) -> &DepsStats {
        self.deps_stats.get(|| {
            self.get_deps_stats()
        })
    }

    pub fn crate_by_name(&self, name: &Origin) -> Result<&Crate, KitchenSinkErr> {
        self.crates()
        .get(name)
        .ok_or_else(|| KitchenSinkErr::CrateNotFound(name.clone()))
    }

    pub fn crate_version_latest_unstable(&self, name: &Origin) -> Result<&Version, KitchenSinkErr> {
        Ok(Self::highest_version(self.crate_by_name(name)?, false))
    }

    pub fn highest_version(krate: &Crate, stable_only: bool) -> &Version {
        krate.versions()
            .iter()
            .max_by_key(|a| {
                let ver = SemVer::parse(a.version())
                    .map_err(|e| eprintln!("{} has invalid version {}: {}", krate.name(), a.version(), e))
                    .ok();
                let bad = a.is_yanked() || (stable_only && !ver.as_ref().map_or(false, |v| v.pre.is_empty()));
                (!bad, ver)
            })
            .unwrap_or_else(|| krate.latest_version()) // latest_version = most recently published version
    }

    pub fn deps_of_crate(&self, krate: &Crate, DepQuery {default, all_optional, dev}: DepQuery) -> Result<Dep, KitchenSinkErr> {
        let latest = Self::highest_version(krate, true);
        let mut features = Vec::with_capacity(if all_optional {latest.features().len()} else {0});
        if all_optional {
            features.extend(latest.features().iter().filter(|(_,v)| !v.is_empty()).map(|(c, _)| c.to_string().into_boxed_str()));
        };
        Ok(Dep {
            semver: semver_parse(latest.version()),
            runtime: self.deps_of_ver(latest, Features {
                all_targets: all_optional,
                default,
                build: false,
                dev,
                features: features.clone().into_boxed_slice(),
            })?,
            build: self.deps_of_ver(latest, Features {
                all_targets: all_optional,
                default,
                build: true,
                dev,
                features: features.into_boxed_slice(),
            })?,
        })
    }

    pub fn deps_of_ver(&self, ver: &Version, wants: Features) -> Result<ArcDepSet, KitchenSinkErr> {
        let key = (format!("{}-{}", ver.name(), ver.version()).into(), wants.clone());
        if let Some(cached) = self.cache.read().unwrap().get(&key) {
            return Ok(cached.clone());
        }

        let mut to_enable = HashMap::new();
        let ver_features = ver.features(); // available features
        let all_wanted_features = wants.features.iter()
                        .map(|s| s.as_ref())
                        .chain(iter::repeat("default").take(if wants.default {1} else {0}));
        for feat in all_wanted_features {
            if let Some(enable) = ver_features.get(feat) {
                for enable in enable {
                    let mut t = enable.splitn(2, '/');
                    let dep_name = t.next().unwrap();
                    let enabled = to_enable.entry(dep_name.to_owned())
                        .or_insert(HashSet::new());
                    if let Some(enable) = t.next() {
                        enabled.insert(enable);
                    }
                }
            } else {
                to_enable.entry(feat.to_owned()).or_insert(HashSet::new());
            }
        }

        let mut set: HashMap<DepName, (_, _, SemVer, HashSet<String>)> = HashMap::with_capacity(60);
        for d in ver.dependencies() {

            // people forget to include winapi conditionally
            let is_target_specific = d.name() == "winapi" || d.target().is_some();
            if !wants.all_targets && is_target_specific {
                continue; // FIXME: allow common targets?
            }
            // hopefully nobody uses clippy at runtime, they just fail to make it dev dep
            if !wants.dev && d.name() == "clippy" && d.is_optional() {
                continue;
            }

            match d.kind() {
                Some("normal") | None => (),
                Some("build") if wants.build => (),
                Some("dev") if wants.dev => (),
                _ => continue,
            }

            let enable_dep_features = to_enable.get(d.name());
            if d.is_optional() && enable_dep_features.is_none() {
                continue;
            }

            let req = VersionReq::parse(d.requirement())
                .map_err(|_| KitchenSinkErr::SemverParsingError)?;
            let name = d.name();
            let krate = match self.crate_by_name(&Origin::from_crates_io_name(name)) {
                Ok(k) => k,
                Err(e) => {
                    eprintln!("{} depends on missing crate {}: {}", ver.name(), name, e);
                    continue;
                },
            };
            let (matched, semver) = krate.versions().iter().rev()
                .filter(|v| !v.is_yanked())
                .filter_map(|v| Some((v, SemVer::parse(v.version()).ok()?)))
                .find(|(_, semver)| {
                    req.matches(&semver)
                })
                .unwrap_or_else(|| {
                    let ver = krate.latest_version(); // bad version, but it shouldn't happen anywya
                    let semver = semver_parse(ver.version());
                    (ver, semver)
                });

            let key = (name.into(), matched.version().into());

            let (_, _, _, all_features) = set.entry(key)
                .or_insert_with(|| (d, matched.clone(), semver, HashSet::new()));
            all_features.extend(d.features().iter().cloned());
            if let Some(s) = enable_dep_features {
                all_features.extend(s.iter().map(|s| s.to_string()));
            }
        }

        // break infinite recursion. Must be inserted first, since depth-first search
        // may end up requesting it.
        let result = Arc::new(Mutex::new(HashMap::new()));
        self.cache.write().unwrap().insert(key, result.clone());

        let set: Result<_,_> = set.into_iter().map(|(k, (d, matched, semver, all_features))| {
            let all_features = all_features.into_iter().map(Into::into).collect::<Vec<_>>().into_boxed_slice();
            let runtime = self.deps_of_ver(&matched, Features {
                all_targets: wants.all_targets,
                build: false,
                dev: false, // dev is only for top-level
                default: d.has_default_features(),
                features: all_features.clone(),
            })?;
            let build = self.deps_of_ver(&matched, Features {
                all_targets: wants.all_targets,
                build: true,
                dev: false, // dev is only for top-level
                default: d.has_default_features(),
                features: all_features,
            })?;
            Ok((k, Dep {
                semver,
                runtime,
                build,
            }))
        }).collect();

        *result.lock().unwrap() = set?;
        Ok(result)
    }

    /// For crate being outdated. Returns (is_latest, popularity)
    /// 0 = not used *or deprecated*
    /// 1 = everyone uses it
    pub fn version_popularity(&self, crate_name: &str, requirement: VersionReq) -> (bool, f32) {
        if is_deprecated(crate_name) {
            return (false, 0.);
        }

        let matches_latest = self.crate_by_name(&Origin::from_crates_io_name(crate_name))
        .ok()
        .and_then(|krate| {
            Self::highest_version(krate, true).version().parse().ok()
        })
        .map_or(false, |latest| {
            requirement.matches(&latest)
        });

        let stats = self.deps_stats();
        let pop = stats.counts.get(crate_name)
        .map(|stats| {
            let mut matches = 0;
            let mut unmatches = 0;
            for (ver, count) in &stats.versions {
                if requirement.matches(ver) {
                    matches += count; // TODO: this should be (slighly) weighed by crate's popularity?
                } else {
                    unmatches += count;
                }
            }
            matches += 1; // one to denoise unpopular crates; div/0
            matches as f32 / (matches + unmatches) as f32
        })
        .unwrap_or(0.);

        (matches_latest, pop)
    }

    /// How likely it is that this exact crate will be installed in any project
    pub fn version_commonality(&self, crate_name: &str, version: &SemVer) -> f32 {
        match crate_name {
            // bindings' SLoC looks heavier than actual overhead of standard system libs
            "libc" | "winapi" | "kernel32-sys" => return 0.91,
            _ => {},
        }

        let stats = self.deps_stats();
        stats.counts.get(crate_name)
        .and_then(|c| {
            c.versions.get(version)
            .map(|&ver| ver as f32 / stats.total as f32)
        })
        .unwrap_or(0.)
    }
}

use std::fmt;
impl fmt::Debug for Dep {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Dep {{ {}, runtime: x{}, build: x{} }}", self.semver, self.runtime.lock().unwrap().len(), self.build.lock().unwrap().len())
    }
}

/// TODO: check if the repo is rust-lang-deprecated.
/// Note: the repo URL in the crate is outdated, and it may be a redirect to the deprecated
fn is_deprecated(name: &str) -> bool {
    match name {
        "rustc-serialize" | "gcc" | "rustc-benchmarks" | "time" | "tempdir" => true,
        _ => false,
    }
}

fn semver_parse(ver: &str) -> SemVer {
    SemVer::parse(ver)
        .unwrap_or_else(|_| SemVer::parse("0.0.0").expect("must parse"))
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Features {
    pub all_targets: bool,
    pub default: bool,
    pub build: bool,
    pub dev: bool,
    pub features: Box<[Box<str>]>,
}

pub type DepName = (Arc<str>, Box<str>);
pub type DepSet = HashMap<DepName, Dep>;
pub type ArcDepSet = Arc<Mutex<DepSet>>;

pub struct Dep {
    pub semver: SemVer,
    pub runtime: ArcDepSet,
    pub build: ArcDepSet,
}

#[derive(Debug, Copy, Clone)]
pub struct DepQuery {
    pub default: bool,
    pub all_optional: bool,
    pub dev: bool,
}

#[test]
fn index_test() {
    let idx = Index::new_default().unwrap();
    let stats = idx.deps_stats();
    assert!(stats.total > 13800);
    let lode = stats.counts.get("lodepng").unwrap();
    assert_eq!(8, lode.runtime.0);
}
