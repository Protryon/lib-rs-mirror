use dashmap::DashMap;
use old_semver::SemVerError;
use rayon::prelude::*;
pub use crates_index::DependencyKind;
pub use crates_index::Version;
use crate::deps_stats::DepsStats;
use crate::DepsErr;
use crate::git_crates_index::*;
use crates_index::Crate;
use crates_index::Dependency;
use double_checked_cell_async::DoubleCheckedCell;
use log::{debug, warn, info, error};
use parking_lot::RwLock;
use rich_crate::Origin;
use rich_crate::RichCrateVersion;
use rich_crate::RichDep;
use semver::Version as SemVer;
use semver::VersionReq;
use serde_derive::*;
use smartstring::alias::String as SmolStr;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::time::Instant;
use string_interner::StringInterner;
use string_interner::symbol::SymbolU32 as Sym;
use ahash::{HashMap, HashSet};
use feat_extractor::is_deprecated_requirement;
use triomphe::Arc;

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize, Default)]
pub struct MiniVer {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
    pub build: u16,
    pub pre: Box<[old_semver::Identifier]>,
}

impl MiniVer {
    pub fn to_semver(&self) -> SemVer {
        if self.pre.is_empty() && self.build == 0 {
            return SemVer {
                major: self.major.into(),
                minor: self.minor.into(),
                patch: self.patch.into(),
                pre: semver::Prerelease::EMPTY,
                build: semver::BuildMetadata::EMPTY
            };
        }
        old_semver::Version {
            major: self.major.into(),
            minor: self.minor.into(),
            patch: self.patch.into(),
            pre: self.pre.clone().into(),
            build: if self.build > 0 { vec![old_semver::Identifier::Numeric(self.build.into())] } else { Vec::new() },
        }.to_string().parse().unwrap_or(SemVer { major: 0, minor: 0, patch: 0, pre: semver::Prerelease::EMPTY, build: semver::BuildMetadata::EMPTY })
    }
}

pub trait FeatureGetter {
    fn get_feature_descriptors(&self, key: &str) -> Option<&Vec<String>>;
}
impl FeatureGetter for std::collections::HashMap<String, Vec<String>> {
    fn get_feature_descriptors(&self, key: &str) -> Option<&Vec<String>> {
        self.get(key)
    }
}
impl FeatureGetter for std::collections::BTreeMap<String, Vec<String>> {
    fn get_feature_descriptors(&self, key: &str) -> Option<&Vec<String>> {
        self.get(key)
    }
}

pub trait IVersion {
    type Features: FeatureGetter;
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    fn dependencies(&self) -> Fudge;
    fn features(&self) -> &Self::Features;
    fn is_yanked(&self) -> bool;
}

impl IVersion for Version {
    type Features = std::collections::HashMap<String, Vec<String>>;
    fn name(&self) -> &str {self.name()}
    fn version(&self) -> &str {self.version()}
    fn dependencies(&self) -> Fudge {Fudge::CratesIo(self.dependencies())}
    fn features(&self) -> &Self::Features  {self.features()}
    fn is_yanked(&self) -> bool {self.is_yanked()}
}

pub trait ICrate {
    type Ver: IVersion;
    fn latest_version_with_features(&self, all_optional: bool) -> (&Self::Ver, Box<[SmolStr]>);
}

impl ICrate for Crate {
    type Ver = Version;
    fn latest_version_with_features(&self, all_optional: bool) -> (&Self::Ver, Box<[SmolStr]>) {
        let latest = Index::highest_crates_io_version(self, true);
        let mut features = Vec::with_capacity(if all_optional {
            latest.features().len() + latest.dependencies().iter().filter(|d| d.is_optional()).count()
        } else { 0 });
        if all_optional {
            features.extend(latest.features().iter().filter(|(_, v)| !v.is_empty()).map(|(c, _)| c.into()));
            // optional dependencis make implicit features
            features.extend(latest.dependencies().iter().filter(|d| d.is_optional()).map(|d| d.name().into()));
        };
        let features = features.into();
        (latest, features)
    }
}

pub enum Fudge<'a> {
    CratesIo(&'a [Dependency]),
    Manifest((Vec<RichDep>, Vec<RichDep>, Vec<RichDep>)),
}

impl IVersion for RichCrateVersion {
    type Features = std::collections::BTreeMap<String, Vec<String>>;
    fn name(&self) -> &str {self.short_name()}
    fn version(&self) -> &str {self.version()}
    fn dependencies(&self) -> Fudge {Fudge::Manifest(self.direct_dependencies())}
    fn features(&self) -> &Self::Features {self.features()}
    fn is_yanked(&self) -> bool {self.is_yanked()}
}

impl ICrate for RichCrateVersion {
    type Ver = RichCrateVersion;
    fn latest_version_with_features(&self, all_optional: bool) -> (&Self::Ver, Box<[SmolStr]>) {
        let mut features = Vec::with_capacity(if all_optional { self.features().len() } else { 0 });
        if all_optional {
            features.extend(self.features().iter().filter(|(_, v)| !v.is_empty()).map(|(c, _)| c.into()));
        };
        let features = features.into();
        (self, features)
    }
}

pub struct Index {
    indexed_crates: HashMap<SmolStr, Crate>,
    pub crates_index_path: PathBuf,
    git_index: GitIndex,

    pub inter: RwLock<StringInterner<Sym, string_interner::backend::StringBackend<Sym>>>,
    pub cache: DashMap<(SmolStr, Features), ArcDepSet, ahash::RandomState>,
    deps_stats: DoubleCheckedCell<DepsStats>,
}

impl Index {
    pub fn new(data_dir: &Path) -> Result<Self, DepsErr> {
        let crates_index_path = data_dir.join("index");
        debug!("Scanning crates index at {}…", crates_index_path.display());
        let start = Instant::now();
        let crates_io_index = crates_index::Index::with_path(&crates_index_path, "https://github.com/rust-lang/crates.io-index")
        .map_err(|e| DepsErr::Crates(e.to_string()))?;
        let indexed_crates: HashMap<_,_> = crates_io_index.crates_parallel()
            .filter_map(|c| {
                debug_assert!(c.is_ok());
                let c = c.ok()?;
                if c.name() == "test+package" {
                    return None; // crates-io bug
                }
                let mut name = SmolStr::from(c.name());
                if !name.bytes().all(|c| c.is_ascii_lowercase() || c == b'_' || c == b'-') {
                    name = c.name().to_ascii_lowercase().into();
                }
                debug_assert!(Origin::is_valid_crate_name(&name), "{name}");
                Some((name, c))
            })
            .collect();
        info!("Scanned crates index in {}…", start.elapsed().as_millis() as u32);
        if indexed_crates.len() < 90_000 {
            return Err(DepsErr::IndexBroken);
        }
        Ok(Self {
            git_index: GitIndex::new(data_dir)?,
            cache: DashMap::with_capacity_and_hasher(10000, Default::default()),
            inter: RwLock::new(StringInterner::new()),
            deps_stats: DoubleCheckedCell::new(),
            indexed_crates,
            crates_index_path,
        })
    }

    pub async fn update(&self) {
        let path = self.crates_index_path.to_owned();
        tokio::task::spawn_blocking(move || {
            info!("Updating crates index");
            let _ = crates_index::Index::with_path(path, "https://github.com/rust-lang/crates.io-index")
                .and_then(|mut idx| idx.update())
                .map_err(|e| error!("crates index update error: {}", e));
            info!("Done updating crates index");
        }).await.expect("spawn fail");
    }

    /// Crates available in the crates.io index
    ///
    /// It returns only a thin and mostly useless data from the index itself,
    /// so `rich_crate`/`rich_crate_version` is needed to do more.
    pub fn crates_io_crates(&self) -> &HashMap<SmolStr, Crate> {
        &self.indexed_crates
    }

    pub fn crate_exists(&self, origin: &Origin) -> bool {
        match origin {
            Origin::CratesIo(lowercase_name) => self.crates_io_crate_by_lowercase_name(lowercase_name).is_ok(),
            Origin::GitHub { .. } | Origin::GitLab { .. } => self.git_index.has(origin),
        }
    }

    /// All crates available in the crates.io index and our index
    ///
    pub fn all_crates(&self) -> impl Iterator<Item = Origin> + '_ {
        self.git_index.crates().cloned().chain(self.crates_io_crates().keys().map(|n| Origin::from_crates_io_name(n)))
    }

    pub async fn deps_stats(&self) -> Result<&DepsStats, DepsErr> {
        tokio::time::timeout(Duration::from_secs(60), self.deps_stats.get_or_init(async {
            tokio::task::yield_now().await;
            tokio::task::block_in_place(|| {
                self.get_deps_stats()
            })
        }))
        .await
        .map_err(|_| DepsErr::DepsNotAvailable)
    }

    #[inline]
    pub fn crates_io_crate_by_lowercase_name(&self, name: &str) -> Result<&Crate, DepsErr> {
        debug_assert_eq!(name, name.to_ascii_lowercase());
        self.crates_io_crates()
        .get(name)
        .ok_or_else(|| DepsErr::CrateNotFound(Origin::from_crates_io_name(name)))
    }

    /// Changes when crates-io metadata changes (something is published or yanked)
    pub fn cache_key_for_crate(&self, name: &str) -> Result<u64, DepsErr> {
        use std::hash::Hash;
        use std::hash::Hasher;
        let mut hasher = fxhash::FxHasher::default();

        let c = self.crates_io_crate_by_lowercase_name(name)?;
        for v in c.versions() {
            v.checksum().hash(&mut hasher);
            v.is_yanked().hash(&mut hasher);
        }
        Ok(hasher.finish())
    }

    pub fn crate_highest_version(&self, name: &str, stable_only: bool) -> Result<&Version, DepsErr> {
        debug_assert_eq!(name, name.to_ascii_lowercase());
        Ok(Self::highest_crates_io_version(self.crates_io_crate_by_lowercase_name(name)?, stable_only))
    }

    fn highest_crates_io_version(krate: &Crate, stable_only: bool) -> &Version {
        krate.versions()
            .iter()
            .max_by_key(|a| {
                let ver = SemVer::parse(a.version())
                    .map_err(|e| info!("{} has invalid version {}: {}", krate.name(), a.version(), e))
                    .ok();
                let bad = a.is_yanked() || ver.is_none() || (stable_only && !ver.as_ref().map_or(false, |v| v.pre.is_empty()));
                (!bad, ver)
            })
            .unwrap_or_else(|| krate.most_recent_version()) // latest_version = most recently published version
    }

    pub(crate) fn deps_of_crate(&self, krate: &impl ICrate, query: DepQuery) -> Result<Dep, DepsErr> {
        let (latest, features) = krate.latest_version_with_features(query.all_optional);
        self.deps_of_crate_int(latest, features, query)
    }

    fn deps_of_crate_int(&self, latest: &impl IVersion, features: Box<[SmolStr]>, DepQuery { default, all_optional, dev }: DepQuery) -> Result<Dep, DepsErr> {
        Ok(Dep {
            semver: latest.version().try_into()
                .map_err(|_| DepsErr::SemverParsingError(format!("{}: {}", latest.name(), latest.version())))?,
            runtime: self.deps_of_ver(latest, Features {
                all_targets: all_optional,
                default,
                build: false,
                dev,
                features: features.clone(),
            })?,
            build: self.deps_of_ver(latest, Features {
                all_targets: all_optional,
                default,
                build: true,
                dev,
                features,
            })?,
        })
    }

    pub(crate) fn deps_of_ver(&self, ver: &impl IVersion, wants: Features) -> Result<ArcDepSet, DepsErr> {
        let mut key = SmolStr::new();
        key.push_str(ver.name());
        key.push('-');
        key.push_str(ver.version());
        let key = (key, wants);

        if let Some(cached) = self.cache.get(&key) {
            return Ok(Arc::clone(&cached));
        }
        let (key_id_part, wants) = key;

        let ver_features = ver.features(); // available features
        let mut to_enable = HashMap::with_capacity_and_hasher(wants.features.len(), Default::default());
        let all_wanted_features = wants.features.iter().map(|s| s.as_str())
                        .chain(if wants.default { Some("default") } else { None });
        for feat in all_wanted_features {
            if let Some(feature_descriptors) = ver_features.get_feature_descriptors(feat) {
                for feat_des in feature_descriptors {
                    let mut t = feat_des.splitn(2, '/');
                    let dep_descriptor = t.next().unwrap();
                    let subfeatures = t.next();
                    let dep_name = dep_descriptor.trim_start_matches("dep:").trim_end_matches('?');
                    let enabled = to_enable.entry(unicase::Ascii::new(dep_name))
                        .or_insert_with(HashSet::default);
                    if let Some(subfeatures) = subfeatures {
                        enabled.insert(subfeatures);
                    }
                }
            } else {
                to_enable.entry(unicase::Ascii::new(feat)).or_insert_with(HashSet::default);
            }
        }

        let deps = ver.dependencies();
        let mut set: HashMap<DepName, (_, _, HashSet<&str>)> = HashMap::with_capacity_and_hasher(60, Default::default());
        let mut iter1;
        let mut iter2;
        let deps: &mut dyn Iterator<Item = _> = match deps {
            Fudge::CratesIo(dep) => {
                iter1 = dep.iter().map(|d| {
                    (d.crate_name(), d.kind(), d.target().is_some(), d.is_optional(), d.requirement(), d.has_default_features(), d.features())
                });
                &mut iter1
            },
            Fudge::Manifest((ref run, ref dev, ref build)) => {
                iter2 = run.iter().map(|r| (r, DependencyKind::Normal))
                .chain(dev.iter().map(|r| (r, DependencyKind::Dev)))
                .chain(build.iter().map(|r| (r, DependencyKind::Build)))
                .map(|(r, kind)| {
                    (&*r.package, kind, !r.only_for_targets.is_empty(), r.is_optional(), r.dep.req(), true, &r.with_features[..])
                });
                &mut iter2
            },
        };
        for (crate_name, kind, target_specific, is_optional, requirement, has_default_features, features) in deps {
            // people forget to include winapi conditionally
            let is_target_specific = target_specific || crate_name == "winapi";
            if !wants.all_targets && is_target_specific {
                continue; // FIXME: allow common targets?
            }
            // hopefully nobody uses clippy at runtime, they just fail to make it dev dep
            if !wants.dev && is_optional && crate_name == "clippy" {
                continue;
            }

            match kind {
                DependencyKind::Normal => (),
                DependencyKind::Build if wants.build => (),
                DependencyKind::Dev if wants.dev => (),
                _ => continue,
            }


            let enable_dep_features = to_enable.get(&unicase::Ascii::new(crate_name));
            if is_optional && enable_dep_features.is_none() {
                continue;
            }

            let req = VersionReq::parse(requirement)
                .unwrap_or_else(|e| {
                    warn!("{crate_name} = {requirement}: {e}");
                    VersionReq::parse("*").unwrap()
                });

            let tmp;
            let crate_name: &str = if crate_name.bytes().all(|c| c.is_ascii_lowercase() || c == b'-' || c == b'_') { crate_name } else { tmp = crate_name.to_ascii_lowercase(); &tmp };
            debug_assert_eq!(crate_name, crate_name.to_ascii_lowercase());
            let krate = match self.crates_io_crate_by_lowercase_name(crate_name) {
                Ok(k) => k,
                Err(e) => {
                    info!("{}@{} depends on missing crate {} (@{}): {}", ver.name(), ver.version(), crate_name, req, e);
                    continue;
                },
            };
            let (matched, _) = krate.versions().iter().rev()
                .filter(|v| !v.is_yanked())
                .filter_map(|v| Some((v, SemVer::parse(v.version()).ok()?)))
                .find(|(_, semver)| {
                    req.matches(semver)
                })
                .unwrap_or_else(|| {
                    let fallback = krate.most_recent_version(); // bad version, but it shouldn't happen anyway
                    let semver = semver_parse(fallback.version());
                    (fallback, semver)
                });

            let key = {
                let mut inter = self.inter.write();
                (inter.get_or_intern(crate_name), inter.get_or_intern(matched.version()))
            };

            let (_, _, all_features) = set.entry(key)
                .or_insert_with(|| (has_default_features, matched, HashSet::default()));
            all_features.extend(features.iter().map(|s| s.as_str()));
            if let Some(s) = enable_dep_features {
                all_features.extend(s.iter().copied());
            }
        }

        // break infinite recursion. Must be inserted first, since depth-first search
        // may end up requesting it.
        let key = (key_id_part.clone(), wants.clone());
        self.cache.insert(key, Default::default());

        let set: Result<_,_> = set.into_iter().map(|(k, (has_default_features, matched, all_features))| {
            let all_features = all_features.into_iter().map(Into::into).collect::<Vec<_>>().into_boxed_slice();
            let runtime = self.deps_of_ver(matched, Features {
                all_targets: wants.all_targets,
                build: false,
                dev: false, // dev is only for top-level
                default: has_default_features,
                features: all_features.clone(),
            })?;
            let build = self.deps_of_ver(matched, Features {
                all_targets: wants.all_targets,
                build: true,
                dev: false, // dev is only for top-level
                default: has_default_features,
                features: all_features,
            })?;
            Ok((k, Dep {
                semver: matched.version().try_into().unwrap(),
                runtime,
                build,
            }))
        }).collect();

        let result: ArcDepSet = Arc::new(set?);
        let key = (key_id_part, wants);
        self.cache.insert(key, Arc::clone(&result));

        Ok(result)
    }

    pub fn clear_cache(&self) {
        self.cache.clear();
        *self.inter.write() = StringInterner::new();
    }

    /// For crate being outdated. Returns (is_latest, popularity)
    /// 0 = not used *or deprecated*
    /// 1 = everyone uses it
    pub async fn version_popularity(&self, crate_name: &str, requirement: &VersionReq) -> Result<Option<(bool, f32)>, DepsErr> {
        if is_deprecated_requirement(crate_name, requirement) {
            return Ok(Some((false, 0.)));
        }

        let stats = self.deps_stats().await?;
        let krate = self.crates_io_crate_by_lowercase_name(&crate_name.to_ascii_lowercase())?;

        fn matches(ver: &Version, req: &VersionReq) -> bool {
            ver.version().parse().ok().map_or(false, |ver| req.matches(&ver))
        }

        let matches_latest = matches(Self::highest_crates_io_version(krate, true), requirement) ||
            // or match latest unstable
            matches(Self::highest_crates_io_version(krate, false), requirement);

        let pop = stats.counts.get(crate_name)
        .map(|stats| {
            let mut matches = 0;
            let mut matches_max = 0;
            let mut unmatches = 0;
            let mut unmatches_max = 0;
            for (ver, count) in &stats.versions {
                if requirement.matches(&ver.to_semver()) {
                    matches += count; // TODO: this should be (slighly) weighed by crate's popularity?
                    matches_max = matches_max.max(*count);
                } else {
                    unmatches += count;
                    unmatches_max = unmatches_max.max(*count);
                }
            }
            matches += 1; // one to denoise unpopular crates; div/0
            matches_max += 1; // one to denoise unpopular crates; div/0

            let real_ratio = matches as f32 / (matches + unmatches) as f32;

            // when major versions are shifting in popularity, matching second-most-popular crate
            // is still not that terrible.
            // This also helps with crates that have very fragmented users and no
            // version is popular.
            let ratio_of_maxes = matches_max as f32 / (matches_max + unmatches_max) as f32;

            (real_ratio + ratio_of_maxes) * 0.5
        })
        .unwrap_or(0.);

        Ok(Some((matches_latest, pop)))
    }

    /// How likely it is that this exact crate will be installed in any project
    pub async fn version_global_popularity(&self, crate_name: &str, version: &MiniVer) -> Result<Option<f32>, DepsErr> {
        match crate_name {
            // bindings' SLoC looks heavier than actual overhead of standard system libs
            "libc" | "winapi" | "kernel32-sys" | "winapi-i686-pc-windows-gnu" | "winapi-x86_64-pc-windows-gnu" => return Ok(Some(0.99)),
            _ => {},
        }

        let stats = self.deps_stats().await?;
        Ok(stats.counts.get(crate_name)
        .and_then(|c| {
            c.versions.get(version)
            .map(|&ver| ver as f32 / stats.total as f32)
        }))
    }
}

use std::fmt;
impl fmt::Debug for Dep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Dep {{ {}, runtime: x{}, build: x{} }}", self.semver, self.runtime.len(), self.build.len())
    }
}

fn semver_parse(ver: &str) -> SemVer {
    SemVer::parse(ver).unwrap_or_else(|_| SemVer::parse("0.0.0").expect("must parse"))
}

impl From<SemVer> for MiniVer {
    fn from(s: SemVer) -> Self {
        Self::try_from(s.to_string().as_str()).unwrap_or_default()
    }
}

impl TryFrom<&str> for MiniVer {
    type Error = SemVerError;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        let s: old_semver::Version = s.parse()?;
        Ok(Self {
            major: s.major as u16,
            minor: s.minor as u16,
            patch: s.patch as u16,
            pre: s.pre.into_boxed_slice(),
            build: if let Some(old_semver::Identifier::Numeric(m)) = s.build.get(0) { *m as u16 } else { 0 },
        })
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
    pub features: Box<[SmolStr]>,
}

pub type DepName = (Sym, Sym);
pub type DepSet = HashMap<DepName, Dep>;
pub type ArcDepSet = Arc<DepSet>;

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
