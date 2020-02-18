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
mod tarball;
mod ctrlcbreak;
pub use crate::ctrlcbreak::*;

pub use crate_db::builddb::CompatibilityInfo;
pub use crate_db::builddb::Compat;
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
pub use rich_crate::MaintenanceStatus;
pub use rich_crate::Markup;
pub use rich_crate::Origin;
pub use rich_crate::RichCrate;
pub use rich_crate::RichCrateVersion;
pub use rich_crate::RichDep;
pub use rich_crate::{Cfg, Target};
pub use semver::Version as SemVer;
use rich_crate::ManifestExt;

use cargo_toml::Manifest;
use cargo_toml::Package;
use categories::Category;
use chrono::DateTime;
use chrono::prelude::*;
use crate::tarball::CrateFile;
use crate_db::{CrateDb, CrateVersionData, RepoChange, builddb::BuildDb};
use crates_io_client::CrateOwner;
use failure::ResultExt;
use github_info::GitCommitAuthor;
use github_info::GitHubRepo;
use itertools::Itertools;
use lazyonce::LazyOnce;
use parking_lot::RwLock;
use rayon::prelude::*;
use repo_url::Repo;
use repo_url::RepoHost;
use repo_url::SimpleRepo;
use rich_crate::Author;
use rich_crate::CrateVersion;
use rich_crate::CrateVersionSourceData;
use rich_crate::Readme;
use semver::VersionReq;
use simple_cache::TempCache;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::hash_map::Entry::*;
use std::collections::HashMap;
use std::collections::HashSet;
use std::convert::TryInto;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

type FxHashMap<K, V> = std::collections::HashMap<K, V, ahash::RandomState>;

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
    #[fail(display = "`readme` property points to a file that hasn't been published")]
    NoReadmePackaged,
    #[fail(display = "Can't find README in repository: {}", _0)]
    NoReadmeInRepo(String),
    #[fail(display = "Could not clone repository: {}", _0)]
    ErrorCloning(String),
    #[fail(display = "{} URL is a broken link: {}", _0, _1)]
    BrokenLink(String, String),
    #[fail(display = "Error parsing manifest: {}", _0)]
    ManifestParseError(String),
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
    #[fail(display = "crate {} not found in repo {}", _0, _1)]
    CrateNotFoundInRepo(String, String),
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
    #[fail(display = "Git crate '{:?}' can't be indexed, because it's not on the list", _0)]
    GitCrateNotAllowed(Origin),
}

#[derive(Debug, Clone)]
pub struct DownloadWeek {
    pub date: Date<Utc>,
    pub total: usize,
    pub downloads: HashMap<Option<usize>, usize>,
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
    loaded_rich_crate_version_cache: RwLock<FxHashMap<Origin, RichCrateVersion>>,
    category_crate_counts: LazyOnce<Option<HashMap<String, u32>>>,
    removals: LazyOnce<HashMap<Origin, f64>>,
    top_crates_cached: RwLock<FxHashMap<String, Arc<Vec<Origin>>>>,
    git_checkout_path: PathBuf,
    main_cache_dir: PathBuf,
    yearly: AllDownloads,
    category_overrides: HashMap<String, Vec<Cow<'static, str>>>,
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
        Self::new(&data_path, &github_token)
    }

    pub fn new(data_path: &Path, github_token: &str) -> CResult<Self> {
        let main_cache_dir = data_path.to_owned();

        let ((crates_io, gh), index) = rayon::join(|| rayon::join(
                || crates_io_client::CratesIoClient::new(data_path),
                || github_info::GitHub::new(&data_path.join("github.db"), github_token)),
            || Index::new(data_path));
        Ok(Self {
            crates_io: crates_io.context("cratesio")?,
            index: index.context("index")?,
            url_check_cache: TempCache::new(&data_path.join("url_check.db")).context("urlcheck")?,
            docs_rs: docs_rs_client::DocsRsClient::new(data_path.join("docsrs.db")).context("docs")?,
            crate_db: CrateDb::new(Self::assert_exists(data_path.join("crate_data.db"))?).context("db")?,
            user_db: user_db::UserDb::new(Self::assert_exists(data_path.join("users.db"))?).context("udb")?,
            gh: gh.context("gh")?,
            loaded_rich_crate_version_cache: RwLock::new(FxHashMap::default()),
            git_checkout_path: data_path.join("git"),
            category_crate_counts: LazyOnce::new(),
            removals: LazyOnce::new(),
            top_crates_cached: RwLock::new(FxHashMap::default()),
            yearly: AllDownloads::new(&main_cache_dir),
            main_cache_dir,
            category_overrides: Self::load_category_overrides(&data_path.join("category_overrides.txt"))?,
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

    fn load_category_overrides(path: &Path) -> CResult<HashMap<String, Vec<Cow<'static, str>>>> {
        let p = std::fs::read_to_string(path)?;
        let mut out = HashMap::new();
        for line in p.lines() {
            let mut parts = line.splitn(2, ':');
            let crate_name = parts.next().unwrap().trim();
            if crate_name.is_empty() {
                continue;
            }
            let categories: Vec<_> = parts.next().unwrap().split(',')
                .map(|s| s.trim().to_string().into()).collect();
            if categories.is_empty() {
                continue;
            }
            out.insert(crate_name.to_owned(), categories);
        }
        Ok(out)
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
    pub fn all_crates(&self) -> impl Iterator<Item=Origin> + '_ {
        self.index.all_crates()
    }

    /// Iterator over all crates available in the index
    ///
    /// It returns only identifiers,
    /// so `rich_crate`/`rich_crate_version` is needed to do more.
    pub fn all_crates_io_crates(&self) -> &FxHashMap<Box<str>, CratesIndexCrate> {
        self.index.crates_io_crates()
    }

    /// Gets cratesio download data, but not from the API, but from our local copy
    pub fn weekly_downloads(&self, k: &RichCrate, num_weeks: u16) -> CResult<Vec<DownloadWeek>> {
        let mut res = Vec::with_capacity(num_weeks.into());
        let mut now = Utc::today();

        let mut curr_year = now.year();
        let mut curr_year_data = self.yearly.get_crate_year(k.name(), curr_year as _)?.unwrap_or_default();

        let day_of_year = now.ordinal0();
        let missing_data_days = curr_year_data.0[0..day_of_year as usize].iter().cloned().rev().take_while(|&s| s == 0).count();

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
                if curr_year_data.0[day_of_year] > 0 {
                    any_set = true;
                }
                total += curr_year_data.0[day_of_year] as usize;
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

    pub fn all_new_crates(&self) -> CResult<Vec<RichCrate>> {
        let min_timestamp = self.crate_db.latest_crate_update_timestamp()?.unwrap_or(0);
        let all = self.index.crates_io_crates(); // too slow to scan all GH crates
        Ok(all.into_par_iter()
        .filter_map(move |(name, _)| {
            self.rich_crate(&Origin::from_crates_io_name(&*name)).map_err(|e| eprintln!("{}: {}", name, e)).ok()
        })
        .filter(move |k| {
            let latest = k.versions().iter().map(|v| v.created_at.as_str()).max().unwrap_or("");
            if let Ok(timestamp) = DateTime::parse_from_rfc3339(latest) {
                timestamp.timestamp() >= min_timestamp as i64
            } else {
                eprintln!("Can't parse {} of {}", latest, k.name());
                true
            }
        }).collect())
    }

    pub fn crate_exists(&self, origin: &Origin) -> bool {
        self.index.crate_exists(origin)
    }

    /// Wrapper object for metadata common for all versions of a crate
    pub fn rich_crate(&self, origin: &Origin) -> CResult<RichCrate> {
        if stopped() {Err(KitchenSinkErr::Stopped)?;}
        match origin {
            Origin::CratesIo(name) => {
                let meta = self.crates_io_meta(name)?;
                let versions = meta.meta.versions().map(|c| CrateVersion {
                    num: c.num,
                    updated_at: c.updated_at,
                    created_at: c.created_at,
                    yanked: c.yanked,
                }).collect();
                Ok(RichCrate::new(origin.clone(), meta.owners, meta.meta.krate.name, versions))
            },
            Origin::GitHub {repo, package} => {
                let host = RepoHost::GitHub(repo.clone()).try_into().map_err(|_| KitchenSinkErr::CrateNotFound(origin.clone())).context("ghrepo host bad")?;
                let cachebust = self.cachebust_string_for_repo(&host).context("ghrepo")?;
                let gh = self.gh.repo(repo, &cachebust)?
                    .ok_or_else(|| KitchenSinkErr::CrateNotFound(origin.clone()))
                    .context(format!("ghrepo {:?} not found", repo))?;
                let versions = self.get_repo_versions(origin, &host, &cachebust)?;
                Ok(RichCrate::new(origin.clone(), gh.owner.into_iter().map(|o| {
                    CrateOwner {
                        id: 0,
                        avatar: o.avatar_url,
                        url: o.html_url,
                        login: o.login,
                        kind: OwnerKind::User, // FIXME: crates-io uses teams, and we'd need to find the right team? is "owners" a guaranteed thing?
                        name: o.name,
                    }
                }).collect(),
                format!("github/{}/{}", repo.owner, package),
                versions))
            },
            Origin::GitLab {repo, package} => {
                let host = RepoHost::GitLab(repo.clone()).try_into().map_err(|_| KitchenSinkErr::CrateNotFound(origin.clone())).context("ghrepo host bad")?;
                let cachebust = self.cachebust_string_for_repo(&host).context("ghrepo")?;
                let versions = self.get_repo_versions(origin, &host, &cachebust)?;
                Ok(RichCrate::new(origin.clone(), vec![], format!("gitlab/{}/{}", repo.owner, package), versions))
            }
        }
    }

    fn get_repo_versions(&self, origin: &Origin, repo: &Repo, cachebust: &str) -> CResult<Vec<CrateVersion>> {
        let package = match origin {
            Origin::GitLab {package, ..} => package,
            Origin::GitHub {repo, package} => {
                let releases = self.gh.releases(repo, cachebust)?.ok_or_else(|| KitchenSinkErr::CrateNotFound(origin.clone())).context("releases not found")?;
                let versions: Vec<_> = releases.into_iter().filter_map(|r| {
                    let date = r.published_at.or(r.created_at)?;
                    let num_full = r.tag_name?;
                    let num = num_full.trim_start_matches(|c:char| !c.is_numeric());
                    // verify that it semver-parses
                    let _ = SemVer::parse(num).map_err(|e| eprintln!("{:?}: ignoring {}, {}", origin, num_full, e)).ok()?;
                    Some(CrateVersion {
                        num: num.to_string(),
                        yanked: r.draft.unwrap_or(false),
                        updated_at: date.clone(),
                        created_at: date,
                    })
                }).collect();
                if !versions.is_empty() {
                    return Ok(versions);
                }
                package
            },
            _ => unreachable!(),
        };

        let versions: Vec<_> = self.crate_db.crate_versions(origin)?.into_iter().map(|(num, timestamp)| {
            let date = Utc.timestamp(timestamp as _, 0).to_rfc3339();
            CrateVersion {
                num,
                yanked: false,
                updated_at: date.clone(),
                created_at: date,
            }
        }).collect();
        if !versions.is_empty() {
            return Ok(versions);
        }
        eprintln!("Need to scan repo {:?}", repo);
        let checkout = crate_git_checkout::checkout(repo, &self.git_checkout_path)?;
        let mut pkg_ver = crate_git_checkout::find_versions(&checkout)?;
        if let Some(v) = pkg_ver.remove(&**package) {
            let versions: Vec<_> = v.into_iter().map(|(num, timestamp)| {
                let date = Utc.timestamp(timestamp, 0).to_rfc3339();
                CrateVersion {
                    num,
                    yanked: false,
                    updated_at: date.clone(),
                    created_at: date,
                }
            }).collect();
            if !versions.is_empty() {
                return Ok(versions);
            }
        }
        Err(KitchenSinkErr::CrateNotFound(origin.clone())).context("missing releases, even tags")?
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

        // arbitrary multiplier. TODO: it's not fair for apps vs libraries
        Ok(self.github_stargazers_and_watchers(origin)?.map(|(stars, watch)| stars.saturating_sub(1) as usize * 50 + watch.saturating_sub(1) as usize * 150))
    }

    /// Only for GitHub origins, not for crates-io crates
    pub fn github_stargazers_and_watchers(&self, origin: &Origin) -> CResult<Option<(u32, u32)>> {
        if let Origin::GitHub {repo, ..} = origin {
            let repo = RepoHost::GitHub(repo.clone()).try_into().expect("repohost");
            if let Some(gh) = self.github_repo(&repo)? {
                return Ok(Some((gh.stargazers_count, gh.subscribers_count)));
            }
        }
        Ok(None)
    }

    fn downloads_recent(&self, origin: &Origin) -> CResult<Option<usize>> {
        Ok(match origin {
            Origin::CratesIo(name) => {
                let meta = self.crates_io_meta(name)?;
                meta.meta.krate.recent_downloads
            },
            _ => None,
        })
    }

    fn crates_io_meta(&self, name: &str) -> CResult<CratesIoCrate> {
        let krate = self.index.crates_io_crate_by_lowercase_name(name).context("rich_crate")?;
        let latest_in_index = krate.latest_version().version(); // most recently published version
        let meta = self.crates_io.krate(name, latest_in_index)
            .with_context(|_| format!("crates.io meta for {} {}", name, latest_in_index))?;
        let mut meta = meta.ok_or_else(|| KitchenSinkErr::CrateNotFound(Origin::from_crates_io_name(name)))?;
        if !meta.meta.versions.iter().any(|v| v.num == latest_in_index) {
            eprintln!("Crate data missing latest version {}@{}", name, latest_in_index);
            meta = self.crates_io.krate(name, &format!("{}-try-again", latest_in_index))?
                .ok_or_else(|| KitchenSinkErr::CrateNotFound(Origin::from_crates_io_name(name)))?;
            if !meta.meta.versions.iter().any(|v| v.num == latest_in_index) {
                eprintln!("Error: crate data is borked {}@{}. Has only: {:?}", name, latest_in_index, meta.meta.versions.iter().map(|v| &v.num).collect::<Vec<_>>());
            }
        }
        Ok(meta)
    }

    /// Wrapper for the latest version of a given crate.
    ///
    /// This function is quite slow, as it reads everything about the crate.
    ///
    /// There's no support for getting anything else than the latest version.
    pub fn rich_crate_version(&self, origin: &Origin) -> CResult<RichCrateVersion> {
        if stopped() {Err(KitchenSinkErr::Stopped)?;}

        if let Some(krate) = self.loaded_rich_crate_version_cache.read().get(origin) {
            return Ok(krate.clone());
        }

        let (manifest, derived) = match self.crate_db.rich_crate_version_data(origin) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("getting {:?}: {}", origin, e);
                self.index_crate_highest_version(origin)?;
                match self.crate_db.rich_crate_version_data(origin) {
                    Ok(v) => v,
                    Err(e) => Err(e)?,
                }
            },
        };

        let krate = RichCrateVersion::new(origin.clone(), manifest, derived);
        self.loaded_rich_crate_version_cache.write().insert(origin.clone(), krate.clone());
        Ok(krate)
    }

    pub fn changelog_url(&self, k: &RichCrateVersion) -> Option<String> {
        let repo = k.repository()?;
        if let RepoHost::GitHub(ref gh) = repo.host() {
            let releases = self.gh.releases(gh, &self.cachebust_string_for_repo(repo).ok()?).ok()??;
            if releases.iter().any(|rel| rel.body.as_ref().map_or(false, |b| b.len() > 15)) {
                return Some(format!("https://github.com/{}/{}/releases", gh.owner, gh.repo));
            }
        }
        None
    }

    fn rich_crate_version_from_repo(&self, origin: &Origin) -> CResult<(CrateVersionSourceData, Manifest, Warnings)> {
        let (repo, package) = match origin {
            Origin::GitHub {repo, package} => {
                (RepoHost::GitHub(repo.clone()).try_into().expect("repohost"), &**package)
            },
            Origin::GitLab {repo, package} => {
                (RepoHost::GitLab(repo.clone()).try_into().expect("repohost"), &**package)
            },
            _ => unreachable!()
        };

        let checkout = crate_git_checkout::checkout(&repo, &self.git_checkout_path)?;
        let (path_in_repo, tree_id, manifest) = crate_git_checkout::path_in_repo(&checkout, package)?
            .ok_or_else(|| {
                let (has, err) = crate_git_checkout::find_manifests(&checkout).unwrap_or_default();
                for e in err {
                    eprintln!("parse err: {}", e.0);
                }
                for h in has {
                    eprintln!("has: {} -> {}", h.0, h.2.package.as_ref().map(|p| p.name.as_str()).unwrap_or("?"));
                }
                KitchenSinkErr::CrateNotFoundInRepo(package.to_string(), repo.canonical_git_url().into_owned())
            })?;

        let mut warnings = HashSet::new();

        let mut meta = tarball::read_repo(&checkout, tree_id)?;
        debug_assert_eq!(meta.manifest.package, manifest.package);
        let package = meta.manifest.package.as_mut().ok_or_else(|| KitchenSinkErr::NotAPackage(origin.clone()))?;

        // Allowing any other URL would allow spoofing
        package.repository = Some(repo.canonical_git_url().into_owned());

        let has_readme = meta.readme.is_some();
        if !has_readme {
            let maybe_repo = package.repository.as_ref().and_then(|r| Repo::new(r).ok());
            warnings.insert(Warning::NoReadmeProperty);
            warnings.extend(self.add_readme_from_repo(&mut meta, maybe_repo.as_ref()));
        }

        meta.readme.as_mut().map(|readme| {
            readme.base_url = Some(repo.readme_base_url(&path_in_repo));
            readme.base_image_url = Some(repo.readme_base_image_url(&path_in_repo));
        });

        self.rich_crate_version_data_common(origin.clone(), meta, 0, false, warnings)
    }

    pub fn tarball(&self, name: &str, ver: &str) -> CResult<Vec<u8>> {
        let tarball = self.crates_io.crate_data(name, ver)
            .context("crate_file")?
            .ok_or_else(|| KitchenSinkErr::DataNotFound(format!("{}-{}", name, ver)))?;
        Ok(tarball)
    }

    fn rich_crate_version_data_from_crates_io(&self, latest: &crates_index::Version) -> CResult<(CrateVersionSourceData, Manifest, Warnings)> {
        let mut warnings = HashSet::new();

        let name = latest.name();
        let ver = latest.version();
        let origin = Origin::from_crates_io_name(name);

        let (crate_tarball, crates_io_meta) = rayon::join(
            || self.tarball(name, ver),
            || self.crates_io_meta(&name.to_ascii_lowercase()));

        let crates_io_meta = crates_io_meta?.meta.krate;
        let crate_tarball = crate_tarball?;
        let crate_compressed_size = crate_tarball.len();
        let mut meta = crate::tarball::read_archive(&crate_tarball[..], name, ver)?;
        drop(crate_tarball);

        let package = meta.manifest.package.as_mut().ok_or_else(|| KitchenSinkErr::NotAPackage(origin.clone()))?;

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

        let maybe_repo = package.repository.as_ref().and_then(|r| Repo::new(r).ok());
        let has_readme_file = meta.readme.is_some();
        if !has_readme_file {
            let has_readme_prop = meta.manifest.package.as_ref().map_or(false, |p| p.readme.is_some());
            if has_readme_prop {
                warnings.insert(Warning::NoReadmePackaged);
            } else {
                warnings.insert(Warning::NoReadmeProperty);
            }
            // readmes in form of readme="../foo.md" are lost in packaging,
            // and the only copy exists in crates.io own api
            self.add_readme_from_crates_io(&mut meta, name, ver);
            let has_readme = meta.readme.is_some();
            if !has_readme {
                warnings.extend(self.add_readme_from_repo(&mut meta, maybe_repo.as_ref()));
            }
        }

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

        self.rich_crate_version_data_common(origin, meta, crate_compressed_size as u32, latest.is_yanked(), warnings)
    }

    ///// Fixing and faking the data
    fn rich_crate_version_data_common(&self, origin: Origin, mut meta: CrateFile, crate_compressed_size: u32, is_yanked: bool, mut warnings: Warnings) -> CResult<(CrateVersionSourceData, Manifest, Warnings)> {
        Self::override_bad_categories(&mut meta.manifest);

        let mut github_keywords = None;

        let package = meta.manifest.package.as_mut().ok_or_else(|| KitchenSinkErr::NotAPackage(origin.clone()))?;
        let maybe_repo = package.repository.as_ref().and_then(|r| Repo::new(r).ok());
        // Guess keywords if none were specified
        // TODO: also ignore useless keywords that are unique db-wide
        let gh = match maybe_repo.as_ref() {
            Some(repo) => if let RepoHost::GitHub(ref gh) = repo.host() {
                self.gh.topics(gh, &self.cachebust_string_for_repo(repo).context("fetch topics")?)?
            } else {None},
            _ => None,
        };
        if let Some(mut topics) = gh {
            for t in &mut topics {
                if t.starts_with("rust-") {
                    *t = t.trim_start_matches("rust-").into();
                }
            }
            topics.retain(|t| match t.as_str() {
                "rust" | "rs" | "rustlang" | "rust-lang" | "crate" | "crates" | "library" => false,
                _ => true,
            });
            if !topics.is_empty() {
                github_keywords = Some(topics);
            }
        }

        if let Origin::CratesIo(_) = &origin {
            // Delete the original docs.rs link, because we have our own
            // TODO: what if the link was to another crate or a subpage?
            if package.documentation.as_ref().map_or(false, |s| Self::is_docs_rs_link(s)) {
                if self.has_docs_rs(&origin, &package.name, &package.version) {
                    package.documentation = None; // docs.rs is not proper docs
                }
            }
        }

        warnings.extend(self.remove_redundant_links(package, maybe_repo.as_ref()));

        let mut github_description = None;
        let mut github_name = None;
        if let Some(ref crate_repo) = maybe_repo {
            if let Some(ghrepo) = self.github_repo(crate_repo)? {
                if package.homepage.is_none() {
                    if let Some(url) = ghrepo.homepage {
                        let also_add_docs = package.documentation.is_none() && ghrepo.github_page_url.as_ref().map_or(false, |p| p != &url);
                        package.homepage = Some(url);
                        if also_add_docs {
                            if let Some(url) = ghrepo.github_page_url {
                                package.documentation = Some(url);
                            }
                        }
                    } else if let Some(url) = ghrepo.github_page_url {
                        package.homepage = Some(url);
                    }
                    warnings.extend(self.remove_redundant_links(package, maybe_repo.as_ref()));
                }
                if package.description.is_none() {
                    package.description = ghrepo.description;
                } else {
                    github_description = ghrepo.description;
                }
                github_name = Some(ghrepo.name);
            }
        }

        // lib file takes majority of space in cache, so remove it if it won't be used
        if !self.is_readme_short(meta.readme.as_ref()) {
            meta.lib_file = None;
        }

        // Process crate's text to guess non-lowercased name
        let mut words = vec![package.name.as_str()];
        let readme_txt;
        if let Some(ref r) = meta.readme {
            readme_txt = render_readme::Renderer::new(None).visible_text(&r.markup);
            words.push(&readme_txt);
        }
        if let Some(ref s) = package.description {words.push(s);}
        if let Some(ref s) = github_description {words.push(s);}
        if let Some(ref s) = github_name {words.push(s);}
        if let Some(ref s) = package.homepage {words.push(s);}
        if let Some(ref s) = package.documentation {words.push(s);}
        if let Some(ref s) = package.repository {words.push(s);}

        let capitalized_name = Self::capitalized_name(&package.name, words.into_iter());

        let has_buildrs = meta.has("build.rs");
        let has_code_of_conduct = meta.has("CODE_OF_CONDUCT.md") || meta.has("docs/CODE_OF_CONDUCT.md") || meta.has(".github/CODE_OF_CONDUCT.md");
        let src = CrateVersionSourceData {
            capitalized_name,
            language_stats: meta.language_stats,
            crate_compressed_size,
            // sometimes uncompressed sources without junk are smaller than tarball with junk
            crate_decompressed_size: (meta.decompressed_size as u32).max(crate_compressed_size),
            is_nightly: meta.is_nightly,
            has_buildrs,
            has_code_of_conduct,
            readme: meta.readme,
            lib_file: meta.lib_file.map(|s| s.into()),
            github_description,
            github_keywords,
            is_yanked,
        };

        Ok((src, meta.manifest, warnings))
    }

    fn override_bad_categories(manifest: &mut Manifest) {
        let direct_dependencies = &manifest.dependencies;
        let has_cargo_bin = manifest.has_cargo_bin();
        let package = manifest.package.as_mut().expect("pkg");
        let eq = |a:&str,b:&str| -> bool {a.eq_ignore_ascii_case(b)};

        for cat in &mut package.categories {
            if cat.as_bytes().iter().any(|c| !c.is_ascii_lowercase()) {
                *cat = cat.to_lowercase();
            }
            if cat == "development-tools" || (cat == "command-line-utilities" && has_cargo_bin) {
                if package.keywords.iter().any(|k| k.eq_ignore_ascii_case("cargo-subcommand") || k.eq_ignore_ascii_case("subcommand")) {
                    *cat = "development-tools::cargo-plugins".into();
                }
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
                if package.keywords.iter().any(|k| eq(k,"bitcoin") || eq(k,"ethereum") || eq(k,"ledger") || eq(k,"exonum") || eq(k,"blockchain")) {
                    *cat = "cryptography::cryptocurrencies".into();
                }
            }
            if cat == "games" {
                if package.keywords.iter().any(|k| {
                    k == "game-dev" || k == "game-development" || eq(k,"gamedev") || eq(k,"framework") || eq(k,"utilities") || eq(k,"parser") || eq(k,"api")
                }) {
                    *cat = "game-engines".into();
                }
            }
            if cat == "science" || cat == "algorithms" {
                if package.keywords.iter().any(|k| k == "neural-network" || eq(k,"machine-learning") || eq(k,"neuralnetworks") || eq(k,"neuralnetwork") || eq(k,"tensorflow") || eq(k,"deep-learning")) {
                    *cat = "science::ml".into();
                } else if package.keywords.iter().any(|k| {
                    k == "math" || eq(k,"calculus") || eq(k,"algebra") || eq(k,"linear-algebra") || eq(k,"mathematics") || eq(k,"maths") || eq(k,"number-theory")
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

    pub fn is_build_or_dev(&self, k: &Origin) -> Result<(bool, bool), KitchenSinkErr> {
        Ok(self.crates_io_dependents_stats_of(k)?
        .map(|d| {
            // direct deps are more relevant, but sparse data gives wrong results
            let direct_weight = 1 + d.direct.all()/4;

            let build = d.direct.build as u32 * direct_weight + d.build.def as u32 * 2 + d.build.opt as u32;
            let runtime = d.direct.runtime as u32 * direct_weight + d.runtime.def as u32 * 2 + d.runtime.opt as u32;
            let dev = d.direct.dev as u32 * direct_weight + d.dev as u32 * 2;
            let is_build = build > 3 * (runtime + 15); // fudge factor, don't show anything if data is uncertain
            let is_dev = !is_build && dev > (3 * runtime + 3 * build + 15);
            (is_build, is_dev)
        })
        .unwrap_or((false, false)))
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
        res
    }

    fn is_docs_rs_link(d: &str) -> bool {
        let d = d.trim_start_matches("http://").trim_start_matches("https://");
        d.starts_with("docs.rs/") || d.starts_with("crates.fyi/")
    }

    /// name is case-sensitive!
    pub fn has_docs_rs(&self, origin: &Origin, name: &str, ver: &str) -> bool {
        match origin {
            Origin::CratesIo(_) => self.docs_rs.builds(name, ver).unwrap_or(true), // fail open
            _ => false,
        }
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

    pub fn all_dependencies_flattened(&self, krate: &RichCrateVersion) -> Result<DepInfMap, KitchenSinkErr> {
        match krate.origin() {
            Origin::CratesIo(name) => {
                self.index.all_dependencies_flattened(self.index.crates_io_crate_by_lowercase_name(name)?)
            },
            _ => {
                self.index.all_dependencies_flattened(krate)
            }
        }
    }

    pub fn prewarm(&self) {
        let _ = self.index.deps_stats();
    }

    pub fn update(&self) {
        self.index.update();
        let _ = self.index.deps_stats();
    }

    pub fn crates_io_dependents_stats_of(&self, origin: &Origin) -> Result<Option<&RevDependencies>, KitchenSinkErr> {
        match origin {
            Origin::CratesIo(crate_name) => Ok(self.index.deps_stats()?.counts.get(crate_name)),
            _ => Ok(None),
        }
    }

    /// (latest, pop)
    /// 0 = not used
    /// 1 = everyone uses it
    pub fn version_popularity(&self, crate_name: &str, requirement: &VersionReq) -> Result<Option<(bool, f32)>, KitchenSinkErr> {
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
        krate.category_slugs()
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
        self.crate_db.index_versions(k, score, self.downloads_recent(k.origin())?)?;
        Ok(())
    }

    pub fn index_crate_downloads(&self, crates_io_name: &str, by_day: &HashMap<Date<Utc>, u32>) -> CResult<()> {
        if stopped() {Err(KitchenSinkErr::Stopped)?;}
        let mut modified = false;
        let mut curr_year = Utc::today().year() as u16;
        let mut curr_year_data = self.yearly.get_crate_year(crates_io_name, curr_year)?.unwrap_or_default();
        for (day, &dls) in by_day {
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

            if curr_year_data.0[day_of_year] != dls {
                modified = true;
                curr_year_data.0[day_of_year] = dls;
            }
        }
        if modified {
            self.yearly.set_crate_year(crates_io_name, curr_year, &curr_year_data)?;
        }
        Ok(())
    }

    pub fn index_crate_highest_version(&self, origin: &Origin) -> CResult<()> {
        if stopped() {Err(KitchenSinkErr::Stopped)?;}

        let (src, manifest, _warn) = match origin {
            Origin::CratesIo(ref name) => {
                let ver = self.index.crate_highest_version(name, false).context("rich_crate_version2")?;
                self.rich_crate_version_data_from_crates_io(ver).context("rich_crate_version_data_from_crates_io")?
            },
            Origin::GitHub {..} | Origin::GitLab {..} => {
                if !self.crate_exists(origin) {
                    Err(KitchenSinkErr::GitCrateNotAllowed(origin.to_owned()))?
                }
                self.rich_crate_version_from_repo(&origin)?
            },
        };

        // direct deps are used as extra keywords for similarity matching,
        // but we're taking only niche deps to group similar niche crates together
        let raw_deps_stats = self.index.deps_stats()?;
        let mut weighed_deps = Vec::<(&str, f32)>::new();
        let all_deps = manifest.direct_dependencies()?;
        let all_deps = [(all_deps.0, 1.0), (all_deps.2, 0.33)];
        // runtime and (lesser) build-time deps
        for (deps, overall_weight) in all_deps.iter() {
            for dep in deps {
                if let Some(rev) = raw_deps_stats.counts.get(dep.package.as_str()) {
                    let right_popularity = rev.direct.all() > 1 && rev.direct.all() < 150 && rev.runtime.def < 500 && rev.runtime.opt < 800;
                    if Self::dep_interesting_for_index(dep.package.as_str()).unwrap_or(right_popularity) {
                        let weight = overall_weight / (1 + rev.direct.all()) as f32;
                        weighed_deps.push((dep.package.as_str(), weight));
                    }
                }
            }
        }
        let (is_build, is_dev) = self.is_build_or_dev(origin)?;
        let package = manifest.package();
        let readme_text = src.readme.as_ref().map(|r| render_readme::Renderer::new(None).visible_text(&r.markup));
        let repository = package.repository.as_ref().and_then(|r| Repo::new(r).ok());
        let authors = package.authors.iter().map(|a| Author::new(a)).collect::<Vec<_>>();

        let tmp;
        let category_slugs = if let Some(overrides) = self.category_overrides.get(origin.short_crate_name()) {
            &overrides
        } else {
            tmp = categories::Categories::fixed_category_slugs(&package.categories);
            &tmp
        };

        self.crate_db.index_latest(CrateVersionData {
            readme_text,
            category_slugs,
            authors: &authors,
            origin,
            repository: repository.as_ref(),
            deps_stats: &weighed_deps,
            is_build, is_dev,
            manifest: &manifest,
            derived: &src,
        })?;
        Ok(())
    }

    fn capitalized_name<'a>(name: &str, source_words: impl Iterator<Item = &'a str>) -> String {
        let mut first_capital = String::with_capacity(name.len());
        let mut ch = name.chars();
        if let Some(f) = ch.next() {
            first_capital.extend(f.to_uppercase());
            first_capital.extend(ch.map(|c| if c == '_' {' '} else {c}));
        }

        let mut words = HashMap::with_capacity(100);
        let lcname = name.to_lowercase();
        let shouty = name.to_uppercase();
        for s in source_words {
            for s in s.split(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_').filter(|&w| w != lcname && w.eq_ignore_ascii_case(&lcname)) {
                let mut points = 2;
                if lcname.len() > 2 {
                    if s[1..] != lcname[1..] {
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
        }

        if let Some((name, _)) = words.into_iter().max_by_key(|&(_, v)| v) {
            name
        } else {
            first_capital
        }
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

    pub fn inspect_repo_manifests(&self, repo: &Repo) -> CResult<Vec<(String, crate_git_checkout::Oid, Manifest)>> {
        let checkout = crate_git_checkout::checkout(repo, &self.git_checkout_path)?;
        let (has, _) = crate_git_checkout::find_manifests(&checkout)?;
        Ok(has)
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
        let manif = manif.into_iter().filter_map(|(subpath, _, manifest)| {
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

    pub fn rustc_compatibility(&self, origin: &Origin) -> CResult<Vec<CompatibilityInfo>> {
        let db = BuildDb::new(self.main_cache_dir().join("builds.db"))?;
        Ok(db.get_compat(origin)?)
    }

    /// List of all notable crates
    /// Returns origin, rank, last updated unix timestamp
    pub fn sitemap_crates(&self) -> CResult<Vec<(Origin, f64, i64)>> {
        Ok(self.crate_db.sitemap_crates()?)
    }

    /// If given crate is a sub-crate, return crate that owns it.
    /// The relationship is based on directory layout of monorepos.
    pub fn parent_crate(&self, child: &RichCrateVersion) -> Option<Origin> {
        let repo = child.repository()?;
        self.crate_db.parent_crate(repo, child.short_name()).ok()?
    }

    pub fn cachebust_string_for_repo(&self, crate_repo: &Repo) -> CResult<String> {
        Ok(self.crate_db.crates_in_repo(crate_repo)
            .context("db crates_in_repo")?
            .into_iter()
            .filter_map(|origin| {
                match origin {
                    Origin::CratesIo(name) => self.index.crate_highest_version(&name, false).ok(),
                    _ => None,
                }
            })
            .map(|c| c.version().to_string())
            .next()
            .unwrap_or_else(|| {
                let weeks = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).expect("clock").as_secs() / (3600*24*7);
                format!("w{}", weeks)
            }))
    }

    pub fn user_github_orgs(&self, github_login: &str) -> CResult<Option<Vec<UserOrg>>> {
        Ok(self.gh.user_orgs(github_login)?)
    }

    /// Returns (contrib, github user)
    fn contributors_from_repo(&self, crate_repo: &Repo, owners: &[CrateOwner], found_crate_in_repo: bool) -> CResult<(bool, HashMap<String, (f64, User)>)> {
        let mut hit_max_contributor_count = false;
        match crate_repo.host() {
            // TODO: warn on errors?
            RepoHost::GitHub(ref repo) => {
                if !found_crate_in_repo && !owners.iter().any(|owner| owner.login.eq_ignore_ascii_case(&repo.owner)) {
                    return Ok((false, HashMap::new()));
                }

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
                Ok((hit_max_contributor_count, by_login))
            },
            RepoHost::BitBucket(..) |
            RepoHost::GitLab(..) |
            RepoHost::Other => Ok((false, HashMap::new())), // TODO: could use git checkout...
        }
    }

    /// Merge authors, owners, contributors
    pub fn all_contributors<'a>(&self, krate: &'a RichCrateVersion) -> CResult<(Vec<CrateAuthor<'a>>, Vec<CrateAuthor<'a>>, bool, usize)> {
        let owners = self.crate_owners(krate)?;

        let (hit_max_contributor_count, mut contributors_by_login) = match krate.repository().as_ref() {
            // Only get contributors from github if the crate has been found in the repo,
            // otherwise someone else's repo URL can be used to get fake contributor numbers
            Some(crate_repo) => self.contributors_from_repo(crate_repo, &owners, krate.has_path_in_repo())?,
            None => (false, HashMap::new()),
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


        for owner in owners {
            if let Ok(user) = self.owners_github(&owner) {
                match authors.entry(AuthorId::GitHub(user.id)) {
                    Occupied(mut e) => {
                        let e = e.get_mut();
                        e.owner = true;
                        if e.info.is_none() {
                            e.info = Some(Cow::Owned(Author{
                                name: Some(owner.name().to_owned()),
                                email: None,
                                url: Some(owner.url.clone()),
                            }));
                        }
                        if e.github.is_none() {
                            e.github = Some(user);
                        } else if let Some(ref mut gh) = e.github {
                            if gh.name.is_none() {
                                gh.name = Some(owner.name().to_owned());
                            }
                        }
                    },
                    Vacant(e) => {
                        e.insert(CrateAuthor {
                            contribution: 0.,
                            github: Some(user),
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
            let mut lc_ascii_name = deunicode::deunicode(a.name());
            lc_ascii_name.make_ascii_lowercase();
            match authors_by_name.entry(lc_ascii_name) {
                Occupied(mut e) => {
                    let e = e.get_mut();
                    if let (Some(e), Some(a)) = (e.github.as_ref(), a.github.as_ref()) {
                         // different users? may fail on stale/renamed login name
                        if !e.login.eq_ignore_ascii_case(&a.login) {
                            continue;
                        }
                    }
                    if e.github.is_none() {
                        e.github = a.github;
                    }
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

    fn owners_github(&self, owner: &CrateOwner) -> CResult<User> {
        // this is silly, but crates.io doesn't keep the github ID explicitly
        // (the id field is crates-io's field), but it does keep the avatar URL
        // which contains github's ID
        if let Some(ref avatar) = owner.avatar {
            lazy_static::lazy_static! {
                static ref R: regex::Regex = regex::Regex::new("https://avatars[0-9]+.githubusercontent.com/u/([0-9]+)").expect("regex");
            }
            if let Some(c) = R.captures(avatar) {
                let id = c.get(1).expect("regex").as_str();
                let id = id.parse().expect("regex");
                if let Some(user) = self.gh.user_by_id(id)? {
                    return Ok(user);
                }
            }
        }
        // This is a bit weak, since logins are not permanent
        if let Some(user) = self.gh.user_by_login(owner.github_login().ok_or(KitchenSinkErr::OwnerWithoutLogin)?)? {
            return Ok(user);
        }
        Err(KitchenSinkErr::OwnerWithoutLogin)?
    }

    fn crate_owners(&self, krate: &RichCrateVersion) -> CResult<Vec<CrateOwner>> {
        match krate.origin() {
            Origin::CratesIo(name) => self.crates_io_crate_owners(name, krate.version()),
            Origin::GitLab {..} => Ok(vec![]),
            Origin::GitHub {repo, ..} => Ok(vec![
                CrateOwner {
                    id: 0,
                    avatar: None,
                    // FIXME: read from GH
                    url: format!("https://github.com/{}", repo.owner),
                    // FIXME: read from GH
                    login: repo.owner.to_string(),
                    kind: OwnerKind::User, // FIXME: crates-io uses teams, and we'd need to find the right team? is "owners" a guaranteed thing?
                    name: None,
                }
            ]),
        }
    }

    pub fn crates_io_crate_owners(&self, crate_name: &str, version: &str) -> CResult<Vec<CrateOwner>> {
        Ok(self.crates_io.crate_owners(crate_name, version).context("crate_owners")?.unwrap_or_default())
    }

    // Sorted from the top, returns origins
    pub fn top_crates_in_category(&self, slug: &str) -> CResult<Arc<Vec<Origin>>> {
        {
            let cache = self.top_crates_cached.read();
            if let Some(category) = cache.get(slug) {
                return Ok(category.clone());
            }
        }
        let total_count = self.category_crate_count(slug)?;
        let wanted_num = ((total_count/2+25)/50 * 50).max(100);
        let mut cache = self.top_crates_cached.write();
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
        let mut keywords: Vec<_> = krate.keywords()
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
            return Ok(300);
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
fn is_build_or_dev_test() {
    let c = KitchenSink::new_default().expect("uhg");
    assert_eq!((false, false), c.is_build_or_dev(&Origin::from_crates_io_name("semver")).unwrap());
    assert_eq!((false, true), c.is_build_or_dev(&Origin::from_crates_io_name("version-sync")).unwrap());
    assert_eq!((true, false), c.is_build_or_dev(&Origin::from_crates_io_name("cc")).unwrap());
}

#[test]
fn fetch_uppercase_name() {
    let k = KitchenSink::new_default().expect("Test if configured");
    let _ = k.rich_crate(&Origin::from_crates_io_name("Inflector")).unwrap();
    let _ = k.rich_crate(&Origin::from_crates_io_name("inflector")).unwrap();
}
