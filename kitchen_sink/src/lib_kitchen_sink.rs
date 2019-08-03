#[macro_use] extern crate failure;

#[macro_use]
extern crate serde_derive;

mod index;
pub use crate::index::*;
mod yearly;
pub use crate::yearly::*;
mod deps_stats;
pub use crate::deps_stats::*;

mod git_crates_index;

mod ctrlcbreak;
pub use crate::ctrlcbreak::*;

pub use crates_index::Crate as CratesIndexCrate;
pub use crates_io_client::CrateDependency;
pub use crates_io_client::CrateDepKind;
pub use crates_io_client::CrateMetaVersion;
pub use crates_io_client::CratesIoCrate;
pub use crates_io_client::OwnerKind;
pub use github_info::User;
pub use github_info::UserOrg;
pub use github_info::UserType;
pub use rich_crate::Edition;
pub use rich_crate::Include;
pub use rich_crate::MaintenanceStatus;
pub use rich_crate::Markup;
pub use rich_crate::Origin;
pub use rich_crate::RichCrate;
pub use rich_crate::RichCrateVersion;
pub use rich_crate::RichDep;
pub use rich_crate::{Cfg, Target};
pub use semver::Version as SemVer;

use cargo_toml::Manifest;
use cargo_toml::Package;
use categories::Category;
use chrono::DateTime;
use chrono::prelude::*;
use crate_db::{CrateDb, RepoChange, CrateVersionData};
use crate_files::CrateFile;
use crates_index::Version;
use crates_io_client::CrateOwner;
use failure::ResultExt;
use fxhash::FxHashMap;
use github_info::GitCommitAuthor;
use github_info::GitHubRepo;
use itertools::Itertools;
use lazyonce::LazyOnce;
use rayon::prelude::*;
use repo_url::Repo;
use repo_url::RepoHost;
use repo_url::SimpleRepo;
use rich_crate::Author;
use rich_crate::CrateVersion;
use rich_crate::Derived;
use rich_crate::DownloadWeek;
use rich_crate::Readme;
use semver::VersionReq;
use simple_cache::TempCache;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::hash_map::Entry::*;
use std::collections::HashMap;
use std::collections::HashSet;
use std::env;
use std::mem;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::RwLock;

pub type CError = failure::Error;
pub type CResult<T> = Result<T, CError>;
pub type Warnings = HashSet<Warning>;

#[derive(Debug, Clone, Serialize, Fail, Deserialize, Hash, Eq, PartialEq)]
pub enum Warning {
    #[fail(display = "`Cargo.toml` doesn't have `repository` property")]
    NoRepositoryProperty,
    #[fail(display = "`Cargo.toml` doesn't have `[package]` section")]
    NotAPackage,
    #[fail(display = "`Cargo.toml` doesn't have `readme` property")]
    NoReadmeProperty,
    #[fail(display = "Can't find README in repository: {}", _0)]
    NoReadmeInRepo(String),
    #[fail(display = "Could not clone repository: {}", _0)]
    ErrorCloning(String),
    #[fail(display = "{} URL is a broken link: {}", _0, _1)]
    BrokenLink(String, String),
}

#[derive(Debug, Clone, Fail)]
pub enum KitchenSinkErr {
    #[fail(display = "git checkout meh")]
    GitCheckoutFailed,
    #[fail(display = "category not found: {}", _0)]
    CategoryNotFound(String),
    #[fail(display = "category query failed")]
    CategoryQueryFailed,
    #[fail(display = "crate not found: {:?}", _0)]
    CrateNotFound(Origin),
    #[fail(display = "crate is not a package: {:?}", _0)]
    NotAPackage(Origin),
    #[fail(display = "data not found, wanted {}", _0)]
    DataNotFound(String),
    #[fail(display = "crate has no versions")]
    NoVersions,
    #[fail(display = "Environment variable CRATES_DATA_DIR is not set.\nChoose a dir where it's OK to store lots of data, and export it like CRATES_DATA_DIR=/var/lib/crates.rs")]
    CratesDataDirEnvVarMissing,
    #[fail(display = "{} does not exist\nPlease get data files from https://lib.rs/data and put them in that directory, or set CRATES_DATA_DIR to their location.", _0)]
    CacheDbMissing(String),
    #[fail(display = "Error when parsing verison")]
    SemverParsingError,
    #[fail(display = "Stopped")]
    Stopped,
    #[fail(display = "Missing github login for crate owner")]
    OwnerWithoutLogin,
    #[fail(display = "Git index parsing failed: {}", _0)]
    GitIndexParse(String),
    #[fail(display = "Git index {:?}: {}", _0, _1)]
    GitIndexFile(PathBuf, String),
    #[fail(display = "Rayon deadlock broke the stats")]
    DepsStatsNotAvailable,
}

/// This is a collection of various data sources. It mostly acts as a starting point and a factory for other objects.
pub struct KitchenSink {
    pub index: Index,
    crates_io: crates_io_client::CratesIoClient,
    docs_rs: docs_rs_client::DocsRsClient,
    url_check_cache: TempCache<bool>,
    crate_db: CrateDb,
    user_db: user_db::UserDb,
    gh: github_info::GitHub,
    crate_derived_cache: TempCache<(String, RichCrateVersionCacheData, Warnings)>,
    loaded_rich_crate_version_cache: RwLock<FxHashMap<Box<str>, RichCrateVersion>>,
    category_crate_counts: LazyOnce<Option<HashMap<String, u32>>>,
    removals: LazyOnce<HashMap<Origin, f64>>,
    top_crates_cached: RwLock<FxHashMap<String, Arc<Vec<Origin>>>>,
    git_checkout_path: PathBuf,
    main_cache_dir: PathBuf,
    yearly: AllDownloads,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct RichCrateVersionCacheData {
    derived: Derived,
    manifest: Manifest,
    readme: Option<Readme>,
    lib_file: Option<String>,
    path_in_repo: Option<String>,
    has_buildrs: bool,
    has_code_of_conduct: bool,
}

impl KitchenSink {
    /// Use env vars to find data directory and config
    pub fn new_default() -> CResult<Self> {
        let github_token = match env::var("GITHUB_TOKEN") {
            Ok(t) => t,
            Err(_) => {
                eprintln!("warning: Environment variable GITHUB_TOKEN is not set.\nGet token from https://github.com/settings/tokens and export GITHUB_TOKEN=…\nWithout it some requests will fail and new crates won't be analyzed properly.");
                "".to_owned()
            },
        };
        let data_path = Self::data_path()?;
        Self::new(&data_path, &github_token, 1.)
    }

    pub fn new(data_path: &Path, github_token: &str, crates_io_tps: f32) -> CResult<Self> {
        let main_cache_dir = data_path.to_owned();

        let ((crates_io, gh), (index, crate_derived_cache)) = rayon::join(|| rayon::join(
                || crates_io_client::CratesIoClient::new(data_path, crates_io_tps),
                || github_info::GitHub::new(&data_path.join("github.db"), github_token)),
            || rayon::join(
                || Index::new(data_path),
                || TempCache::new(&data_path.join("crate_derived.db"))));
        Ok(Self {
            crates_io: crates_io.context("cratesio")?,
            index: index.context("index")?,
            url_check_cache: TempCache::new(&data_path.join("url_check.db")).context("urlcheck")?,
            docs_rs: docs_rs_client::DocsRsClient::new(data_path.join("docsrs.db")).context("docs")?,
            crate_db: CrateDb::new(Self::assert_exists(data_path.join("crate_data.db"))?).context("db")?,
            user_db: user_db::UserDb::new(Self::assert_exists(data_path.join("users.db"))?).context("udb")?,
            gh: gh.context("gh")?,
            crate_derived_cache: crate_derived_cache.context("derived")?,
            loaded_rich_crate_version_cache: RwLock::new(FxHashMap::default()),
            git_checkout_path: data_path.join("git"),
            category_crate_counts: LazyOnce::new(),
            removals: LazyOnce::new(),
            top_crates_cached: RwLock::new(FxHashMap::default()),
            yearly: AllDownloads::new(&main_cache_dir),
            main_cache_dir,
        })
    }

    fn assert_exists(path: PathBuf) -> Result<PathBuf, KitchenSinkErr> {
        if !path.exists() {
            Err(KitchenSinkErr::CacheDbMissing(path.display().to_string()))
        } else {
            Ok(path)
        }
    }

    pub(crate) fn data_path() -> Result<PathBuf, KitchenSinkErr> {
        match env::var("CRATES_DATA_DIR") {
            Ok(d) => {
                if !Path::new(&d).join("crate_data.db").exists() {
                    return Err(KitchenSinkErr::CacheDbMissing(d));
                }
                Ok(d.into())
            },
            Err(_) => {
                for path in &["../data", "./data", "/var/lib/crates.rs/data", "/www/crates.rs/data"] {
                    let path = Path::new(path);
                    if path.exists() && path.join("crate_data.db").exists() {
                        return Ok(path.to_owned());
                    }
                }
                Err(KitchenSinkErr::CratesDataDirEnvVarMissing)
            },
        }
    }

    pub fn main_cache_dir(&self) -> &Path {
        &self.main_cache_dir
    }

    /// Don't make requests to crates.io
    pub fn cache_only(&mut self, no_net: bool) -> &mut Self {
        self.crates_io.cache_only(no_net);
        self
    }

    /// Iterator over all crates available in the index
    ///
    /// It returns only identifiers,
    /// so `rich_crate`/`rich_crate_version` is needed to do more.
    pub fn all_crates(&self) -> impl Iterator<Item=&Origin> {
        self.index.all_crates()
    }

    /// Iterator over all crates available in the index
    ///
    /// It returns only identifiers,
    /// so `rich_crate`/`rich_crate_version` is needed to do more.
    pub fn all_crates_io_crates(&self) -> &FxHashMap<Origin, CratesIndexCrate> {
        self.index.crates_io_crates()
    }

    /// Gets cratesio download data, but not from the API, but from our local copy
    pub fn weekly_downloads(&self, k: &RichCrate, num_weeks: u16) -> CResult<Vec<DownloadWeek>> {
        let mut res = Vec::with_capacity(num_weeks.into());
        let mut now = Utc::today();

        let mut curr_year = now.year();
        let mut curr_year_data = self.yearly.get_crate_year(k.name(), curr_year as _)?.unwrap_or_default();

        let day_of_year = now.ordinal0();
        let missing_data_days = curr_year_data.is_set[0..day_of_year as usize].iter().cloned().rev().take_while(|s| !s).count();

        if missing_data_days > 0 {
            now = now - chrono::Duration::days(missing_data_days as _);
        }

        for i in (0..num_weeks).rev() {
            let date = now - chrono::Duration::weeks(i.into());
            let mut total = 0;
            let mut any_set = false;

            for d in 0..7 {
                let this_date = date + chrono::Duration::days(d);
                let day_of_year = this_date.ordinal0() as usize;
                let year = this_date.year();
                if year != curr_year {
                    curr_year = year;
                    curr_year_data = self.yearly.get_crate_year(k.name(), curr_year as _)?.unwrap_or_default();
                }
                if curr_year_data.is_set[day_of_year] {
                    any_set = true;
                }
                for dl in curr_year_data.versions.values() {
                    total += dl.0[day_of_year] as usize;
                }
            }
            if any_set {
                res.push(DownloadWeek {
                    date,
                    total,
                    downloads: HashMap::new(), // format of this is stupid, as it requires crates.io's version IDs
                });
            }
        }
        Ok(res)
    }

    pub fn all_new_crates<'a>(&'a self) -> CResult<impl Iterator<Item = RichCrate> + 'a> {
        let min_timestamp = self.crate_db.latest_crate_update_timestamp()?.unwrap_or(0);
        let all: Vec<_> = self.index.all_crates().collect();
        let res: Vec<RichCrate> = all.into_par_iter()
        .filter_map(move |o| {
            self.rich_crate(o).map_err(|e| eprintln!("{:?}: {}", o, e)).ok()
        })
        .filter(move |k| {
            let latest = k.versions().iter().map(|v| v.created_at.as_str()).max().unwrap_or("");
            if let Ok(timestamp) = DateTime::parse_from_rfc3339(latest) {
                timestamp.timestamp() >= min_timestamp as i64
            } else {
                eprintln!("Can't parse {} of {}", latest, k.name());
                true
            }
        }).collect();
        Ok(res.into_iter())
    }

    pub fn crate_exists(&self, origin: &Origin) -> bool {
        match origin {
            Origin::CratesIo(_) => {
                self.index.crates_io_crate_by_name(origin).is_ok()
            },
            _ => true,
        }
    }

    /// Wrapper object for metadata common for all versions of a crate
    pub fn rich_crate(&self, origin: &Origin) -> CResult<RichCrate> {
        if stopped() {Err(KitchenSinkErr::Stopped)?;}
        match origin {
            Origin::CratesIo(_) => {
                let meta = self.crates_io_meta(origin, false)?;
                let versions = meta.meta.versions().map(|c| CrateVersion {
                    num: c.num,
                    updated_at: c.updated_at,
                    created_at: c.created_at,
                    yanked: c.yanked,
                }).collect();
                Ok(RichCrate::new(origin.clone(), meta.owners, meta.meta.krate.name, versions))
            },
            Origin::GitHub {repo, package} => {
                Ok(RichCrate::new(origin.clone(), vec![
                    CrateOwner {
                        id: 0,
                        login: repo.owner.to_string(),
                        kind: OwnerKind::User, // FIXME: not really true if this is an org
                        url: format!("https://github.com/{}", repo.owner),
                        name: None,
                        avatar: None,
                    }
                ],
                format!("{}/{}/{}", repo.owner, repo.repo, package),
                vec![]))
            }
        }
    }

    /// Fudge-factor score proprtional to how many times a crate has been removed from some project
    pub fn crate_removals(&self, origin: &Origin) -> Option<f64> {
        self.removals
            .get(|| self.crate_db.removals().expect("fetch crate removals"))
            .get(origin).cloned()
    }

    pub fn downloads_per_month(&self, origin: &Origin) -> CResult<Option<usize>> {
        self.downloads_recent(origin).map(|dl| dl.map(|n| n/3))
    }

    pub fn downloads_per_month_or_equivalent(&self, origin: &Origin) -> CResult<Option<usize>> {
        if let Some(dl) = self.downloads_per_month(origin)? {
            return Ok(Some(dl));
        }

        match origin {
            Origin::GitHub {repo, ..} => {
                let repo = Repo::new(&format!("https://github.com/{}/{}", repo.owner, repo.repo))?;
                if let Some(gh) = self.github_repo(&repo)? {
                    // arbitrary multiplier. TODO: it's not fair for apps vs libraries
                    return Ok(Some(gh.stargazers_count as usize * 100));
                }
            },
            _ => {},
        }
        Ok(None)
    }

    fn downloads_recent(&self, origin: &Origin) -> CResult<Option<usize>> {
        Ok(match origin {
            Origin::CratesIo(_) => {
                let meta = self.crates_io_meta(origin, true)?;
                meta.meta.krate.recent_downloads
            },
            _ => None,
        })
    }

    fn crates_io_meta(&self, origin: &Origin, refresh: bool) -> CResult<CratesIoCrate> {
        let krate = self.index.crates_io_crate_by_name(origin).context("rich_crate")?;
        let name = krate.name();
        let latest_in_index = krate.latest_version().version(); // most recently published version
        let meta = self.crates_io.krate(name, latest_in_index, refresh)
            .with_context(|_| format!("crates.io meta for {} {}", name, latest_in_index))?;
        let mut meta = meta.ok_or_else(|| KitchenSinkErr::CrateNotFound(Origin::from_crates_io_name(name)))?;
        if !meta.meta.versions.iter().any(|v| v.num == latest_in_index) {
            eprintln!("Crate data missing latest version {:?}@{}", origin, latest_in_index);
            meta = self.crates_io.krate(name, &format!("{}-try-again", latest_in_index), true)?
                .ok_or_else(|| KitchenSinkErr::CrateNotFound(Origin::from_crates_io_name(name)))?;
            if !meta.meta.versions.iter().any(|v| v.num == latest_in_index) {
                eprintln!("Error: crate data is borked {:?}@{}. Has only: {:?}", origin, latest_in_index, meta.meta.versions.iter().map(|v| &v.num).collect::<Vec<_>>());
            }
        }
        Ok(meta)
    }

    /// Wrapper for the latest version of a given crate.
    ///
    /// This function is quite slow, as it reads everything about the crate.
    ///
    /// There's no support for getting anything else than the latest version.
    pub fn rich_crate_version(&self, origin: &Origin, fetch_type: CrateData) -> CResult<RichCrateVersion> {
        if stopped() {Err(KitchenSinkErr::Stopped)?;}
        let ver = self.index.crate_version_latest_unstable(origin).context("rich_crate_version")?;

        self.rich_crate_version_from_index( ver, fetch_type)
    }

    fn rich_crate_version_from_index(&self, krate: &Version, fetch_type: CrateData) -> CResult<RichCrateVersion> {
        let cache_key = format!("{}-{}", krate.name(), krate.version()).into_boxed_str();

        if fetch_type != CrateData::FullNoDerived {
            let cache = self.loaded_rich_crate_version_cache.read().unwrap();
            if let Some(krate) = cache.get(&cache_key) {
                return Ok(krate.clone());
            }
        }

        let krate = self.rich_crate_version_verbose(krate, fetch_type).map(|(krate, _)| krate)?;
        if fetch_type == CrateData::Full {
            self.loaded_rich_crate_version_cache.write().unwrap().insert(cache_key, krate.clone());
        }
        Ok(krate)
    }

    /// With warnings
    pub fn rich_crate_version_verbose(&self, krate: &Version, fetch_type: CrateData) -> CResult<(RichCrateVersion, Warnings)> {
        if stopped() {Err(KitchenSinkErr::Stopped)?;}

        let key = (krate.name(), krate.version());
        let cached = if fetch_type != CrateData::FullNoDerived {
            match self.crate_derived_cache.get(key.0)? {
                Some((ver, res, warn)) => {
                    if key.1 != ver {
                        None
                    } else {
                        Some((res, warn))
                    }
                },
                None => None,
            }
        } else {
            None
        };

        let (d, warn) = if let Some(res) = cached {res} else {
            let (d, warn) = self.rich_crate_version_data(krate, fetch_type).with_context(|_| format!("failed geting rich crate data for {}", key.0))?;
            if fetch_type == CrateData::Full {
                self.crate_derived_cache.set(key.0, (key.1.to_string(), d.clone(), warn.clone()))?;
            } else if fetch_type == CrateData::FullNoDerived {
                self.crate_derived_cache.delete(key.0).context("clear cache 2")?;
            }
            (d, warn)
        };
        Ok((RichCrateVersion::new(krate.clone(), d.manifest, d.derived, d.readme, d.lib_file.map(|s| s.into()), d.path_in_repo, d.has_buildrs, d.has_code_of_conduct), warn))
    }

    pub fn changelog_url(&self, k: &RichCrateVersion) -> Option<String> {
        let repo = k.repository()?;
        if let RepoHost::GitHub(ref gh) = repo.host() {
            let releases = self.gh.releases(gh, &self.cachebust_string_for_repo(repo).ok()?).ok()??;
            if releases.iter().any(|rel| rel.body.as_ref().map_or(false, |b| b.len() > 10)) {
                return Some(format!("https://github.com/{}/{}/releases", gh.owner, gh.repo));
            }
        }
        None
    }

    fn rich_crate_version_data(&self, latest: &crates_index::Version, fetch_type: CrateData) -> CResult<(RichCrateVersionCacheData, Warnings)> {
        let mut warnings = HashSet::new();

        let name = latest.name();
        let ver = latest.version();
        let origin = Origin::from_crates_io_name(name);

        let (crate_tarball, crates_io_meta) = rayon::join(
            || self.crates_io.crate_data(name, ver).context("crate_file"),
            || self.crates_io_meta(&origin, fetch_type == CrateData::FullNoDerived));

        let crates_io_meta = crates_io_meta?.meta.krate;
        let crate_tarball = crate_tarball?.ok_or_else(|| KitchenSinkErr::DataNotFound(format!("{}-{}", name, ver)))?;
        let crate_compressed_size = crate_tarball.len();
        let mut meta = crate_files::read_archive(&crate_tarball[..], name, ver)?;
        drop(crate_tarball);

        let has_buildrs = meta.has("build.rs");
        let has_code_of_conduct = meta.has("CODE_OF_CONDUCT.md") || meta.has("docs/CODE_OF_CONDUCT.md") || meta.has(".github/CODE_OF_CONDUCT.md");

        let mut derived = Derived::default();
        mem::swap(&mut derived.language_stats, &mut meta.language_stats); // move
        derived.crate_compressed_size = crate_compressed_size as u32;
        // sometimes uncompressed sources without junk are smaller than tarball with junk
        derived.crate_decompressed_size = meta.decompressed_size.max(crate_compressed_size) as u32;
        derived.is_nightly = meta.is_nightly;

        let package = meta.manifest.package.as_mut().ok_or_else(|| KitchenSinkErr::NotAPackage(origin.clone()))?;

        // Guess repo URL if none was specified; must be done before getting stuff from the repo
        if package.repository.is_none() {
            warnings.insert(Warning::NoRepositoryProperty);
            // it may contain data from nowhere! https://github.com/rust-lang/crates.io/issues/1624
            if let Some(repo) = crates_io_meta.repository {
                package.repository = Some(repo);
            } else {
                if package.homepage.as_ref().map_or(false, |h| Repo::looks_like_repo_url(h)) {
                    package.repository = package.homepage.take();
                }
            }
        }

        let maybe_repo = package.repository.as_ref().and_then(|r| Repo::new(r).ok());
        let path_in_repo = match maybe_repo.as_ref() {
            Some(repo) => if fetch_type != CrateData::Minimal {
                self.crate_db.path_in_repo(repo, name)?
            } else {
                None
            },
            None => None,
        };

        ///// Fixing and faking the data /////

        // it may contain data from nowhere! https://github.com/rust-lang/crates.io/issues/1624
        if package.homepage.is_none() {
            if let Some(repo) = crates_io_meta.homepage {
                package.homepage = Some(repo);
            }
        }
        if package.documentation.is_none() {
            if let Some(repo) = crates_io_meta.documentation {
                package.documentation = Some(repo);
            }
        }


        let has_readme = meta.readme.is_some();
        if !has_readme {
            warnings.insert(Warning::NoReadmeProperty);
            if fetch_type != CrateData::Minimal {
                warnings.extend(self.add_readme_from_repo(&mut meta, maybe_repo.as_ref()));
                let has_readme = meta.readme.is_some();
                if !has_readme && meta.manifest.package.as_ref().map_or(false, |p| p.readme.is_some()) {
                    // readmes in form of readme="../foo.md" are lost in packaging,
                    // and the only copy exists in crates.io own api
                    self.add_readme_from_crates_io(&mut meta, name, ver);
                }
            }
        }

        let package = meta.manifest.package.as_mut().ok_or_else(|| KitchenSinkErr::NotAPackage(origin.clone()))?;

        // Guess keywords if none were specified
        // TODO: also ignore useless keywords that are unique db-wide
        if package.keywords.is_empty() && fetch_type != CrateData::Minimal {
            let gh = match maybe_repo.as_ref() {
                Some(repo) => if let RepoHost::GitHub(ref gh) = repo.host() {
                    self.gh.topics(gh, &self.cachebust_string_for_repo(repo).context("fetch topics")?)?
                } else {None},
                _ => None,
            };
            if let Some(mut topics) = gh {
                topics.retain(|t| match t.as_str() {
                    "rust" | "rs" | "rustlang" | "rust-lang" | "crate" | "crates" | "library" => false,
                    t if t.starts_with("rust-") => false,
                    _ => true,
                });
                if !topics.is_empty() {
                    derived.github_keywords = Some(topics);
                }
            }
            if derived.github_keywords.is_none() && fetch_type != CrateData::FullNoDerived {
                derived.keywords = Some(self.crate_db.keywords(&origin).context("keywordsdb")?);
            }
        }

        Self::override_bad_categories(package, &meta.manifest.dependencies);

        // Guess categories if none were specified
        if categories::Categories::filtered_category_slugs(&package.categories).next().is_none() && fetch_type == CrateData::Full {
            derived.categories = Some({
                let keywords_iter = package.keywords.iter().map(|s| s.as_str());
                self.crate_db.guess_crate_categories(&origin, keywords_iter).context("catdb")?
                .into_iter().map(|(_, c)| c).collect()
            });
        }

        // Delete the original docs.rs link, because we have our own
        // TODO: what if the link was to another crate or a subpage?
        if package.documentation.as_ref().map_or(false, |s| Self::is_docs_rs_link(s)) {
            if self.has_docs_rs(name, ver) {
                package.documentation = None; // docs.rs is not proper docs
            }
        }

        warnings.extend(self.remove_redundant_links(package, maybe_repo.as_ref()));

        if fetch_type != CrateData::Minimal {
            if let Some(ref crate_repo) = maybe_repo {
                if let Some(ghrepo) = self.github_repo(crate_repo)? {
                    if package.homepage.is_none() {
                        if let Some(url) = ghrepo.github_page_url {
                            package.homepage = Some(url);
                        } else if let Some(url) = ghrepo.homepage {
                            package.homepage = Some(url);
                        }
                        warnings.extend(self.remove_redundant_links(package, maybe_repo.as_ref()));
                    }
                    derived.github_description = ghrepo.description;
                    derived.github_name = Some(ghrepo.name);
                }
            }
        }

        // lib file takes majority of space in cache, so remove it if it won't be used
        if !self.is_readme_short(meta.readme.as_ref()) {
            meta.lib_file = None;
        }

        Ok((RichCrateVersionCacheData {
            derived,
            has_buildrs,
            has_code_of_conduct,
            manifest: meta.manifest,
            readme: meta.readme,
            lib_file: meta.lib_file.map(|s| s.into()),
            path_in_repo,
        }, warnings))
    }

    fn override_bad_categories(package: &mut Package, direct_dependencies: &cargo_toml::DepsSet) {
        for cat in &mut package.categories {
            if cat.as_bytes().iter().any(|c| !c.is_ascii_lowercase()) {
                *cat = cat.to_lowercase();
            }
            if cat == "localization" {
                // nobody knows the difference
                *cat = "internationalization".to_string();
            }
            if cat == "parsers" {
                if direct_dependencies.keys().any(|k| k == "nom" || k == "peresil" || k == "combine") ||
                    package.keywords.iter().any(|k| match k.to_ascii_lowercase().as_ref() {
                        "asn1" | "tls" | "idl" | "crawler" | "xml" | "nom" | "json" | "logs" | "elf" | "uri" | "html" | "protocol" | "semver" | "ecma" |
                        "chess" | "vcard" | "exe" | "fasta" => true,
                        _ => false,
                    })
                {
                    *cat = "parser-implementations".into();
                }
            }
            if cat == "cryptography" || cat == "database" || cat == "rust-patterns" || cat == "development-tools" {
                if package.keywords.iter().any(|k| k == "bitcoin" || k == "ethereum" || k == "ledger" || k == "exonum" || k == "blockchain") {
                    *cat = "cryptography::cryptocurrencies".into();
                }
            }
            if cat == "games" {
                if package.keywords.iter().any(|k| {
                    k == "game-dev" || k == "game-development" || k == "gamedev" || k == "framework" || k == "utilities" || k == "parser" || k == "api"
                }) {
                    *cat = "game-engines".into();
                }
            }
            if cat == "science" || cat == "algorithms" {
                if package.keywords.iter().any(|k| k == "neural-network" || k == "machine-learning" || k == "deep-learning") {
                    *cat = "science::ml".into();
                } else if package.keywords.iter().any(|k| {
                    k == "math" || k == "calculus" || k == "algebra" || k == "linear-algebra" || k == "mathematics" || k == "maths" || k == "number-theory"
                }) {
                    *cat = "science::math".into();
                }
            }
        }
    }

    fn github_repo(&self, crate_repo: &Repo) -> CResult<Option<GitHubRepo>> {
        Ok(match crate_repo.host() {
            RepoHost::GitHub(ref repo) => {
                let cachebust = self.cachebust_string_for_repo(crate_repo).context("ghrepo")?;
                self.gh.repo(repo, &cachebust)?
            },
            _ => None,
        })
    }

    pub fn is_readme_short(&self, readme: Option<&Readme>) -> bool {
        if let Some(r) = readme {
            match r.markup {
                Markup::Markdown(ref s) | Markup::Rst(ref s) | Markup::Html(ref s) => s.len() < 1000,
            }
        } else {
            true
        }
    }

    pub fn is_build_or_dev(&self, k: &RichCrateVersion) -> (bool, bool) {
        self.dependents_stats_of(k)
        .map(|d| {
            let is_build = d.build.def > 3 * (d.runtime.def + d.runtime.opt + 5);
            let is_dev = !is_build && d.dev > (3 * d.runtime.def + d.runtime.opt + 3 * d.build.def + d.build.opt + 5);
            (is_build, is_dev)
        })
        .unwrap_or((false, false))
    }

    fn add_readme_from_repo(&self, meta: &mut CrateFile, maybe_repo: Option<&Repo>) -> Warnings {
        let mut warnings = HashSet::new();
        let package = match meta.manifest.package.as_ref() {
            Some(p) => p,
            None => {
                warnings.insert(Warning::NotAPackage);
                return warnings;
            },
        };
        if let Some(repo) = maybe_repo {
            let res = crate_git_checkout::checkout(repo, &self.git_checkout_path)
            .map_err(From::from)
            .and_then(|checkout| {
                crate_git_checkout::find_readme(&checkout, package)
            });
            match res {
                Ok(Some(readme)) => {
                    meta.readme = Some(readme);
                },
                Ok(None) => {
                    warnings.insert(Warning::NoReadmeInRepo(repo.canonical_git_url().to_string()));
                },
                Err(err) => {
                    warnings.insert(Warning::ErrorCloning(repo.canonical_git_url().to_string()));
                    eprintln!("Checkout of {} ({}) failed: {}", package.name, repo.canonical_git_url(), err);
                },
            }
        }
        warnings
    }

    fn add_readme_from_crates_io(&self, meta: &mut CrateFile, name: &str, ver: &str) {
        if let Ok(Some(html)) = self.crates_io.readme(name, ver) {
            eprintln!("Found readme on crates.io {}@{}", name, ver);
            meta.readme = Some(Readme {
                markup: Markup::Html(String::from_utf8_lossy(&html).to_string()),
                base_url: None,
                base_image_url: None,
            });
        } else {
            eprintln!("No readme on crates.io for {}@{}", name, ver);
        }
    }

    fn remove_redundant_links(&self, package: &mut Package, maybe_repo: Option<&Repo>) -> Warnings {
        let mut warnings = HashSet::new();

        // We show github link prominently, so if homepage = github, that's nothing new
        let homepage_is_repo = Self::is_same_url(package.homepage.as_ref(), package.repository.as_ref());
        let homepage_is_canonical_repo = maybe_repo
            .and_then(|repo| {
                package.homepage.as_ref()
                .and_then(|home| Repo::new(&home).ok())
                .map(|other| {
                    repo.canonical_git_url() == other.canonical_git_url()
                })
            })
            .unwrap_or(false);

        if homepage_is_repo || homepage_is_canonical_repo {
            package.homepage = None;
        }

        if Self::is_same_url(package.documentation.as_ref(), package.homepage.as_ref()) ||
           Self::is_same_url(package.documentation.as_ref(), package.repository.as_ref()) ||
           maybe_repo.map_or(false, |repo| Self::is_same_url(Some(&*repo.canonical_http_url("")), package.documentation.as_ref())) {
            package.documentation = None;
        }

        if package.homepage.as_ref().map_or(false, |d| Self::is_docs_rs_link(d) || d.starts_with("https://lib.rs/") || d.starts_with("https://crates.io/")) {
            package.homepage = None;
        }

        if package.homepage.as_ref().map_or(false, |url| !self.check_url_is_valid(url)) {
            warnings.insert(Warning::BrokenLink("homepage".to_string(), package.homepage.as_ref().unwrap().to_string()));
            package.homepage = None;
        }

        if package.documentation.as_ref().map_or(false, |url| !self.check_url_is_valid(url)) {
            warnings.insert(Warning::BrokenLink("documentation".to_string(), package.documentation.as_ref().unwrap().to_string()));
            package.documentation = None;
        }
        warnings
    }

    pub fn check_url_is_valid(&self, url: &str) -> bool {
        if let Ok(Some(res)) = self.url_check_cache.get(url) {
            return res;
        }
        eprintln!("CHK: {}", url);
        let res = reqwest::Client::builder().build()
        .and_then(|res| res.get(url).send())
        .map(|res| {
            res.status().is_success()
        })
        .unwrap_or(false);
        self.url_check_cache.set(url, res).unwrap();
        return res;
    }

    fn is_docs_rs_link(d: &str) -> bool {
        let d = d.trim_start_matches("http://").trim_start_matches("https://");
        d.starts_with("docs.rs/") || d.starts_with("crates.fyi/")
    }

    pub fn has_docs_rs(&self, name: &str, ver: &str) -> bool {
        self.docs_rs.builds(name, ver).unwrap_or(true) // fail open
    }

    fn is_same_url<A: AsRef<str> + std::fmt::Debug>(a: Option<A>, b: Option<&String>) -> bool {
        fn trim(s: &str) -> &str {
            let s = s.trim_start_matches("http://").trim_start_matches("https://");
            s.split('#').next().unwrap().trim_end_matches("/index.html").trim_end_matches('/')
        }

        match (a, b) {
            (Some(ref a), Some(ref b)) if trim(a.as_ref()).eq_ignore_ascii_case(trim(b)) => true,
            _ => false,
        }
    }

    pub fn all_dependencies_flattened(&self, origin: &Origin) -> Result<DepInfMap, KitchenSinkErr> {
        self.index.all_dependencies_flattened(self.index.crates_io_crate_by_name(origin)?)
    }

    pub fn prewarm(&self) {
        let _ = self.index.deps_stats();
    }

    pub fn update(&self) {
        self.index.update();
        let _ = self.index.deps_stats();
    }

    pub fn dependents_stats_of(&self, krate: &RichCrateVersion) -> Option<RevDependencies> {
        self.dependents_stats_of_crates_io_crate(krate.short_name())
    }

    pub fn dependents_stats_of_crates_io_crate(&self, crate_name: &str) -> Option<RevDependencies> {
        self.index.deps_stats()?.counts.get(crate_name).cloned()
    }

    /// (latest, pop)
    /// 0 = not used
    /// 1 = everyone uses it
    pub fn version_popularity(&self, crate_name: &str, requirement: &VersionReq) -> Option<(bool, f32)> {
        self.index.version_popularity(crate_name, requirement)
    }

    /// "See also"
    pub fn related_categories(&self, slug: &str) -> CResult<Vec<String>> {
        self.crate_db.related_categories(slug)
    }

    /// Recommendations
    pub fn related_crates(&self, krate: &RichCrateVersion, min_recent_downloads: u32) -> CResult<Vec<Origin>> {
        let (replacements, related) = rayon::join(
            || self.crate_db.replacement_crates(krate.short_name()).context("related_crates1"),
            || self.crate_db.related_crates(krate.origin(), min_recent_downloads).context("related_crates2"),
        );

        Ok(replacements?.into_iter()
            .map(|name| Origin::from_crates_io_name(&name))
            .chain(related?)
            .unique()
            .take(10)
            .collect())

    }

    /// Returns (nth, slug)
    pub fn top_category<'crat>(&self, krate: &'crat RichCrateVersion) -> Option<(u32, Cow<'crat, str>)> {
        let crate_origin = krate.origin();
        krate.category_slugs(Include::Cleaned)
        .filter_map(|slug| {
            self.top_crates_in_category(&slug).ok()
            .and_then(|cat| {
                cat.iter().position(|o| o == crate_origin).map(|pos| {
                    (pos as u32 +1, slug)
                })
            })
        })
        .min_by_key(|a| a.0)
    }

    /// Returns (nth, keyword)
    pub fn top_keyword(&self, krate: &RichCrate) -> CResult<Option<(u32, String)>> {
        Ok(self.crate_db.top_keyword(&krate.origin())?)
    }

    /// Maintenance: add user to local db index
    pub fn index_user(&self, user: &User, commit: &GitCommitAuthor) -> CResult<()> {
        if stopped() {Err(KitchenSinkErr::Stopped)?;}
        if !self.user_db.email_has_github(&commit.email)? {
            println!("{} => {}", commit.email, user.login);
            self.user_db.index_user(&user, Some(&commit.email), commit.name.as_ref().map(|s| s.as_str()))?;
        }
        Ok(())
    }

    /// Maintenance: add user to local db index
    pub fn index_email(&self, email: &str, name: Option<&str>) -> CResult<()> {
        if stopped() {Err(KitchenSinkErr::Stopped)?;}
        if !self.user_db.email_has_github(&email)? {
            match self.gh.user_by_email(&email) {
                Ok(Some(users)) => {
                    for user in users {
                        println!("{} == {} ({:?})", user.login, email, name);
                        self.user_db.index_user(&user, Some(email), name)?;
                    }
                },
                Ok(None) => println!("{} not found on github", email),
                Err(e) => eprintln!("•••• {}", e),
            }
        }
        Ok(())
    }

    /// Maintenance: add crate to local db index
    pub fn index_crate(&self, k: &RichCrate, score: f64) -> CResult<()> {
        if stopped() {Err(KitchenSinkErr::Stopped)?;}
        let (res1, res2) = rayon::join(|| -> CResult<()> {
            let origin = k.origin();
            match origin {
                Origin::CratesIo(name) => {
                    let meta = self.crates_io_meta(origin, true)?;
                    self.index_crate_downloads(name, &meta)?;
                },
                _ => {},
            }
            Ok(())
        }, || self.crate_db.index_versions(k, score, self.downloads_recent(k.origin())?));
        res1?;
        res2?;
        Ok(())
    }

    pub fn index_crate_downloads(&self, crates_io_name: &str, crates_io_data: &CratesIoCrate) -> CResult<()> {
        let mut modified = false;
        let mut curr_year = Utc::today().year() as u16;
        let mut curr_year_data = self.yearly.get_crate_year(crates_io_name, curr_year)?.unwrap_or_default();

        let dd = crates_io_data.daily_downloads();
        let mut by_day = FxHashMap::<_, Vec<_>>::default();
        for dl in dd {
            by_day.entry(dl.date).or_insert_with(Default::default).push(dl);
        }
        // The latest day in the data is going to be incomplete, so throw it out
        if let Some(max_date) = by_day.keys().max().cloned() {
            by_day.remove(&max_date);
        }
        for (day, dls) in by_day {
            let day_of_year = day.ordinal0() as usize;
            let y = day.year() as u16;
            if y != curr_year {
                if modified {
                    modified = false;
                    self.yearly.set_crate_year(crates_io_name, curr_year, &curr_year_data)?;
                }
                curr_year = y;
                curr_year_data = self.yearly.get_crate_year(crates_io_name, curr_year)?.unwrap_or_default();
            }

            // crates.io data gets worse as it gets older, so the first one was the best
            if curr_year_data.is_set[day_of_year] {
                continue;
            }

            modified = true;
            curr_year_data.is_set[day_of_year] = true;
            for ver in curr_year_data.versions.values_mut() {
                ver.0[day_of_year] = 0; // clear that day only
            }

            for dl in dls {
                let ver = dl.version.map(|v| v.num.clone().into_boxed_str());
                let ver_year = curr_year_data.versions.entry(ver).or_insert_with(|| DailyDownloads([0; 366]));
                ver_year.0[day_of_year] += dl.downloads as u32;
            }
        }
        if modified {
            self.yearly.set_crate_year(crates_io_name, curr_year, &curr_year_data)?;
        }
        Ok(())
    }

    pub fn index_crate_highest_version(&self, v: &RichCrateVersion) -> CResult<()> {
        if stopped() {Err(KitchenSinkErr::Stopped)?;}

        // direct deps are used as extra keywords for similarity matching,
        // but we're taking only niche deps to group similar niche crates together
        let raw_deps_stats = self.index.deps_stats().ok_or(KitchenSinkErr::DepsStatsNotAvailable)?;
        let mut weighed_deps = Vec::<(&str, f32)>::new();
        let all_deps = v.direct_dependencies()?;
        let all_deps = [(all_deps.0, 1.0), (all_deps.2, 0.33)];
        // runtime and (lesser) build-time deps
        for (deps, overall_weight) in all_deps.iter() {
            for dep in deps {
                if let Some(rev) = raw_deps_stats.counts.get(dep.package.as_str()) {
                    let right_popularity = rev.direct > 1 && rev.direct < 150 && rev.runtime.def < 500 && rev.runtime.opt < 800;
                    if Self::dep_interesting_for_index(dep.package.as_str()).unwrap_or(right_popularity) {
                        let weight = overall_weight / (1 + rev.direct) as f32;
                        weighed_deps.push((dep.package.as_str(), weight));
                    }
                }
            }
        }
        let (is_build, is_dev) = self.is_build_or_dev(v);
        self.crate_db.index_latest(CrateVersionData {
            name: v.short_name(),
            keywords: v.keywords(Include::AuthoritativeOnly).map(|k| k.trim().to_lowercase()).collect(),
            description: v.description(),
            alternative_description: v.alternative_description(),
            readme_text: v.readme().map(|r| render_readme::Renderer::new(None).visible_text(&r.markup)),
            category_slugs: v.category_slugs(Include::AuthoritativeOnly).collect(),
            authors: v.authors(),
            origin: v.origin(),
            repository: v.repository(),
            deps_stats: &weighed_deps,
            features: v.features(),
            is_sys: v.is_sys(),
            has_bin: v.has_bin(),
            is_yanked: v.is_yanked(),
            has_cargo_bin: v.has_cargo_bin(),
            is_proc_macro: v.is_proc_macro(),
            is_build, is_dev,
            links: v.links(),
        })?;
        Ok(())
    }

    // deps that are closely related to crates in some category
    fn dep_interesting_for_index(name: &str) -> Option<bool> {
        match name {
            "futures" | "tokio" | "actix-web" | "rocket_codegen" | "iron" | "rusoto_core" | "rocket" | "router" |
            "quoted_printable" | "mime" | "rustls" | "websocket" |
            "piston2d-graphics" | "amethyst_core" | "amethyst" | "specs" | "piston" | "allegro" | "minifb" |
            "rgb" | "imgref" |
            "core-foundation" |
            "proc-macro2" | "cargo" | "cargo_metadata" | "git2" | "dbus" |
            "hound" | "lopdf" |
            "nom" | "lalrpop" | "combine" |
            "clap" | "structopt" |
            "syntect" | "stdweb" | "parity-wasm" => Some(true),
            /////////
            "threadpool" | "rayon" | "md5" | "arrayref" | "memmmap" | "xml" | "crossbeam" | "pyo3" |
            "rustc_version" | "crossbeam-channel" | "cmake" | "errno" | "zip" | "enum_primitive" | "pretty_env_logger" |
            "skeptic" | "crc" | "hmac" | "sha1" | "serde_macros" | "serde_codegen" | "derive_builder" |
            "derive_more" | "ron" | "fxhash" | "simple-logger" | "chan" | "stderrlog" => Some(false),
            _ => None,
        }
    }

    pub fn index_repo(&self, repo: &Repo, as_of_version: &str) -> CResult<()> {
        if stopped() {Err(KitchenSinkErr::Stopped)?;}
        let url = repo.canonical_git_url();
        let checkout = crate_git_checkout::checkout(repo, &self.git_checkout_path)?;

        let (manif, warnings) = crate_git_checkout::find_manifests(&checkout)
            .with_context(|_| format!("find manifests in {}", url))?;
        for warn in warnings {
            eprintln!("warning: {}", warn.0);
        }
        let manif = manif.into_iter().filter_map(|(subpath, manifest)| {
            manifest.package.map(|p| (subpath, p.name))
        });
        self.crate_db.index_repo_crates(repo, manif).context("index rev repo")?;

        if let Repo { host: RepoHost::GitHub(ref repo), .. } = repo {
            if let Some(commits) = self.repo_commits(repo, as_of_version)? {
                for c in commits {
                    if let Some(a) = c.author {
                        self.index_user(&a, &c.commit.author)?;
                    }
                    if let Some(a) = c.committer {
                        self.index_user(&a, &c.commit.committer)?;
                    }
                }
            }
        }

        if stopped() {Err(KitchenSinkErr::Stopped)?;}

        let mut changes = Vec::new();
        crate_git_checkout::find_dependency_changes(&checkout, |added, removed, age| {
            if removed.is_empty() {
                if added.len() > 1 {
                    // Divide evenly between all recommendations
                    // and decay with age, assuming older changes are less relevant now.
                    let weight = (1.0 / added.len().pow(2) as f64) * 30.0 / (30.0 + age as f64);
                    if weight > 0.002 {
                        for dep1 in added.iter().take(8) {
                            for dep2 in added.iter().take(5) {
                                if dep1 == dep2 {
                                    continue;
                                }
                                // Not really a replacement, but a recommendation if A then B
                                changes.push(RepoChange::Replaced { crate_name: dep1.to_string(), replacement: dep2.to_string(), weight })
                            }
                        }
                    }
                }
            } else {
                for crate_name in removed {
                    if !added.is_empty() {
                        // Assuming relevance falls a bit when big changes are made.
                        // Removals don't decay with age (if we see great comebacks, maybe it should be split by semver-major).
                        let weight = 8.0 / (7.0 + added.len() as f64);
                        for replacement in &added {
                            changes.push(RepoChange::Replaced { crate_name: crate_name.clone(), replacement: replacement.to_string(), weight })
                        }
                        changes.push(RepoChange::Removed { crate_name, weight });
                    } else {
                        // ??? maybe use sliiight recommendation score based on existing (i.e. newer) state of deps in the repo?
                        changes.push(RepoChange::Removed { crate_name, weight: 0.95 });
                    }
                }
            }
        })?;
        self.crate_db.index_repo_changes(repo, &changes)?;

        Ok(())
    }

    pub fn user_by_email(&self, email: &str) -> CResult<Option<User>> {
        Ok(self.user_db.user_by_email(email).context("user_by_email")?)
    }

    pub fn user_by_github_login(&self, github_login: &str) -> CResult<Option<User>> {
        if let Some(gh) = self.user_db.user_by_github_login(github_login)? {
            if gh.name.is_some() {
                return Ok(Some(gh));
            }
        }
        Ok(self.gh.user_by_login(github_login)?) // errs on 404
    }

    /// List of all notable crates
    /// Returns origin, rank, last updated unix timestamp
    pub fn sitemap_crates(&self) -> CResult<Vec<(Origin, f64, i64)>> {
        Ok(self.crate_db.sitemap_crates()?)
    }

    /// If given crate is a sub-crate, return crate that owns it.
    /// The relationship is based on directory layout of monorepos.
    pub fn parent_crate(&self, child: &RichCrateVersion) -> Option<RichCrateVersion> {
        let repo = child.repository()?;
        let origin = self.crate_db.parent_crate(repo, child.short_name()).ok().and_then(|v| v)?;
        self.rich_crate_version(&origin, CrateData::Minimal)
            .map_err(|e| eprintln!("parent crate: {} {:?}", e, origin)).ok()
    }

    pub fn cachebust_string_for_repo(&self, crate_repo: &Repo) -> CResult<String> {
        Ok(self.crate_db.crates_in_repo(crate_repo)
            .context("db crates_in_repo")?
            .into_iter()
            .filter_map(|origin| self.index.crate_version_latest_unstable(&origin).ok())
            .map(|c| c.version().to_string())
            .next()
            .unwrap_or_else(|| "*".to_string()))
    }

    pub fn user_github_orgs(&self, github_login: &str) -> CResult<Option<Vec<UserOrg>>> {
        Ok(self.gh.user_orgs(github_login)?)
    }

    /// Merge authors, owners, contributors
    pub fn all_contributors<'a>(&self, krate: &'a RichCrateVersion) -> CResult<(Vec<CrateAuthor<'a>>, Vec<CrateAuthor<'a>>, bool, usize)> {
        let mut hit_max_contributor_count = false;
        let mut contributors_by_login = match krate.repository().as_ref() {
            Some(crate_repo) => match crate_repo.host() {
                // TODO: warn on errors?
                RepoHost::GitHub(ref repo) => {
                    // multiple crates share a repo, which causes cache churn when version "changes"
                    // so pick one of them and track just that one version
                    let cachebust = self.cachebust_string_for_repo(crate_repo).context("contrib")?;
                    let contributors = self.gh.contributors(repo, &cachebust).context("contributors")?.unwrap_or_default();
                    if contributors.len() >= 100 {
                        hit_max_contributor_count = true;
                    }
                    let mut by_login = HashMap::new();
                    for contr in contributors {
                        if let Some(author) = contr.author {
                            if author.user_type == UserType::Bot {
                                continue;
                            }
                            let count = contr.weeks.iter()
                                .map(|w| {
                                    w.commits as f64 +
                                    ((w.added + w.deleted*2) as f64).sqrt()
                                }).sum::<f64>();
                            by_login.entry(author.login.to_lowercase())
                                .or_insert((0., author)).0 += count;
                        }
                    }
                    by_login
                },
                RepoHost::BitBucket(..) |
                RepoHost::GitLab(..) |
                RepoHost::Other => HashMap::new(), // TODO: could use git checkout...
            },
            None => HashMap::new(),
        };


        let mut authors: HashMap<AuthorId, CrateAuthor<'_>> = krate.authors()
            .iter().enumerate().map(|(i,author)| {
                let mut ca = CrateAuthor {
                    nth_author: Some(i),
                    contribution: 0.,
                    info: Some(Cow::Borrowed(author)),
                    github: None,
                    owner: false,
                };
                if let Some(ref email) = author.email {
                    if let Ok(Some(github)) = self.user_db.user_by_email(email) {
                        let id = github.id;
                        ca.github = Some(github);
                        return (AuthorId::GitHub(id), ca);
                    }
                }
                if let Some(ref url) = author.url {
                    let gh_url = "https://github.com/";
                    if url.to_ascii_lowercase().starts_with(gh_url) {
                        let login = url[gh_url.len()..].splitn(1, '/').next().expect("can't happen");
                        if let Ok(Some(gh)) = self.gh.user_by_login(login) {
                            let id = gh.id;
                            ca.github = Some(gh);
                            return (AuthorId::GitHub(id), ca);
                        }
                    }
                }
                // name only, no email
                if ca.github.is_none() && author.email.is_none() {
                    if let Some(ref name) = author.name {
                        if let Some((contribution, github)) = contributors_by_login.remove(&name.to_lowercase()) {
                            let id = github.id;
                            ca.github = Some(github);
                            ca.info = None; // was useless; just a login; TODO: only clear name once it's Option
                            ca.contribution = contribution;
                            return (AuthorId::GitHub(id), ca);
                        }
                    }
                }
                let key = author.email.as_ref().map(|e| AuthorId::Email(e.to_ascii_lowercase()))
                    .or_else(|| author.name.as_ref().map(|n| AuthorId::Name(n.to_lowercase())))
                    .unwrap_or(AuthorId::Meh(i));
                (key, ca)
            }).collect();

        if let Ok(owners) = self.crate_owners(krate) {
            for owner in owners {
                if let Ok(id) = self.owners_github_id(&owner) {
                    match authors.entry(AuthorId::GitHub(id)) {
                        Occupied(mut e) => {
                            let e = e.get_mut();
                            e.owner = true;
                            if e.info.is_none() {
                                e.info = Some(Cow::Owned(Author{
                                    name: Some(owner.name().to_owned()),
                                    email: None,
                                    url: Some(owner.url.clone()),
                                }));
                            } else if let Some(ref mut gh) = e.github {
                                if gh.name.is_none() {
                                    gh.name = Some(owner.name().to_owned());
                                }
                            }
                        },
                        Vacant(e) => {
                            e.insert(CrateAuthor {
                                contribution: 0.,
                                github: self.gh.user_by_id(id).ok().and_then(|a|a),
                                info: Some(Cow::Owned(Author{
                                    name: Some(owner.name().to_owned()),
                                    email: None,
                                    url: Some(owner.url.clone()),
                                })),
                                nth_author: None,
                                owner: true,
                            });
                        },
                    }
                }
            }
        }

        for (_, (contribution, github)) in contributors_by_login {
            authors.entry(AuthorId::GitHub(github.id))
            .or_insert(CrateAuthor {
                nth_author: None,
                contribution: 0.,
                info: None,
                github: Some(github),
                owner: false,
            }).contribution += contribution;
        }

        let mut authors_by_name = HashMap::<String, CrateAuthor<'_>>::new();
        for (_, a) in authors {
            match authors_by_name.entry(a.name().to_lowercase()) {
                Occupied(mut e) => {
                    let e = e.get_mut();
                    // TODO: should keep both otherwise
                    // if both have gh, they're different users
                    if e.github.is_some() != a.github.is_some() {
                        // merge
                        if a.owner {
                            e.owner = true;
                        }
                        if e.info.is_none() {
                            e.info = a.info;
                        }
                        if e.nth_author.is_none() {
                            e.nth_author = a.nth_author;
                        }
                        e.contribution = e.contribution.max(a.contribution);
                        if e.github.is_none() {
                            e.github = a.github;
                        }
                    }
                },
                Vacant(e) => {
                    e.insert(a);
                },
            }
        }

        let max_author_contribution = authors_by_name
            .values()
            .map(|a| if a.owner || a.nth_author.is_some() { a.contribution } else { 0. })
            .max_by(|a, b| a.partial_cmp(&b).unwrap_or(Ordering::Equal))
            .unwrap_or(0.);
        let big_contribution = if max_author_contribution < 50. { 200. } else { max_author_contribution / 2. };

        let mut contributors = 0;
        let (mut authors, mut owners): (Vec<_>, Vec<_>) = authors_by_name.into_iter().map(|(_,v)|v)
            .filter(|a| if a.owner || a.nth_author.is_some() || a.contribution >= big_contribution {
                true
            } else {
                contributors += 1;
                false
            })
            .partition(|a| {
                a.nth_author.is_some() || a.contribution > 0.
            });


        for author in &mut authors {
            if let Some(ref mut gh) = author.github {
                if gh.name.is_none() {
                    let res = self.user_by_github_login(&gh.login);
                    if let Ok(Some(new_gh)) = res {
                        *gh = new_gh
                    }
                }
            }
        }

        authors.sort_by(|a, b| {
            fn score(a: &CrateAuthor<'_>) -> f64 {
                let o = if a.owner { 200. } else { 1. };
                o * (a.contribution + 10.) / (1 + a.nth_author.unwrap_or(99)) as f64
            }
            score(b).partial_cmp(&score(a)).unwrap_or(Ordering::Equal)
        });

        // That's a guess
        if authors.len() == 1 && owners.len() == 1 && authors[0].github.is_none() {
            let author_is_team = authors[0].likely_a_team();
            let gh_is_team = owners[0].github.as_ref().map_or(false, |g| g.user_type == UserType::Org);
            if author_is_team == gh_is_team {
                let co = owners.remove(0);
                authors[0].github = co.github;
                authors[0].owner = co.owner;
            }
        }

        let owners_partial = authors.iter().any(|a| a.owner);
        Ok((authors, owners, owners_partial, if hit_max_contributor_count { 100 } else { contributors }))
    }

    fn owners_github_id(&self, owner: &CrateOwner) -> CResult<u32> {
        // this is silly, but crates.io doesn't keep the github ID explicitly
        // (the id field is crates-io's field), but it does keep the avatar URL
        // which contains github's ID
        if let Some(ref avatar) = owner.avatar {
            let r = regex::Regex::new("https://avatars[0-9]+.githubusercontent.com/u/([0-9]+)").expect("regex");
            if let Some(c) = r.captures(avatar) {
                let id = c.get(1).expect("regex").as_str();
                return Ok(id.parse().expect("regex"))
            }
        }
        // This is a bit weak, since logins are not permanent
        if let Some(user) = self.gh.user_by_login(owner.github_login().ok_or(KitchenSinkErr::OwnerWithoutLogin)?)? {
            return Ok(user.id);
        }
        Err(KitchenSinkErr::OwnerWithoutLogin)?
    }

    fn crate_owners(&self, krate: &RichCrateVersion) -> CResult<Vec<CrateOwner>> {
        self.crates_io_crate_owners(krate.short_name(), krate.version())
    }

    pub fn crates_io_crate_owners(&self, crate_name: &str, version: &str) -> CResult<Vec<CrateOwner>> {
        Ok(self.crates_io.crate_owners(crate_name, version).context("crate_owners")?.unwrap_or_default())
    }

    // Sorted from the top, returns origins
    pub fn top_crates_in_category(&self, slug: &str) -> CResult<Arc<Vec<Origin>>> {
        {
            let cache = self.top_crates_cached.read().expect("poison");
            if let Some(category) = cache.get(slug) {
                return Ok(category.clone());
            }
        }
        let total_count = self.category_crate_count(slug)?;
        let wanted_num = ((total_count/3+25)/50 * 50).max(100);
        let mut cache = self.top_crates_cached.write().expect("poison");
        use std::collections::hash_map::Entry::*;
        Ok(match cache.entry(slug.to_owned()) {
            Occupied(e) => Arc::clone(e.get()),
            Vacant(e) => {
                let crates = if slug == "uncategorized" {
                    self.crate_db.top_crates_uncategorized(wanted_num)?
                } else {
                    self.crate_db.top_crates_in_category_partially_ranked(slug, wanted_num)?
                };
                let crates: Vec<_> = crates.into_iter().map(|(o, _)| o).take(wanted_num as usize).collect();
                let res = Arc::new(crates);
                e.insert(Arc::clone(&res));
                res
            },
        })
    }

    pub fn top_keywords_in_category(&self, cat: &Category) -> CResult<Vec<String>> {
        let mut keywords = self.crate_db.top_keywords_in_category(&cat.slug)?;
        keywords.retain(|k| !cat.obvious_keywords.contains(k));
        keywords.truncate(10);
        Ok(keywords)
    }

    /// true if it's useful as a keyword page
    pub fn is_it_a_keyword(&self, k: &str) -> bool {
        self.crate_db.crates_with_keyword(k).map(|n| n >= 5).unwrap_or(false)
    }

    /// True if there are multiple crates with that keyword. Populated first.
    pub fn keywords_populated(&self, krate: &RichCrateVersion) -> Vec<(String, bool)> {
        let mut keywords: Vec<_> = krate.keywords(Include::Cleaned)
        .map(|k| {
            let populated = self.crate_db.crates_with_keyword(&k.to_lowercase()).unwrap() >= 3;
            (k.to_owned(), populated)
        })
        .collect();
        keywords.sort_by_key(|&(_, v)| !v); // populated first; relies on stable sort
        keywords
    }

    pub fn recently_updated_crates_in_category(&self, slug: &str) -> CResult<Vec<Origin>> {
        Ok(self.crate_db.recently_updated_crates_in_category(slug)?)
    }

    pub fn recently_updated_crates(&self) -> CResult<Vec<Origin>> {
        Ok(self.crate_db.recently_updated_crates()?)
    }

    pub fn category_crate_count(&self, slug: &str) -> Result<u32, KitchenSinkErr> {
        if slug == "uncategorized" {
            return Ok(100);
        }
        self.category_crate_counts
            .get(|| match self.crate_db.category_crate_counts() {
                Ok(res) => Some(res),
                Err(err) => {
                    eprintln!("error: can't get category counts: {}", err);
                    None
                },
            })
            .as_ref()
            .ok_or(KitchenSinkErr::CategoryQueryFailed)
            .and_then(|h| {
                h.get(slug).map(|&c| c).ok_or_else(|| {
                    KitchenSinkErr::CategoryNotFound(slug.to_string())
                })
            })
    }

    fn repo_commits(&self, repo: &SimpleRepo, as_of_version: &str) -> CResult<Option<Vec<github_info::CommitMeta>>> {
        Ok(self.gh.commits(repo, as_of_version)?)
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum CrateData {
    Minimal,
    Full,
    FullNoDerived,
}

/// This is used to uniquely identify authors based on as little information as is available
#[derive(Debug, Hash, Eq, PartialEq)]
enum AuthorId {
    GitHub(u32),
    Name(String),
    Email(String),
    Meh(usize),
}

#[derive(Debug, Clone)]
pub struct CrateAuthor<'a> {
    /// Is identified as owner of the crate by crates.io API?
    pub owner: bool,
    /// Order in the Cargo.toml file
    pub nth_author: Option<usize>,
    /// Arbitrary value derived from number of commits. The more the better.
    pub contribution: f64,
    /// From Cargo.toml
    pub info: Option<Cow<'a, Author>>,
    /// From GitHub API and/or crates.io API
    pub github: Option<User>,
}

impl<'a> CrateAuthor<'a> {
    pub fn likely_a_team(&self) -> bool {
        let unattributed_name = self.github.is_none() && self.info.as_ref().map_or(true, |a| a.email.is_none() && a.url.is_none());
        unattributed_name || self.name().ends_with(" Developers") || self.name().ends_with(" contributors")
    }

    pub fn name(&self) -> &str {
        if let Some(ref info) = self.info {
            if let Some(ref name) = &info.name {
                return name;
            }
        }
        if let Some(ref gh) = self.github {
            if let Some(ref name) = gh.name {
                name
            } else {
                &gh.login
            }
        } else {
            if let Some(ref info) = self.info {
                if let Some(ref email) = &info.email {
                    return email.split('@').next().unwrap();
                }
            }
            "?anon?"
        }
    }
}

#[test]
fn crates() {
    KitchenSink::new_default().expect("Test if configured");
}
