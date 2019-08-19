use crate::deps_stats::DepsStats;
use crate::git_crates_index::*;
use crate::KitchenSink;
use crate::KitchenSinkErr;
use crates_index;
use crates_index::Crate;
use crates_index::Version;
use lazyonce::LazyOnce;
use parking_lot::Mutex;
use parking_lot::RwLock;
use rich_crate::Origin;
use semver::Version as SemVer;
use semver::VersionReq;
use std::iter;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use string_interner::StringInterner;
use string_interner::Sym;

type FxHashMap<K, V> = std::collections::HashMap<K, V, ahash::ABuildHasher>;
type FxHashSet<V> = std::collections::HashSet<V, ahash::ABuildHasher>;

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct MiniVer {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
    pub build: u16,
    pub pre: Box<[semver::Identifier]>,
}

impl MiniVer {
    pub fn to_semver(&self) -> SemVer {
        SemVer {
            major: self.major.into(),
            minor: self.minor.into(),
            patch: self.patch.into(),
            pre: self.pre.clone().into(),
            build: if self.build > 0 { vec![semver::Identifier::Numeric(self.build.into())] } else { Vec::new() },
        }
    }
}

pub struct Index {
    indexed_crates: LazyOnce<FxHashMap<Box<str>, Crate>>,
    pub(crate) crates_io_index: crates_index::Index,
    git_index: GitIndex,

    pub(crate) inter: RwLock<StringInterner<Sym>>,
    pub(crate) cache: RwLock<FxHashMap<(Box<str>, Features), ArcDepSet>>,
    deps_stats: LazyOnce<DepsStats>,
}

impl Index {
    pub fn new_default() -> Result<Self, KitchenSinkErr> {
        Self::new(&KitchenSink::data_path()?)
    }

    pub fn new(data_dir: &Path) -> Result<Self, KitchenSinkErr> {
        let crates_io_index = crates_index::Index::new(data_dir.join("index"));
        Ok(Self {
            git_index: GitIndex::new(data_dir)?,
            cache: RwLock::new(FxHashMap::with_capacity_and_hasher(5000, Default::default())),
            inter: RwLock::new(StringInterner::new()),
            deps_stats: LazyOnce::new(),
            indexed_crates: LazyOnce::new(),
            crates_io_index,
        })
    }

    pub fn update(&self) {
        let _ = self.crates_io_index.update().map_err(|e| eprintln!("{}", e));
    }

    /// Crates available in the crates.io index
    ///
    /// It returns only a thin and mostly useless data from the index itself,
    /// so `rich_crate`/`rich_crate_version` is needed to do more.
    pub fn crates_io_crates(&self) -> &FxHashMap<Box<str>, Crate> {
        self.indexed_crates.get(|| {
            self.crates_io_index.crates()
                .map(|c| (c.name().to_ascii_lowercase().into(), c))
                .collect()
        })
    }

    pub fn crate_exists(&self, origin: &Origin) -> bool {
        match origin {
            Origin::CratesIo(lowercase_name) => self.crates_io_crate_by_lowercase_name(lowercase_name).is_ok(),
            Origin::GitHub {..} | Origin::GitLab {..} => self.git_index.has(origin),
        }
    }

    /// All crates available in the crates.io index and our index
    ///
    pub fn all_crates(&self) -> impl Iterator<Item=Origin> + '_ {
        self.git_index.crates().cloned().chain(self.crates_io_crates().keys().map(|n| Origin::from_crates_io_name(&n)))
    }

    pub fn deps_stats(&self) -> Result<&DepsStats, KitchenSinkErr> {
        match self.deps_stats.try_get_for(Duration::from_secs(10), || self.get_deps_stats()) {
            Some(res) => Ok(res),
            None => {
                eprintln!("rayon deadlock");
                return Err(KitchenSinkErr::DepsStatsNotAvailable)
            },
        }
    }

    #[inline]
    pub fn crates_io_crate_by_lowercase_name(&self, name: &str) -> Result<&Crate, KitchenSinkErr> {
        debug_assert_eq!(name, name.to_ascii_lowercase());
        self.crates_io_crates()
        .get(name)
        .ok_or_else(|| KitchenSinkErr::CrateNotFound(Origin::from_crates_io_name(name)))
    }

    pub fn crate_version_latest_unstable(&self, name: &str) -> Result<&Version, KitchenSinkErr> {
        debug_assert_eq!(name, name.to_ascii_lowercase());
        Ok(Self::highest_crates_io_version(self.crates_io_crate_by_lowercase_name(name)?, false))
    }

    fn highest_crates_io_version(krate: &Crate, stable_only: bool) -> &Version {
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

    pub(crate) fn deps_of_crate(&self, krate: &Crate, DepQuery { default, all_optional, dev }: DepQuery) -> Result<Dep, KitchenSinkErr> {
        let latest = Self::highest_crates_io_version(krate, true);
        let mut features = Vec::with_capacity(if all_optional { latest.features().len() } else { 0 });
        if all_optional {
            features.extend(latest.features().iter().filter(|(_, v)| !v.is_empty()).map(|(c, _)| c.to_string().into_boxed_str()));
        };
        Ok(Dep {
            semver: semver_parse(latest.version()).into(),
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

    pub(crate) fn deps_of_ver(&self, ver: &Version, wants: Features) -> Result<ArcDepSet, KitchenSinkErr> {
        let key = (format!("{}-{}", ver.name(), ver.version()).into(), wants.clone());
        if let Some(cached) = self.cache.read().get(&key) {
            return Ok(cached.clone());
        }

        let ver_features = ver.features(); // available features
        let mut to_enable = FxHashMap::with_capacity_and_hasher(wants.features.len(), Default::default());
        let all_wanted_features = wants.features.iter()
                        .map(|s| s.as_ref())
                        .chain(iter::repeat("default").take(if wants.default {1} else {0}));
        for feat in all_wanted_features {
            if let Some(enable) = ver_features.get(feat) {
                for enable in enable {
                    let mut t = enable.splitn(2, '/');
                    let dep_name = t.next().unwrap();
                    let enabled = to_enable.entry(dep_name.to_owned())
                        .or_insert(FxHashSet::default());
                    if let Some(enable) = t.next() {
                        enabled.insert(enable);
                    }
                }
            } else {
                to_enable.entry(feat.to_owned()).or_insert_with(FxHashSet::default);
            }
        }

        let mut set: FxHashMap<DepName, (_, _, SemVer, FxHashSet<String>)> = FxHashMap::with_capacity_and_hasher(60, Default::default());
        for d in ver.dependencies() {
            let package = d.package().unwrap_or_else(|| d.name()).to_ascii_lowercase();

            // people forget to include winapi conditionally
            let is_target_specific = package == "winapi" || d.target().is_some();
            if !wants.all_targets && is_target_specific {
                continue; // FIXME: allow common targets?
            }
            // hopefully nobody uses clippy at runtime, they just fail to make it dev dep
            if !wants.dev && package == "clippy" && d.is_optional() {
                continue;
            }

            match d.kind() {
                Some("normal") | None => (),
                Some("build") if wants.build => (),
                Some("dev") if wants.dev => (),
                _ => continue,
            }

            let enable_dep_features = to_enable.get(&package);
            if d.is_optional() && enable_dep_features.is_none() {
                continue;
            }

            let req = VersionReq::parse(d.requirement()).map_err(|_| KitchenSinkErr::SemverParsingError)?;
            let krate = match self.crates_io_crate_by_lowercase_name(&package) {
                Ok(k) => k,
                Err(e) => {
                    eprintln!("{}@{} depends on missing crate {} (@{}): {}", ver.name(), ver.version(), package, req, e);
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


            let key = {
                let mut inter = self.inter.write();
                (inter.get_or_intern(package), inter.get_or_intern(matched.version()))
            };

            let (_, _, _, all_features) = set.entry(key)
                .or_insert_with(|| (d, matched.clone(), semver, FxHashSet::default()));
            all_features.extend(d.features().iter().cloned());
            if let Some(s) = enable_dep_features {
                all_features.extend(s.iter().map(|s| s.to_string()));
            }
        }

        // break infinite recursion. Must be inserted first, since depth-first search
        // may end up requesting it.
        let result = Arc::new(Mutex::new(FxHashMap::default()));
        self.cache.write().insert(key, result.clone());

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
                semver: semver.into(),
                runtime,
                build,
            }))
        }).collect();

        *result.lock() = set?;
        Ok(result)
    }

    pub fn clear_cache(&self) {
        self.cache.write().clear();
        *self.inter.write() = StringInterner::new();
    }

    /// For crate being outdated. Returns (is_latest, popularity)
    /// 0 = not used *or deprecated*
    /// 1 = everyone uses it
    pub fn version_popularity(&self, crate_name: &str, requirement: &VersionReq) -> Result<Option<(bool, f32)>, KitchenSinkErr> {
        if is_deprecated(crate_name) {
            return Ok(Some((false, 0.)));
        }

        let krate = self.crates_io_crate_by_lowercase_name(&crate_name.to_ascii_lowercase())?;

        let matches_latest = Self::highest_crates_io_version(krate, true).version().parse().ok()
            .map_or(false, |latest| requirement.matches(&latest));

        let stats = self.deps_stats()?;
        let pop = stats.counts.get(crate_name)
        .map(|stats| {
            let mut matches = 0;
            let mut unmatches = 0;
            for (ver, count) in &stats.versions {

                if requirement.matches(&ver.to_semver()) {
                    matches += count; // TODO: this should be (slighly) weighed by crate's popularity?
                } else {
                    unmatches += count;
                }
            }
            matches += 1; // one to denoise unpopular crates; div/0
            matches as f32 / (matches + unmatches) as f32
        })
        .unwrap_or(0.);

        Ok(Some((matches_latest, pop)))
    }

    /// How likely it is that this exact crate will be installed in any project
    pub fn version_commonality(&self, crate_name: &str, version: &MiniVer) -> Result<Option<f32>, KitchenSinkErr> {
        match crate_name {
            // bindings' SLoC looks heavier than actual overhead of standard system libs
            "libc" | "winapi" | "kernel32-sys" => return Ok(Some(0.91)),
            _ => {},
        }

        let stats = self.deps_stats()?;
        Ok(stats.counts.get(crate_name)
        .and_then(|c| {
            c.versions.get(&version)
            .map(|&ver| ver as f32 / stats.total as f32)
        }))
    }
}

use std::fmt;
impl fmt::Debug for Dep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Dep {{ {}, runtime: x{}, build: x{} }}", self.semver, self.runtime.lock().len(), self.build.lock().len())
    }
}

/// TODO: check if the repo is rust-lang-deprecated.
/// Note: the repo URL in the crate is outdated, and it may be a redirect to the deprecated
pub fn is_deprecated(name: &str) -> bool {
    match name {
        "rustc-serialize" | "gcc" | "rustc-benchmarks" | "time" |
        "flate2-crc" | "complex" | "simple_stats" | "concurrent" | "feed" |
        "isatty" | "thread-scoped" | "target_build_utils" | "chan" | "chan-signal" |
        "glsl-to-spirv" => true,
        _ => false,
    }
}

fn semver_parse(ver: &str) -> SemVer {
    SemVer::parse(ver).unwrap_or_else(|_| SemVer::parse("0.0.0").expect("must parse"))
}

impl From<SemVer> for MiniVer {
    fn from(s: SemVer) -> Self {
        Self {
            major: s.major as u16,
            minor: s.minor as u16,
            patch: s.patch as u16,
            pre: s.pre.into_boxed_slice(),
            build: if let Some(semver::Identifier::Numeric(m)) = s.build.get(0) {*m as u16} else {0},
        }
    }
}

impl fmt::Display for MiniVer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}-{}", self.major, self.minor, self.patch, self.build)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Features {
    pub all_targets: bool,
    pub default: bool,
    pub build: bool,
    pub dev: bool,
    pub features: Box<[Box<str>]>,
}

pub type DepName = (Sym, Sym);
pub type DepSet = FxHashMap<DepName, Dep>;
pub type ArcDepSet = Arc<Mutex<DepSet>>;

pub struct Dep {
    pub semver: MiniVer,
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
    let stats = idx.deps_stats().unwrap();
    assert!(stats.total > 13800);
    let lode = stats.counts.get("lodepng").unwrap();
    assert_eq!(11, lode.runtime.def);
}
