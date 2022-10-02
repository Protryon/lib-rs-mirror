#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate log;

mod yearly;
use crate_db::builddb::RustcMinorVersion;
use crate_git_checkout::FoundManifest;
use event_log::EventLog;
use anyhow::Context;
use feat_extractor::is_deprecated_requirement;
use futures::TryFutureExt;
use tokio::time::Instant;

pub use crate::yearly::*;
pub use deps_index::*;
use futures::future::BoxFuture;
use futures::FutureExt;
use tokio::task::spawn_blocking;
pub mod filter;

mod ctrlcbreak;
pub use crate::ctrlcbreak::*;
mod nonblock;
pub use crate::nonblock::*;

pub use crate_db::builddb::Compat;
pub use crate_db::builddb::CompatByCrateVersion;
pub use crate_db::builddb::CompatRanges;
pub use crate_db::CrateOwnerRow;
pub use crates_io_client::CrateDepKind;
pub use crates_io_client::CrateDependency;
use crates_io_client::CrateMetaFile;
pub use crates_io_client::CrateMetaVersion;
pub use crates_io_client::CrateOwner;
pub use crates_io_client::OwnerKind;
pub use creviews::Level;
pub use creviews::Rating;
pub use creviews::Review;
pub use creviews::security::Advisory;
pub use creviews::security::Severity;
pub use github_info::Org;
pub use github_info::User;
pub use github_info::UserOrg;
pub use github_info::UserType;
pub use rich_crate::DependerChangesMonthly;
pub use rich_crate::Edition;
pub use rich_crate::Derived;
pub use rich_crate::MaintenanceStatus;
use rich_crate::ManifestExt;
pub use rich_crate::Markup;
pub use rich_crate::Origin;
pub use rich_crate::RichCrate;
pub use rich_crate::RichCrateVersion;
pub use rich_crate::RichDep;
pub use rich_crate::{Cfg, Target};
pub use semver::Version as SemVer;

use tarball::CrateFilesSummary;
use cargo_toml::Manifest;
use cargo_toml::Package;
use categories::Category;
use chrono::prelude::*;
use chrono::DateTime;
use crate_db::{builddb::BuildDb, CrateDb, CrateVersionData, RepoChange};
use creviews::Creviews;
use double_checked_cell_async::DoubleCheckedCell;
use futures::future::join_all;
use futures::stream::StreamExt;
use futures::Future;
use github_info::GitCommitAuthor;
use github_info::GitHubRepo;
use github_info::MinimalUser;
use once_cell::sync::OnceCell;
use parking_lot::RwLock;
use rayon::prelude::*;
use repo_url::Repo;
use repo_url::RepoHost;
use repo_url::SimpleRepo;
use rich_crate::Author;
pub use rich_crate::CrateVersion;
use rich_crate::CrateVersionSourceData;
use rich_crate::Readme;
pub use semver::VersionReq;
use simple_cache::SimpleCache;
use simple_cache::TempCache;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::cmp::Reverse;
use std::collections::hash_map::Entry::*;
use ahash::HashMap;
use ahash::HashSet;
use std::convert::TryInto;
use std::env;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Mutex;
use std::time::Duration;
use std::time::SystemTime;
use triomphe::Arc;
use smartstring::alias::String as SmolStr;
use ahash::HashMapExt;
use ahash::HashSetExt;

pub type ArcRichCrateVersion = Arc<RichCrateVersion>;

type FxHashMap<K, V> = std::collections::HashMap<K, V, ahash::RandomState>;

pub type CError = anyhow::Error;
pub type CResult<T> = Result<T, CError>;
pub type KResult<T> = Result<T, KitchenSinkErr>;
pub type Warnings = HashSet<Warning>;

#[derive(Debug, Clone, Serialize, thiserror::Error, Deserialize, Hash, Eq, PartialEq)]
pub enum Warning {
    #[error("`Cargo.toml` doesn't have `repository` property")]
    NoRepositoryProperty,
    #[error("`Cargo.toml` doesn't have `[package]` section")]
    NotAPackage,
    #[error("`Cargo.toml` doesn't have `readme` property")]
    NoReadmeProperty,
    #[error("`readme` property points to a file that hasn't been published")]
    NoReadmePackaged,
    #[error("Can't find README in repository: {}", _0)]
    NoReadmeInRepo(Box<str>),
    #[error("Readme has a problematic path: {}", _0)]
    EscapingReadmePath(Box<str>),
    #[error("Could not clone repository: {}", _0)]
    ErrorCloning(Box<str>),
    #[error("The crate is not in the repository")]
    NotFoundInRepo,
    #[error("{} URL is a broken link: {}", _0, _1)]
    BrokenLink(Box<str>, Box<str>),
    #[error("Bad category: {}", _0)]
    BadCategory(Box<str>),
    #[error("No categories specified")]
    NoCategories,
    #[error("No keywords specified")]
    NoKeywords,
    #[error("Edition {:?}, but MSRV {}", _0, _1)]
    EditionMSRV(Edition, u16),
    #[error("Bad MSRV: needs {}, but has {}", _0, _1)]
    BadMSRV(u16, u16),
    #[error("docs.rs did not build")]
    DocsRs,
    #[error("Dependency {} v{} is outdated ({}%)", _0, _1, _2)]
    OutdatedDependency(Box<str>, Box<str>, u8),
    #[error("Dependency {} v{} is deprecated", _0, _1)]
    DeprecatedDependency(Box<str>, Box<str>),
    #[error("Dependency {} has bad requirement {}", _0, _1)]
    BadRequirement(Box<str>, Box<str>),
    #[error("Dependency {} has exact requirement {}", _0, _1)]
    ExactRequirement(Box<str>, Box<str>),
    // bool = is breaking semver
    #[error("Dependency {} has imprecise requirement {}", _0, _1)]
    LaxRequirement(Box<str>, Box<str>, bool),
    #[error("Version {} does not parse: {}", _0, _1)]
    BadSemVer(Box<str>, Box<str>),
    #[error("The crate is classified as a cryptocurrency-related")]
    CryptocurrencyBS,
    #[error("The crate tarball is big: {}MB", _0 / 1000 / 1000)]
    Chonky(u64),
    #[error("A *-sys crates without links property")]
    SysNoLinks,
    #[error("Squatted name")]
    Reserved,
    #[error("License is not an SPDX expression")]
    LicenseSpdxSyntax,
    /// last arg is severity 1-n
    #[error("It's been {} days since the last {}release", _0, if *_1 {"stable "} else {"pre"})]
    StaleRelease(u32, bool, u8)
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum KitchenSinkErr {
    #[error("git checkout meh")]
    GitCheckoutFailed,
    #[error("category count not found in crates db: {}", _0)]
    CategoryNotFound(String),
    #[error("category query failed")]
    CategoryQueryFailed,
    #[error("crate not found: {:?}", _0)]
    CrateNotFound(Origin),
    #[error("author not found: {}", _0)]
    AuthorNotFound(SmolStr),
    #[error("crate {} not found in repo {}", _0, _1)]
    CrateNotFoundInRepo(String, String),
    #[error("crate is not a package: {:?}", _0)]
    NotAPackage(Origin),
    #[error("data not found, wanted {}", _0)]
    DataNotFound(String),
    #[error("tarball unarchiving error in {}", _0)]
    UnarchiverError(String, #[source] Arc<tarball::UnarchiverError>),
    #[error("crate has no versions")]
    NoVersions,
    #[error("cached data has different version than the index")]
    CacheExpired,
    #[error("Environment variable CRATES_DATA_DIR is not set.\nChoose a dir where it's OK to store lots of data, and export it like CRATES_DATA_DIR=/var/lib/crates.rs")]
    CratesDataDirEnvVarMissing,
    #[error("{} does not exist\nPlease get data files from https://lib.rs/data and put them in that directory, or set CRATES_DATA_DIR to their location.", _0)]
    CacheDbMissing(String),
    #[error("Error when parsing verison")]
    SemverParsingError,
    #[error("Stopped")]
    Stopped,
    #[error("Deps stats timeout")]
    DepsNotAvailable,
    #[error("Crate data timeout")]
    DataTimedOut,
    #[error("{} timed out after {}s", _0, _1)]
    TimedOut(&'static str, u16),
    #[error("Crate derived cache timeout")]
    DerivedDataTimedOut,
    #[error("Missing github login for crate owner")]
    OwnerWithoutLogin,
    #[error("Git index parsing failed: {}", _0)]
    GitIndexParse(String),
    #[error("Git index {:?}: {}", _0, _1)]
    GitIndexFile(PathBuf, String),
    #[error("Git crate '{:?}' can't be indexed, because it's not on the list", _0)]
    GitCrateNotAllowed(Origin),
    #[error("Deps err: {}", _0)]
    Deps(#[from] DepsErr),
    #[error("Bad rustc compat data")]
    BadRustcCompatData,
    #[error("bad cache: {}", _0)]
    BorkedCache(String),
    #[error("Event log error")]
    Event(#[from] #[source] Arc<event_log::Error>),
    #[error("RustSec: {}", _0)]
    RustSec(#[from] #[source] Arc<creviews::security::Error>),
    #[error("Internal error: {}", _0)]
    Internal(std::sync::Arc<dyn std::error::Error + Send + Sync>),
}

impl From<crates_io_client::Error> for KitchenSinkErr {
    #[cold]
    fn from(e: crates_io_client::Error) -> Self {
        match e {
            crates_io_client::Error::NotInCache => Self::CacheExpired,
            other => Self::BorkedCache(other.to_string()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DownloadWeek {
    pub date: Date<Utc>,
    pub total: usize,
    pub downloads: HashMap<Option<usize>, usize>,
}

/// bucket (like age, number of releases) -> (number of crates in the bucket, sample of those crate names)
pub type StatsHistogram = HashMap<u32, (u32, Vec<String>)>;

/// This is a collection of various data sources. It mostly acts as a starting point and a factory for other objects.
pub struct KitchenSink {
    pub index: Arc<Index>,
    crates_io: crates_io_client::CratesIoClient,
    docs_rs: docs_rs_client::DocsRsClient,
    url_check_cache: TempCache<(bool, u8)>,
    readme_check_cache: TempCache<()>,
    canonical_http_of_crate_at_version_cache: TempCache<String>,
    pub crate_db: CrateDb,
    derived_storage: SimpleCache,
    user_db: user_db::UserDb,
    gh: github_info::GitHub,
    loaded_rich_crate_version_cache: RwLock<FxHashMap<Origin, ArcRichCrateVersion>>,
    category_crate_counts: DoubleCheckedCell<Option<HashMap<String, (u32, f64)>>>,
    top_crates_cached: Mutex<FxHashMap<String, Arc<DoubleCheckedCell<Arc<Vec<Origin>>>>>>,
    git_checkout_path: PathBuf,
    yearly: AllDownloads,
    category_overrides: HashMap<SmolStr, Vec<SmolStr>>,
    crates_io_owners_cache: TempCache<Vec<CrateOwner>>,
    depender_changes: TempCache<Vec<DependerChanges>>,
    stats_histograms: TempCache<StatsHistogram>,
    throttle: tokio::sync::Semaphore,
    auto_indexing_throttle: tokio::sync::Semaphore,
    crev: Arc<Creviews>,
    rustsec: Arc<Mutex<creviews::security::RustSec>>,
    crate_rustc_compat_cache: RwLock<HashMap<Origin, CompatByCrateVersion>>,
    crate_rustc_compat_db: OnceCell<BuildDb>,
    data_path: PathBuf,
    /// login -> reason
    pub author_shitlist: HashMap<SmolStr, SmolStr>,
    event_log: EventLog<SharedEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SharedEvent {
    // Origin serialized
    CrateIndexed(String),
    // Origin serialized
    CrateNeedsReindexing(String),
    /// Newer crates.io release found
    CrateUpdated(String),
    DailyStatsUpdated,
}

impl KitchenSink {
    /// Use env vars to find data directory and config
    pub async fn new_default() -> CResult<Self> {
        let github_token = match env::var("GITHUB_TOKEN") {
            Ok(t) => t,
            Err(_) => {
                warn!("warning: Environment variable GITHUB_TOKEN is not set.\nGet token from https://github.com/settings/tokens and export GITHUB_TOKEN=…\nWithout it some requests will fail and new crates won't be analyzed properly.");
                "".to_owned()
            },
        };
        let data_path = Self::data_path().context("can't get data path")?;
        Self::new(&data_path, &github_token).await
    }

    pub async fn new(data_path: &Path, github_token: &str) -> CResult<Self> {
        let _ = env_logger::Builder::from_default_env()
            .filter(Some("html5ever"), log::LevelFilter::Error)
            .filter(Some("html5ever::tree_builder"), log::LevelFilter::Error)
            .filter(Some("html5ever::tokenizer::char_ref"), log::LevelFilter::Error)
            .filter(Some("tantivy"), log::LevelFilter::Error)
            .try_init();

        let ((crates_io, gh), (index, (crev, rustsec))) = tokio::task::spawn_blocking({
            let data_path = data_path.to_owned();
            let github_token = github_token.to_owned();
            let ghdb = data_path.join("github.db");
            move || {
                rayon::join(|| rayon::join(
                    || crates_io_client::CratesIoClient::new(&data_path),
                    move || github_info::GitHub::new(&ghdb, &github_token)),
                || rayon::join(|| Index::new(&data_path), || rayon::join(Creviews::new, || creviews::security::RustSec::new(&data_path))))
            }
        }).await?;

        let synonyms = categories::Synonyms::new(data_path)?;

        tokio::task::block_in_place(move || Ok(Self {
            crev: Arc::new(crev.context("crev")?),
            rustsec: Arc::new(Mutex::new(rustsec.map_err(std::sync::Arc::new)?)),
            crates_io: crates_io.context("crates_io")?,
            index: Arc::new(index?),
            url_check_cache: TempCache::new(&data_path.join("url_check2.db")).context("urlcheck")?,
            readme_check_cache: TempCache::new(&data_path.join("readme_check.db")).context("readmecheck")?,
            canonical_http_of_crate_at_version_cache: TempCache::new(&data_path.join("canonical_http_url_at.db")).context("readmecheck")?,
            docs_rs: docs_rs_client::DocsRsClient::new(data_path.join("docsrs.db")).context("docs")?,
            crate_db: CrateDb::new_with_synonyms(&Self::assert_exists(data_path.join("crate_data.db"))?, synonyms).context("db")?,
            derived_storage: SimpleCache::new(data_path.join("derived.db"), true)?,
            user_db: user_db::UserDb::new(Self::assert_exists(data_path.join("users.db"))?).context("udb")?,
            gh: gh.context("gh")?,
            loaded_rich_crate_version_cache: RwLock::new(FxHashMap::default()),
            git_checkout_path: data_path.join("git"),
            category_crate_counts: DoubleCheckedCell::new(),
            top_crates_cached: Mutex::new(FxHashMap::default()),
            yearly: AllDownloads::new(data_path),
            category_overrides: Self::load_category_overrides(&data_path.join("category_overrides.txt")).context("cat")?,
            author_shitlist: Self::load_author_shitlist(&data_path.join("author_shitlist.txt"))?,
            crates_io_owners_cache: TempCache::new(&data_path.join("cio-owners.tmp")).context("tmp1")?,
            depender_changes: TempCache::new(&data_path.join("deps-changes2.tmp")).context("tmp2")?,
            stats_histograms: TempCache::new(&data_path.join("stats-histograms.tmp")).context("tmp3")?,
            throttle: tokio::sync::Semaphore::new(40),
            auto_indexing_throttle: tokio::sync::Semaphore::new(4),
            crate_rustc_compat_cache: RwLock::default(),
            crate_rustc_compat_db: OnceCell::new(),
            event_log: EventLog::new(data_path.join("event_log.db")).context("events")?,
            data_path: data_path.into(),
        }))
    }

    fn assert_exists(path: PathBuf) -> Result<PathBuf, KitchenSinkErr> {
        if !path.exists() {
            Err(KitchenSinkErr::CacheDbMissing(path.display().to_string()))
        } else {
            Ok(path)
        }
    }

    pub fn event_log(&self) -> &EventLog<SharedEvent> {
        &self.event_log
    }

    pub fn data_path() -> Result<PathBuf, KitchenSinkErr> {
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
        &self.data_path
    }

    fn load_author_shitlist(path: &Path) -> CResult<HashMap<SmolStr, SmolStr>> {
        let p = std::fs::read_to_string(path)?;
        let mut out = HashMap::with_capacity(10);
        for line in p.lines() {
            if line.starts_with('#') {
                continue;
            }
            let mut parts = line.splitn(2, ':');
            let login = parts.next().unwrap().trim();
            if login.is_empty() {
                continue;
            }
            let reason = parts.next().expect("shitlist broken").trim();
            let mut login = SmolStr::from(login);
            login.make_ascii_lowercase();
            out.insert(login, reason.into());
        }
        Ok(out)
    }

    fn load_category_overrides(path: &Path) -> CResult<HashMap<SmolStr, Vec<SmolStr>>> {
        let p = std::fs::read_to_string(path)?;
        let mut out = HashMap::new();
        for line in p.lines() {
            let mut parts = line.splitn(2, ':');
            let crate_name = parts.next().unwrap().trim();
            if crate_name.is_empty() {
                continue;
            }
            let categories: Vec<_> = parts.next().expect("overrides broken").split(',')
                .map(|s| s.trim().into()).collect();
            if categories.is_empty() {
                continue;
            }
            categories.iter().for_each(|k| debug_assert!(categories::CATEGORIES.from_slug(k).1, "'{}' is invalid for override '{}'", k, crate_name));
            out.insert(crate_name.into(), categories);
        }
        Ok(out)
    }

    pub fn is_crates_io_login_on_shitlist(&self, login: &str) -> bool {
        self.author_shitlist.get(login.to_ascii_lowercase().as_str()).is_some()
    }

    pub async fn is_crate_on_shitlist(&self, k: &RichCrate) -> bool {
        let owners = match self.crate_owners(k.origin(), CrateOwners::All).await {
            Ok(o) => o,
            Err(e) => {
                warn!("can't check owners of {:?}: {e}", k.origin());
                return false
            },
        };
        owners.iter()
            // some crates are co-owned by both legit and banned owners,
            // so banning by "any" would interfere with legit users' usage :(
            .all(|owner| {
                self.is_crates_io_login_on_shitlist(&owner.crates_io_login)
            })
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
    pub fn all_crates(&self) -> impl Iterator<Item = Origin> + '_ {
        self.index.all_crates()
    }

    /// Iterator over all crates available in the index
    ///
    /// It returns only identifiers,
    /// so `rich_crate`/`rich_crate_version` is needed to do more.
    pub fn all_crates_io_crates(&self) -> &FxHashMap<SmolStr, CratesIndexCrate> {
        self.index.crates_io_crates()
    }

    pub fn total_year_downloads(&self, year: u16) -> KResult<[u64; 366]> {
        Ok(self.yearly.total_year_downloads(year)?)
    }

    #[inline]
    fn summed_year_downloads(&self, crate_name: &str, curr_year: u16) -> KResult<[u32; 366]> {
        let curr_year_data = self.yearly.get_crate_year(crate_name, curr_year)?.unwrap_or_default();
        let mut summed_days = [0; 366];
        for (_, days) in curr_year_data {
            for (sd, vd) in summed_days.iter_mut().zip(days.0.iter()) {
                *sd += *vd;
            }
        }
        Ok(summed_days)
    }

    // Get top n crates-io crates with most sharply increasing downloads
    pub async fn trending_crates(&self, top_n: usize) -> Vec<(Origin, f64)> {
        let mut top = tokio::task::block_in_place(|| {
            self.trending_crates_raw(top_n)
        });
        let crates_present: HashSet<_> = top.iter().filter_map(|(o, _)| match o {
                Origin::CratesIo(name) => Some(name.clone()),
                _ => None,
        }).collect();
        if let Ok(stats) = self.index.deps_stats().await {
            for (o, score) in &mut top {
                if let Origin::CratesIo(name) = o {
                    if let Some(s) = stats.counts.get(&**name) {
                        // if it's a dependency of another top crate, its not trending, it's riding that crate
                        if s.rev_dep_names_default.iter().any(|parent| crates_present.contains(parent)) {
                            *score = 0.;
                        } else {
                            // it should be trending users, not just download hits
                            if s.direct.all() > 10 {
                                *score *= 1.1;
                            }
                            if s.direct.all() > 100 {
                                *score *= 1.1;
                            }
                        }
                    }
                }
            }

            top.retain(|&(_, score)| score > 0.);
        }

        // apply some rank weight to downgrade spam, and pull main crates before their -sys or -derive
        for (origin, score) in &mut top {
            *score *= 0.3 + self.crate_db.crate_rank(origin).await.unwrap_or(0.);
        }
        top.sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));

        watch("knock_duplicates", self.knock_duplicates(&mut top)).await;
        top.truncate(top_n);
        top
    }

    pub async fn crate_ranking_for_builder(&self, origin: &Origin) -> CResult<f64> {
        Ok(self.crate_db.crate_rank(origin).await?)
    }

    // actually gives 2*top_n…
    fn trending_crates_raw(&self, top_n: usize) -> Vec<(Origin, f64)> {
        let mut now = Utc::today();
        let mut day_of_year = now.ordinal0() as usize;
        if day_of_year < 7 {
            now = Utc::today() - chrono::Duration::days(7);
            day_of_year = now.ordinal0() as usize;
        }
        let curr_year = now.year() as u16;
        let shortlen = (day_of_year / 2).min(10);
        let longerlen = (day_of_year / 2).min(3 * 7);

        let missing_data_factor = longerlen as f32 / (3 * 7) as f32;
        let missing_data_factor = missing_data_factor.powi(2);

        fn average_nonzero(slice: &[u32], min_days: u32) -> f32 {
            let mut sum = 0u32;
            let mut n = 0u32;
            for val in slice.iter().copied().filter(|&n| n > 0) {
                sum += val;
                n += 1;
            }
            sum as f32 / (n.max(min_days) as f32) // too few days are too noisy, and div/0
        }

        let mut ratios = self.all_crates().par_bridge().filter_map(|origin| {
            match &origin {
                Origin::CratesIo(crate_name) => {
                    let d = self.summed_year_downloads(crate_name, curr_year).ok()?;
                    let prev_week_avg = average_nonzero(&d[day_of_year-shortlen*2 .. day_of_year-shortlen], 7);
                    if prev_week_avg < 70. * missing_data_factor { // it's too easy to trend from zero downloads!
                        return None;
                    }

                    let this_week_avg = average_nonzero(&d[day_of_year-shortlen .. day_of_year], 8);
                    if prev_week_avg * missing_data_factor >= this_week_avg {
                        return None;
                    }

                    let prev_4w_avg = average_nonzero(&d[day_of_year-longerlen*2 .. day_of_year-longerlen], 7).max(average_nonzero(&d[.. day_of_year-longerlen*2], 7));
                    let this_4w_avg = average_nonzero(&d[day_of_year-longerlen .. day_of_year], 14);
                    if prev_4w_avg * missing_data_factor >= this_4w_avg || prev_4w_avg * missing_data_factor >= prev_week_avg || prev_4w_avg * missing_data_factor >= this_week_avg {
                        return None;
                    }

                    let ratio1 = (800. + this_week_avg) / (900. + prev_week_avg) * prev_week_avg.sqrt().min(10.);
                    // 0.8, because it's less interesting
                    let ratio4 = 0.8 * (700. + this_4w_avg) / (600. + prev_4w_avg) * prev_4w_avg.sqrt().min(9.);

                    // combine short term and long term trends
                    Some((origin, ratio1, ratio4))
                },
                _ => None,
            }
        }).collect::<Vec<_>>();

        if ratios.is_empty() {
            warn!("no trending crates");
            return Vec::new();
        }
        ratios.sort_unstable_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal));
        let len = ratios.len();
        let mut top: Vec<_> = ratios.drain(len.saturating_sub(top_n)..).map(|(o, s, _)| (o, s as f64)).collect();
        ratios.sort_unstable_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(Ordering::Equal));
        let len = ratios.len();
        top.extend(ratios.drain(len.saturating_sub(top_n)..).map(|(o, _, s)| (o, s as f64)).take(top_n));
        top
    }

    // Monthly downloads, sampled from last few days or weeks
    pub async fn recent_downloads_by_version(&self, origin: &Origin) -> KResult<HashMap<MiniVer, u32>> {

        let now = Utc::today();
        let curr_year = now.year() as u16;
        let curr_year_data = match origin {
            Origin::CratesIo(name) => self.yearly.get_crate_year(name, curr_year)?.unwrap_or_default(),
            _ => return Ok(HashMap::new()),
        };

        let mut out = HashMap::new();
        let mut total = 0;
        let mut days = 0;
        let mut end_day = now.ordinal0() as usize; // we'll have garbage data in january…
        loop {
            let start_day = end_day.saturating_sub(4);
            days += end_day - start_day;

            for (ver, dl) in &curr_year_data {
                let cnt = out.entry(ver).or_insert(0);
                for d in dl.0[start_day..end_day].iter().copied() {
                    *cnt += d;
                    total += d;
                }
            }
            if start_day == 0 || total > 10000 || days >= 30 {
                break;
            }
            end_day = start_day;
        }

        // normalize data sample to be proportional to montly downloads
        let actual_downloads_per_month = self.downloads_per_month(origin).await?.unwrap_or(total as usize * 30 / days as usize);
        Ok(out.into_iter().map(|(k,v)|
            (k.clone(), (v as usize * actual_downloads_per_month / total.max(1) as usize) as u32)
        ).collect())
    }

    /// Gets cratesio download data, but not from the API, but from our local copy
    pub fn weekly_downloads(&self, k: &RichCrate, num_weeks: u16) -> CResult<Vec<DownloadWeek>> {
        let mut res = Vec::with_capacity(num_weeks.into());
        let mut now = Utc::today();

        let mut curr_year = now.year() as u16;
        let mut summed_days = self.summed_year_downloads(k.name(), curr_year)?;

        let day_of_year = now.ordinal0();
        let missing_data_days = summed_days[0..day_of_year as usize].iter().cloned().rev().take_while(|&s| s == 0).count().min(7);

        if missing_data_days > 0 {
            now -= chrono::Duration::days(missing_data_days as _);
        }

        for i in (1..=num_weeks).rev() {
            let date = now - chrono::Duration::weeks(i.into());
            let mut total = 0;
            let mut any_set = false;

            for d in 0..7 {
                let this_date = date + chrono::Duration::days(d);
                let year = this_date.year() as u16;
                if year != curr_year {
                    curr_year = year;
                    summed_days = self.summed_year_downloads(k.name(), curr_year)?;
                }
                let day_of_year = this_date.ordinal0() as usize;
                if summed_days[day_of_year] > 0 {
                    any_set = true;
                }
                total += summed_days[day_of_year] as usize;
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

    pub async fn crates_to_reindex(&self) -> CResult<Vec<RichCrate>> {
        let min_timestamp = self.crate_db.latest_crate_update_timestamp().await?.unwrap_or(0);
        tokio::task::yield_now().await;
        let all = tokio::task::block_in_place(|| {
            self.index.crates_io_crates() // too slow to scan all GH crates
        });
        let stream = futures::stream::iter(all.iter())
            .map(move |(name, _)| async move {
                self.rich_crate_async(&Origin::from_crates_io_name(name)).await.map_err(|e| error!("to reindex {}: {}", name, e)).ok()
            })
            .buffer_unordered(8)
            .filter_map(|x| async {x});
        let mut crates = stream.filter(move |k| {
            let latest = k.versions().iter().map(|v| &v.created_at).max();
            let res = if let Some(timestamp) = latest {
                timestamp.timestamp() >= min_timestamp as i64
            } else {
                true
            };
            async move { res }
        })
        .collect::<Vec<_>>().await;

        let mut crates2 = futures::stream::iter(self.crate_db.crates_to_reindex().await?.into_iter())
            .map(move |origin| async move {
                self.force_crate_reindexing(&origin);
                let _ = self.event_log.post(&SharedEvent::CrateUpdated(origin.to_str())).map_err(|e| error!("even {}", e));

                self.rich_crate_async(&origin).await.map_err(|e| {
                    error!("Can't reindex {:?}: {}", origin, e);
                    for e in e.chain() {
                        error!("• {}", e);
                    }
                }).ok()
            })
            .buffer_unordered(8)
            .filter_map(|x| async {x})
            .collect::<Vec<_>>().await;

        crates.append(&mut crates2);
        Ok(crates)
    }

    pub fn crate_exists(&self, origin: &Origin) -> bool {
        self.index.crate_exists(origin)
    }

    /// Wrapper object for metadata common for all versions of a crate
    pub async fn rich_crate_async(&self, origin: &Origin) -> CResult<RichCrate> {
        if stopped() {return Err(KitchenSinkErr::Stopped.into());}
        match origin {
            Origin::CratesIo(name) => {
                let meta = self.crates_io_meta(name).await?;
                let versions = meta.versions().map(|c| Ok(CrateVersion {
                    num: c.num,
                    updated_at: DateTime::parse_from_rfc3339(&c.updated_at)?.with_timezone(&Utc),
                    created_at: DateTime::parse_from_rfc3339(&c.created_at)?.with_timezone(&Utc),
                    yanked: c.yanked,
                })).collect::<Result<_,chrono::ParseError>>()?;
                Ok(RichCrate::new(origin.clone(), meta.krate.name, versions))
            },
            Origin::GitHub { repo, package } => {
                watch("repocrate", self.rich_crate_gh(origin, repo, package)).await
            },
            Origin::GitLab { repo, package } => {
                watch("repocrate2", self.rich_crate_gitlab(origin, repo, package)).await
            },
        }
    }

    async fn rich_crate_gh(&self, origin: &Origin, repo: &SimpleRepo, package: &str) -> CResult<RichCrate> {
        let host = RepoHost::GitHub(repo.clone()).try_into().map_err(|_| KitchenSinkErr::CrateNotFound(origin.clone())).context("ghrepo host bad")?;
        let cachebust = self.cachebust_string_for_repo(&host).await.context("ghrepo")?;
        let versions = self.get_repo_versions(origin, &host, &cachebust).await?;
        Ok(RichCrate::new(origin.clone(), format!("github/{}/{package}", repo.owner).into(), versions))
    }

    async fn rich_crate_gitlab(&self, origin: &Origin, repo: &SimpleRepo, package: &str) -> CResult<RichCrate> {
        let host = RepoHost::GitLab(repo.clone()).try_into().map_err(|_| KitchenSinkErr::CrateNotFound(origin.clone())).context("ghrepo host bad")?;
        let cachebust = self.cachebust_string_for_repo(&host).await.context("ghrepo")?;
        let versions = self.get_repo_versions(origin, &host, &cachebust).await?;
        Ok(RichCrate::new(origin.clone(), format!("gitlab/{}/{package}", repo.owner).into(), versions))
    }

    async fn get_repo_versions(&self, origin: &Origin, repo: &Repo, cachebust: &str) -> CResult<Vec<CrateVersion>> {
        let package = match origin {
            Origin::GitLab { package, .. } => package,
            Origin::GitHub { repo, package } => {
                let releases = self.gh.releases(repo, cachebust).await?.ok_or_else(|| KitchenSinkErr::CrateNotFound(origin.clone())).context("releases not found")?;
                let versions: Vec<_> = releases.into_iter().filter_map(|r| {
                    let num_full = r.tag_name?;
                    let num = num_full.trim_start_matches(|c:char| !c.is_numeric());
                    // verify that it semver-parses
                    let _ = SemVer::parse(num).map_err(|e| warn!("{:?}: ignoring {}, {}", origin, num_full, e)).ok()?;
                    let date = r.published_at.or(r.created_at)?;
                    let date = DateTime::parse_from_rfc3339(&date)
                        .map_err(|e| warn!("{:?}: ignoring {}, {}", origin, date, e)).ok()?
                        .with_timezone(&Utc);
                    Some(CrateVersion {
                        num: num.into(),
                        yanked: r.draft.unwrap_or(false),
                        updated_at: date,
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

        let versions: Vec<_> = self.crate_db.crate_versions(origin).await?.into_iter().map(|(num, timestamp)| {
            let date = Utc.timestamp(timestamp as _, 0);
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
        let _f = self.throttle.acquire().await;
        info!("Need to scan repo {:?}", repo);
        let origin = origin.clone();
        let repo = repo.clone();
        let package = package.clone();
        let checkout = self.checkout_repo(repo.clone(), false).await?;
        spawn_blocking(move || {
            let mut pkg_ver = crate_git_checkout::find_versions(&checkout)?;
            if let Some(v) = pkg_ver.remove(&*package) {
                let versions: Vec<_> = v.into_iter().map(|(num, timestamp)| {
                    let date = Utc.timestamp(timestamp, 0);
                    CrateVersion {
                        num: num.into(),
                        yanked: false,
                        updated_at: date.clone(),
                        created_at: date,
                    }
                }).collect();
                if !versions.is_empty() {
                    return Ok(versions);
                }
            }
            Err(KitchenSinkErr::CrateNotFound(origin)).context("missing releases, even tags")?
        }).await?
    }

    #[inline]
    pub async fn downloads_per_month(&self, origin: &Origin) -> KResult<Option<usize>> {
        Ok(match origin {
            Origin::CratesIo(name) => {
                let mut now = Utc::today();

                let mut curr_year = now.year() as u16;
                let mut summed_days = self.summed_year_downloads(name, curr_year)?;

                let day_of_year = now.ordinal0();
                let missing_data_days = summed_days[0..day_of_year as usize].iter().cloned().rev().take_while(|&s| s == 0).count().min(7);

                if missing_data_days > 0 {
                    now -= chrono::Duration::days(missing_data_days as _);
                }

                // TODO: make it an iterator
                let mut total = 0;
                for i in 0..30 {
                    let this_date = now - chrono::Duration::days(i);
                    let year = this_date.year() as u16;
                    if year != curr_year {
                        curr_year = year;
                        summed_days = self.summed_year_downloads(name, curr_year)?;
                    }
                    let day_of_year = this_date.ordinal0() as usize;
                    total += summed_days[day_of_year] as usize;
                }
                if total > 0 {
                    return Ok(Some(total));
                }
                // Downloads are scraped daily, so <1 day crates need a fallback
                let meta = timeout("download counts fallback", 5, self.crates_io_meta(name)).await?;
                Some(meta.krate.recent_downloads.unwrap_or(0) / 3) // 90 days
            },
            _ => None,
        })
    }

    pub async fn downloads_per_month_or_equivalent(&self, origin: &Origin) -> CResult<Option<usize>> {
        if let Some(dl) = self.downloads_per_month(origin).await? {
            return Ok(Some(dl));
        }

        // arbitrary multiplier. TODO: it's not fair for apps vs libraries
        Ok(self.github_stargazers_and_watchers(origin).await?.map(|(stars, watch)| stars.saturating_sub(1) as usize * 50 + watch.saturating_sub(1) as usize * 150))
    }

    /// Only for GitHub origins, not for crates-io crates
    pub async fn github_stargazers_and_watchers(&self, origin: &Origin) -> CResult<Option<(u32, u32)>> {
        if let Origin::GitHub { repo, .. } = origin {
            let repo = RepoHost::GitHub(repo.clone()).try_into().expect("repohost");
            if let Some(gh) = self.github_repo(&repo).await? {
                return Ok(Some((gh.stargazers_count, gh.subscribers_count)));
            }
        }
        Ok(None)
    }

    pub async fn crates_io_meta(&self, name: &str) -> KResult<CrateMetaFile> {
        tokio::task::yield_now().await;
        if stopped() {return Err(KitchenSinkErr::Stopped);}

        let krate = tokio::task::block_in_place(|| {
            self.index.crates_io_crate_by_lowercase_name(name)
        })?;
        let latest_in_index = krate.most_recent_version().version(); // most recently published version
        let meta = timeout("cacheable meta request", 10, self.crates_io.crate_meta(name, latest_in_index).map(|r| r.map_err(KitchenSinkErr::from))).await?;
        let mut meta = meta.ok_or_else(|| KitchenSinkErr::CrateNotFound(Origin::from_crates_io_name(name)))?;
        if !meta.versions.iter().any(|v| v.num == latest_in_index) {
            warn!("Crate data missing latest version {}@{}", name, latest_in_index);
            meta = watch("meta-retry", self.crates_io.crate_meta(name, &format!("{}-try-again", latest_in_index)))
                .await?
                .ok_or_else(|| KitchenSinkErr::CrateNotFound(Origin::from_crates_io_name(name)))?;
            if !meta.versions.iter().any(|v| v.num == latest_in_index) {
                error!("Error: crate data is borked {}@{}. Has only: {:?}", name, latest_in_index, meta.versions.iter().map(|v| &v.num).collect::<Vec<_>>());
            }
        }
        Ok(meta)
    }

    /// Wrapper for the latest version of a given crate.
    ///
    /// This function is quite slow, as it reads everything about the crate.
    ///
    /// There's no support for getting anything else than the latest version.
    pub fn rich_crate_version_async<'a>(&'a self, origin: &'a Origin) -> Pin<Box<dyn Future<Output = CResult<ArcRichCrateVersion>> + Send + 'a>> {
        watch("rcv-1", self.rich_crate_version_async_opt(origin, false, false).map(|res| res.map(|(k,_)| k)))
    }
    pub fn rich_crate_warnings<'a>(&'a self, origin: &'a Origin) -> Pin<Box<dyn Future<Output = CResult<HashSet<Warning>>> + Send + 'a>> {
        watch("rcv-1", self.rich_crate_version_async_opt(origin, false, true).map(|res| res.map(|(_, w)| w)))
    }

    /// Same as rich_crate_version_async, but it won't try to refresh the data. Just fails if there's no cached data.
    pub fn rich_crate_version_stale_is_ok<'a>(&'a self, origin: &'a Origin) -> Pin<Box<dyn Future<Output = CResult<ArcRichCrateVersion>> + Send + 'a>> {
        watch("stale-rich-crate", self.rich_crate_version_async_opt(origin, true, false).map(|res| res.map(|(k,_)| k)))
    }

    async fn rich_crate_version_data_derived(&self, origin: &Origin) -> KResult<Option<CachedCrate>> {
        let origin_str = origin.to_str();
        let key = (origin_str.as_str(), "");
        Ok(self.derived_storage.get_deserialized(key)?)
    }

    async fn rich_crate_version_async_opt(&self, origin: &Origin, allow_stale: bool, include_warnings: bool) -> CResult<(ArcRichCrateVersion, HashSet<Warning>)> {
        if stopped() {return Err(KitchenSinkErr::Stopped.into());}

        if !include_warnings {
            if let Some(krate) = self.loaded_rich_crate_version_cache.read().get(origin) {
                trace!("rich_crate_version_async HIT {:?}", origin);
                return Ok((krate.clone(), HashSet::new()));
            }
            trace!("rich_crate_version_async MISS {:?}", origin);
        }

        let mut maybe_data = tokio::time::timeout(Duration::from_secs(3), self.rich_crate_version_data_derived(origin))
            .await.map_err(|_| {
                warn!("db data fetch for {:?} timed out", origin);
                KitchenSinkErr::DerivedDataTimedOut
            })??;

        if let Some(cached) = &maybe_data {
            match origin {
                Origin::CratesIo(name) => {
                    if !allow_stale {
                        let expected_cache_key = self.index.cache_key_for_crate(name).context("error finding crates-io index data")?;
                        if expected_cache_key != cached.cache_key {
                            info!("Ignoring derived cache of {}, because it changed", name);
                            maybe_data = None;
                        }
                    }
                },
                _ => {}, // TODO: figure out when to invalidate cache of git-repo crates
            }
        }

        let mut data = match maybe_data {
            Some(data) => data,
            None => {
                if allow_stale {
                    self.event_log.post(&SharedEvent::CrateNeedsReindexing(origin.to_str()))?;
                    return Err(KitchenSinkErr::CacheExpired.into());
                }
                debug!("Getting/indexing {:?}", origin);
                let _th = timeout("autoindex", 29, self.auto_indexing_throttle.acquire().map(|e| e.map_err(CError::from))).await?;
                let reindex = timeout("reindex", 59, self.index_crate_highest_version(origin, false)).map_err(|e| {error!("{:?} reindex: {}", origin, e); e});
                watch("reindex", reindex).await.with_context(|| format!("reindexing {:?}", origin))?; // Pin to lower stack usage
                timeout("reindexed data", 9, self.rich_crate_version_data_derived(origin)).await?.ok_or(KitchenSinkErr::DerivedDataTimedOut)?
            },
        };

        self.refresh_crate_data(&mut data);

        let krate = Arc::new(RichCrateVersion::new(origin.clone(), data.manifest, data.derived));
        if !allow_stale {
            let mut cache = self.loaded_rich_crate_version_cache.write();
            if cache.len() > 4000 {
                cache.clear();
            }
            cache.insert(origin.clone(), krate.clone());
        }
        Ok((krate, data.warnings))
    }

    // update cached data with external information that can change without reindexing
    fn refresh_crate_data(&self, data: &mut CachedCrate) {
        let package = data.manifest.package();

        // allow overrides to take effect without reindexing
        if let Some(overrides) = self.category_overrides.get(package.name.as_str()) {
            data.derived.categories = overrides.iter().map(|c| (**c).into()).collect();
        }

        // allow forced deprecations to take effect without reindexing
        if data.manifest.badges.maintenance.status == MaintenanceStatus::None {
            if let Ok(req) = package.version().parse() {
                if is_deprecated_requirement(&package.name, &req) {
                    data.manifest.badges.maintenance.status = MaintenanceStatus::Deprecated;
                }
            }
        }
    }

    pub async fn changelog_url(&self, k: &RichCrateVersion) -> Option<String> {
        let repo = k.repository()?;
        if let RepoHost::GitHub(ref gh) = repo.host() {
            trace!("get gh changelog_url");
            let releases = self.gh.releases(gh, &self.cachebust_string_for_repo(repo).await.ok()?).await.ok()??;
            if releases.iter().any(|rel| rel.body.as_ref().map_or(false, |b| b.len() > 15)) {
                return Some(format!("https://github.com/{}/{}/releases", gh.owner, gh.repo));
            }
        }
        None
    }

    async fn crate_files_summary_from_repo(&self, origin: Origin) -> CResult<CrateFilesSummary> {
        let (repo, package): (Repo, _) = match &origin {
            Origin::GitHub { repo, package } => (RepoHost::GitHub(repo.clone()).try_into().expect("repohost"), package.clone()),
            Origin::GitLab { repo, package } => (RepoHost::GitLab(repo.clone()).try_into().expect("repohost"), package.clone()),
            _ => unreachable!(),
        };
        let checkout = self.checkout_repo(repo.clone(), true).await?;
        spawn_blocking(move || {
            let found = crate_git_checkout::path_in_repo(&checkout, &package)?
                .ok_or_else(|| {
                    let (has, err) = crate_git_checkout::find_manifests(&checkout).unwrap_or_default();
                    for e in err {
                        warn!("parse err: {}", e.0);
                    }
                    for h in has {
                        info!("has: {} -> {}", h.inner_path, h.manifest.package.as_ref().map(|p| p.name.as_str()).unwrap_or("?"));
                    }
                    KitchenSinkErr::CrateNotFoundInRepo(package.to_string(), repo.canonical_git_url().into_owned())
                })?;


            let mut meta = tarball::read_repo(&checkout, found.tree)?;
            debug_assert_eq!(meta.manifest.package, found.manifest.package);
            let package = meta.manifest.package.as_mut().ok_or(KitchenSinkErr::NotAPackage(origin))?;

            // Allowing any other URL would allow spoofing
            package.repository = Some(repo.canonical_git_url().into_owned());

            meta.path_in_repo = Some(found.inner_path);
            Ok::<_, CError>(meta)
        }).await?
    }

    async fn rich_crate_version_from_repo(&self, origin: &Origin) -> CResult<(CrateVersionSourceData, Manifest, Warnings)> {
        tokio::task::yield_now().await;
        let _f = self.throttle.acquire().await;
        if stopped() {return Err(KitchenSinkErr::Stopped.into());}

        let mut meta = self.crate_files_summary_from_repo(origin.clone()).await?;

        let package = meta.manifest.package.as_mut().ok_or_else(|| KitchenSinkErr::NotAPackage(origin.clone()))?;
        let mut warnings = HashSet::new();
        let has_readme = meta.readme.is_some();
        if !has_readme {
            let maybe_repo = package.repository.as_ref().and_then(|r| Repo::new(r).ok());
            warnings.insert(Warning::NoReadmeProperty);
            warnings.extend(Box::pin(self.add_readme_from_repo(&mut meta, maybe_repo.as_ref())).await);
        }
        self.rich_crate_version_data_common(origin.clone(), meta, false, warnings).await
    }

    pub async fn crate_files_summary_from_crates_io_tarball(&self, name: &str, ver: &str) -> Result<CrateFilesSummary, KitchenSinkErr> {
        let tarball = timeout("tarball fetch", 16, self.crates_io.crate_data(name, ver)
            .map_err(|e| KitchenSinkErr::DataNotFound(format!("{}-{}: {}", name, ver, e)))).await?;

        let meta = timeout("untar1", 40, spawn_blocking({
                let name = name.to_owned();
                let ver = ver.to_owned();
                move || tarball::read_archive(&tarball[..], &name, &ver)
            })
            .map_err(|e| KitchenSinkErr::Internal(std::sync::Arc::new(e)))).await?
            .map_err(|e| KitchenSinkErr::UnarchiverError(format!("{}-{}", name, ver), Arc::new(e)))?;
        Ok(meta)
    }

    async fn rich_crate_version_data_from_crates_io(&self, latest: &CratesIndexVersion) -> CResult<(CrateVersionSourceData, Manifest, Warnings)> {
        debug!("Building whole crate {}", latest.name());

        let _f = timeout("data-throttle", 28, self.throttle.acquire().map_err(|_| KitchenSinkErr::DataTimedOut)).await;

        let mut warnings = HashSet::new();

        let name = latest.name();
        let name_lower = name.to_ascii_lowercase();
        let ver = latest.version();
        let origin = Origin::from_crates_io_name(name);

        tokio::task::yield_now().await;
        if stopped() {return Err(KitchenSinkErr::Stopped.into());}

        let (meta, crates_io_meta) = futures::join!(
            self.crate_files_summary_from_crates_io_tarball(name, ver),
            timeout("cio meta fetch", 16, self.crates_io_meta(&name_lower)),
        );

        let mut meta = meta?;
        let crates_io_krate = crates_io_meta?.krate;
        let package = meta.manifest.package.as_mut().ok_or_else(|| KitchenSinkErr::NotAPackage(origin.clone()))?;

        // it may contain data from "nowhere"! https://github.com/rust-lang/crates.io/issues/1624
        if package.homepage.is_none() {
            if let Some(url) = crates_io_krate.homepage {
                package.homepage = Some(url.into());
            }
        }
        if package.documentation.is_none() {
            if let Some(url) = crates_io_krate.documentation {
                package.documentation = Some(url.into());
            }
        }

        // Guess repo URL if none was specified; must be done before getting stuff from the repo
        if package.repository.is_none() {
            warnings.insert(Warning::NoRepositoryProperty);
            // it may contain data from nowhere! https://github.com/rust-lang/crates.io/issues/1624
            if let Some(repo) = crates_io_krate.repository {
                package.repository = Some(repo.into());
            } else if package.homepage.as_ref().map_or(false, |h| Repo::looks_like_repo_url(h)) {
                package.repository = package.homepage.take();
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
            watch("readme", self.add_readme_from_crates_io(&mut meta, name, ver)).await;
            let has_readme = meta.readme.is_some();
            if !has_readme {
                warnings.extend(Box::pin(self.add_readme_from_repo(&mut meta, maybe_repo.as_ref())).await);
            }
        }

        if stopped() {return Err(KitchenSinkErr::Stopped.into());}

        if meta.path_in_repo.is_none() {
            if let Some(r) = maybe_repo.as_ref() {
                meta.path_in_repo = self.crate_db.path_in_repo(r, name).await?;
            }
        }

        watch("data-common", self.rich_crate_version_data_common(origin, meta, latest.is_yanked(), warnings)).await
    }

    ///// Fixing and faking the data
    async fn rich_crate_version_data_common(&self, origin: Origin, mut meta: CrateFilesSummary, is_yanked: bool, mut warnings: Warnings) -> CResult<(CrateVersionSourceData, Manifest, Warnings)> {
        if stopped() {return Err(KitchenSinkErr::Stopped.into());}

        Self::override_bad_categories(&mut meta.manifest);

        let mut github_keywords = None;

        let package = meta.manifest.package.as_mut().ok_or_else(|| KitchenSinkErr::NotAPackage(origin.clone()))?;
        let maybe_repo = package.repository.as_ref().and_then(|r| Repo::new(r).ok());
        // Guess keywords if none were specified
        // TODO: also ignore useless keywords that are unique db-wide
        let gh = match maybe_repo.as_ref() {
            Some(repo) => if let RepoHost::GitHub(ref gh) = repo.host() {
                trace!("get gh topics");
                self.gh.topics(gh, &self.cachebust_string_for_repo(repo).await.context("fetch topics")?).await?
            } else {None},
            _ => None,
        };
        if let Some(mut topics) = gh {
            for t in &mut topics {
                if t.starts_with("rust-") {
                    *t = t.trim_start_matches("rust-").into();
                }
                if t.ends_with("-rs") {
                    *t = t.trim_end_matches("-rs").into();
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

        let explicit_documentation_link_existed = package.documentation.is_some();

        if origin.is_crates_io() {
            // Delete the original docs.rs link, because we have our own
            // TODO: what if the link was to another crate or a subpage?
            if package.documentation.as_ref().map_or(false, |s| Self::is_docs_rs_link(s)) && self.has_docs_rs(&origin, &package.name, package.version()).await {
                package.documentation = None; // docs.rs is not proper docs
            }
        }

        warnings.extend(self.remove_redundant_links(package, maybe_repo.as_ref()).await);

        let mut github_description = None;
        let mut github_name = None;
        if let Some(ref crate_repo) = maybe_repo {
            if let Some(ghrepo) = self.github_repo(crate_repo).await? {
                if ghrepo.archived && meta.manifest.badges.maintenance.status == MaintenanceStatus::None {
                    meta.manifest.badges.maintenance.status = MaintenanceStatus::AsIs; // FIXME: not exactly
                }
                if package.homepage.is_none() {
                    if let Some(url) = ghrepo.homepage {
                        let also_add_docs = package.documentation.is_none() && ghrepo.github_page_url.as_ref().map_or(false, |p| p != &url);
                        package.homepage = Some(url);
                        // github pages URLs are often bad, so don't even try to use them unless documentation property is missing
                        // (especially don't try to replace docs.rs with this gamble)
                        if also_add_docs && !explicit_documentation_link_existed {
                            if let Some(url) = ghrepo.github_page_url {
                                package.documentation = Some(url);
                            }
                        }
                    } else if let Some(url) = ghrepo.github_page_url {
                        package.homepage = Some(url);
                    }
                    warnings.extend(self.remove_redundant_links(package, maybe_repo.as_ref()).await);
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
        if !self.is_readme_short(meta.readme.as_ref().map(|r| &r.1)) {
            meta.lib_file = None;
        }

        let capitalized_name = if package.name != package.name.to_ascii_lowercase() {
            // if the crate name isn't all-lowercase, then keep it as-is
            package.name.clone()
        } else {
            // Process crate's text to guess non-lowercased name
            let mut words = vec![package.name.as_str()];
            let readme_txt;
            if let Some(ref r) = meta.readme {
                readme_txt = render_readme::Renderer::new(None).visible_text(&r.1);
                words.push(&readme_txt);
            }
            if let Some(ref lib) = meta.lib_file {
                words.push(lib);
            }
            if let Some(ref s) = package.description {words.push(s);}
            if let Some(ref s) = github_description {words.push(s);}
            if let Some(ref s) = github_name {words.push(s);}
            if let Some(ref s) = package.homepage {words.push(s);}
            if let Some(ref s) = package.documentation {words.push(s);}
            if let Some(ref s) = package.repository {words.push(s);}

            Self::capitalized_name(&package.name, words.into_iter())
        };

        let has_buildrs = meta.has("build.rs");
        let has_code_of_conduct = meta.has("CODE_OF_CONDUCT.md") || meta.has("docs/CODE_OF_CONDUCT.md") || meta.has(".github/CODE_OF_CONDUCT.md");

        let path_in_repo = meta.path_in_repo;
        let treeish_revision = meta.vcs_info_git_sha1.as_ref().map(hex::encode);
        let readme = meta.readme.map(|(readme_path, markup)| {
            let (base_url, base_image_url) = match maybe_repo {
                Some(repo) => {
                    // Not parsing github URL, because "aboslute" path should not be allowed to escape the repo path,
                    // but it needs to normalize ../readme paths
                    let url = url::Url::parse(&format!("http://localhost/{}", path_in_repo.as_deref().unwrap_or_default())).and_then(|u| u.join(&readme_path));
                    let in_repo_url_path = url.as_ref().map_or("", |u| u.path().trim_start_matches('/'));
                    (Some(repo.readme_base_url(in_repo_url_path, treeish_revision.as_deref())), Some(repo.readme_base_image_url(in_repo_url_path, treeish_revision.as_deref())))
                },
                None => (None, None),
            };
            Readme {
                markup,
                base_url,
                base_image_url,
            }
        });

        let crate_compressed_size = meta.compressed_size.min(u32::MAX as _) as u32;
        let src = CrateVersionSourceData {
            capitalized_name,
            language_stats: meta.language_stats,
            crate_compressed_size,
            // sometimes uncompressed sources without junk are smaller than tarball with junk
            crate_decompressed_size: (meta.decompressed_size as u32).max(crate_compressed_size),
            is_nightly: meta.is_nightly,
            has_buildrs,
            has_code_of_conduct,
            readme,
            lib_file: meta.lib_file,
            bin_file: meta.bin_file,
            github_description,
            github_keywords,
            path_in_repo,
            is_yanked,
            vcs_info_git_sha1: meta.vcs_info_git_sha1,
        };

        Ok((src, meta.manifest, warnings))
    }

    fn canonical_http_of_crate_at_version_cache_key(origin: &Origin, crate_version: &str) -> String {
        format!("{}-{crate_version}", origin.short_crate_name())
    }

    pub fn canonical_http_of_crate_at_version_cached(&self, origin: &Origin, crate_version: &str) -> Option<String> {
        self.canonical_http_of_crate_at_version_cache.get(Self::canonical_http_of_crate_at_version_cache_key(origin, crate_version).as_str()).ok().flatten()
    }

    pub async fn canonical_http_of_crate_at_version(&self, origin: &Origin, crate_version: &str) -> CResult<String> {
        if let Some(s) = self.canonical_http_of_crate_at_version_cached(origin, crate_version) {
            return Ok(s);
        }

        let ver = self.crate_files_summary_from_crates_io_tarball(origin.short_crate_name(), crate_version).await?;
        if let Some(sha) = ver.vcs_info_git_sha1 {
            let package = ver.manifest.package.as_ref().ok_or_else(|| KitchenSinkErr::NotAPackage(origin.clone()))?;
            if let Some(Ok(repo)) = package.repository.as_deref().map(Repo::new) {
                let path_in_repo = match ver.path_in_repo {
                    Some(p) => p,
                    None => self.crate_db.path_in_repo(&repo, &package.name).await?.unwrap_or_default(),
                };
                let url = repo.canonical_http_url(&path_in_repo, Some(&hex::encode(sha))).into_owned();
                self.canonical_http_of_crate_at_version_cache.set(Self::canonical_http_of_crate_at_version_cache_key(origin, crate_version), &url)?;
                return Ok(url);
            }
        }
        let mut url = format!("https://docs.rs/crate/{crate_name}/{version}/source/", crate_name = urlencoding::Encoded(origin.short_crate_name()), version = urlencoding::Encoded(crate_version));
        if let Some(sha) = ver.vcs_info_git_sha1 {
            use std::fmt::Write;
            let _ = write!(&mut url, "#{}", hex::encode(sha));
        }
        self.canonical_http_of_crate_at_version_cache.set(Self::canonical_http_of_crate_at_version_cache_key(origin, crate_version), &url)?;
        Ok(url)
    }

    fn override_bad_categories(manifest: &mut Manifest) {
        let direct_dependencies = &manifest.dependencies;
        let has_cargo_bin = manifest.has_cargo_bin();
        let package = manifest.package.as_mut().expect("pkg");
        let eq = |a: &str, b: &str| -> bool { a.eq_ignore_ascii_case(b) };

        for cat in &mut package.categories {
            if cat.as_bytes().iter().any(|c| c.is_ascii_uppercase()) {
                *cat = cat.to_lowercase();
            }
            if has_cargo_bin && (cat == "development-tools" || cat == "command-line-utilities") && package.keywords.iter().any(|k| k.eq_ignore_ascii_case("cargo-subcommand") || k.eq_ignore_ascii_case("subcommand")) {
                *cat = "development-tools::cargo-plugins".into();
            }
            if cat == "localization" {
                // nobody knows the difference
                *cat = "internationalization".to_string();
            }
            if cat == "parsers" && (direct_dependencies.keys().any(|k| k == "nom" || k == "peresil" || k == "combine") ||
                    package.keywords.iter().any(|k| match k.to_ascii_lowercase().as_ref() {
                        "asn1" | "tls" | "idl" | "crawler" | "xml" | "nom" | "json" | "logs" | "elf" | "uri" | "html" | "protocol" | "semver" | "ecma" |
                        "chess" | "vcard" | "exe" | "fasta" => true,
                        _ => false,
                    })) {
                *cat = "parser-implementations".into();
            }
            if (cat == "cryptography" || cat == "database" || cat == "rust-patterns" || cat == "development-tools") && package.keywords.iter().any(|k| eq(k, "bitcoin") || eq(k, "ethereum") || eq(k, "exonum") || eq(k, "blockchain")) {
                *cat = "cryptography::cryptocurrencies".into();
            }
            // crates-io added a better category
            if cat == "game-engines" {
                *cat = "game-development".to_string();
            }
            if cat == "games" && package.keywords.iter().any(|k| {
                    k == "game-dev" || k == "game-development" || eq(k,"gamedev") || eq(k,"framework") || eq(k,"utilities") || eq(k,"parser") || eq(k,"api")
                }) {
                *cat = "game-development".into();
            }

            if cat == "data-structure" {
                *cat = "data-structures".to_string();
            }
            if cat == "serialization" {
                *cat = "encoding".to_string();
            }
            if cat == "async" {
                *cat = "asynchronous".to_string();
            }
            if cat == "blockchain" {
                *cat = "cryptography::cryptocurrencies".to_string();
            }
            // useless category
            if cat == "multimedia::encoding" {
                *cat = "multimedia".to_string();
            }
            if cat == "aerospace::simulation" {
                *cat = "simulation".to_string();
            }
            if cat == "aerospace::drones" {
                *cat = "science::robotics".to_string();
            }
            if cat == "aerospace::unmanned-aerial-vehicles" {
                *cat = "science::robotics".to_string();
            }
            if cat == "aerospace::space-protocols" {
                *cat = "science".to_string();
            }
            if cat == "os::linix-apis" {
                *cat = "os::unix-apis".to_string();
            }
            if cat == "os::freebsd-apis" {
                *cat = "os::unix-apis".to_string();
            }

            // got out of sync with crates-io
            if cat == "mathematics" {
                *cat = "science::math".to_string();
            }
            if cat == "science" || cat == "algorithms" {
                let is_nn = |k: &String| k == "neural-network" || eq(k,"machine-learning") || eq(k,"neuralnetworks") || eq(k,"neuralnetwork") || eq(k,"tensorflow") || eq(k,"deep-learning");
                let is_math = |k: &String| {
                    k == "math" || eq(k,"calculus") || eq(k,"algebra") || eq(k,"linear-algebra") || eq(k,"mathematics") || eq(k,"maths") || eq(k,"number-theory")
                };
                if package.keywords.iter().any(is_nn) {
                    *cat = "science::ml".into();
                } else if package.keywords.iter().any(is_math) {
                    *cat = "science::math".into();
                }
            }
        }
    }

    pub async fn github_repo(&self, crate_repo: &Repo) -> CResult<Option<GitHubRepo>> {
        Ok(match crate_repo.host() {
            RepoHost::GitHub(ref repo) => {
                let cachebust = self.cachebust_string_for_repo(crate_repo).await.context("ghrepo")?;
                self.gh.repo(repo, &cachebust).await?
            },
            _ => None,
        })
    }

    pub fn is_readme_short(&self, readme: Option<&Markup>) -> bool {
        if let Some(r) = readme {
            match r {
                Markup::Markdown(ref s) | Markup::Rst(ref s) | Markup::Html(ref s) => s.len() < 1000,
            }
        } else {
            true
        }
    }

    pub async fn is_build_or_dev(&self, k: &Origin) -> Result<(bool, bool), KitchenSinkErr> {
        Ok(self.crates_io_dependents_stats_of(k).await?
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

    async fn add_readme_from_repo(&self, meta: &mut CrateFilesSummary, maybe_repo: Option<&Repo>) -> Warnings {
        let mut warnings = HashSet::new();
        let package = match meta.manifest.package.as_ref() {
            Some(p) => p,
            None => {
                warnings.insert(Warning::NotAPackage);
                return warnings;
            },
        };
        if let Some(repo) = maybe_repo {
            let res = self.checkout_repo(repo.clone(), true).await
                .map(|checkout| crate_git_checkout::find_readme(&checkout, package));
            match res {
                Ok(Ok(Some(mut readme))) => {
                    // Make the path absolute, because the readme is now absolute relative to repo root,
                    // rather than relative to crate's dir within the repo
                    if !readme.0.starts_with('/') {
                        readme.0.insert(0, '/');
                    }
                    meta.readme = Some(readme);
                },
                Ok(Ok(None)) => {
                    warnings.insert(Warning::NoReadmeInRepo(repo.canonical_git_url().into()));
                },
                Ok(Err(err)) => {
                    warnings.insert(Warning::ErrorCloning(repo.canonical_git_url().into()));
                    error!("Search of {} ({}) failed: {}", package.name, repo.canonical_git_url(), err);
                },
                Err(err) => {
                    warnings.insert(Warning::ErrorCloning(repo.canonical_git_url().into()));
                    error!("Checkout of {} ({}) failed: {}", package.name, repo.canonical_git_url(), err);
                },
            }
        }
        warnings
    }

    async fn add_readme_from_crates_io(&self, meta: &mut CrateFilesSummary, name: &str, ver: &str) {
        let key = format!("{}/{}", name, ver);
        if let Ok(Some(_)) = self.readme_check_cache.get(key.as_str()) {
            return;
        }

        if let Ok(Some(html)) = self.crates_io.readme(name, ver).await {
            debug!("Found readme on crates.io {}@{}", name, ver);
            meta.readme = Some((String::new(), Markup::Html(String::from_utf8_lossy(&html).to_string())));
        } else {
            let _ = self.readme_check_cache.set(key, ());
            debug!("No readme on crates.io for {}@{}", name, ver);
        }
    }

    async fn remove_redundant_links(&self, package: &mut Package, maybe_repo: Option<&Repo>) -> Warnings {
        let mut warnings = HashSet::new();

        // We show github link prominently, so if homepage = github, that's nothing new
        let homepage_is_repo = Self::is_same_url(package.homepage.as_deref(), package.repository.as_deref());
        let homepage_is_canonical_repo = maybe_repo
            .and_then(|repo| {
                package.homepage.as_ref()
                .and_then(|home| Repo::new(home).ok())
                .map(|other| {
                    repo.canonical_git_url() == other.canonical_git_url()
                })
            })
            .unwrap_or(false);

        if homepage_is_repo || homepage_is_canonical_repo {
            package.homepage = None;
        }

        if Self::is_same_url(package.documentation.as_deref(), package.homepage.as_deref()) ||
           Self::is_same_url(package.documentation.as_deref(), package.repository.as_deref()) ||
           maybe_repo.map_or(false, |repo| Self::is_same_url(Some(&*repo.canonical_http_url("", None)), package.documentation.as_deref())) {
            package.documentation = None;
        }

        if package.homepage.as_ref().map_or(false, |d| Self::is_docs_rs_link(d) || d.starts_with("https://lib.rs/") || d.starts_with("https://crates.io/")) {
            package.homepage = None;
        }

        if let Some(url) = package.homepage.as_deref() {
            if !self.check_url_is_valid(url).await {
                warnings.insert(Warning::BrokenLink("homepage".into(), url.into()));
                package.homepage = None;
            }
        }

        if let Some(url) = package.documentation.as_deref() {
            if !self.check_url_is_valid(url).await {
                warnings.insert(Warning::BrokenLink("documentation".into(), url.into()));
                package.documentation = None;
            }
        }
        warnings
    }

    async fn check_url_is_valid(&self, url: &str) -> bool {
        let retries = 1 + if let Ok(Some((res, retries_so_far))) = self.url_check_cache.get(url) {
            if res || retries_so_far > 3 {
                return res;
            }
            retries_so_far
        } else {
            0
        };
        let res = timeout("urlchk", 10, async {
            let req = reqwest::Client::builder().build().unwrap();
            let res = match req.get(url).send().await {
                Ok(res) => res.status().is_success(),
                Err(e) => {
                    warn!("URL CHK: {} = {}", url, e);
                    false
                },
            };
            Ok::<_, KitchenSinkErr>(res)
        }).await.unwrap_or(false);
        let _ = self.url_check_cache.set(url, (res, retries)).map_err(|e| error!("url cache: {}", e));
        res
    }

    fn is_docs_rs_link(d: &str) -> bool {
        let d = d.trim_start_matches("http://").trim_start_matches("https://");
        d.starts_with("docs.rs/") || d.starts_with("crates.fyi/")
    }

    /// name is case-sensitive!
    pub async fn has_docs_rs(&self, origin: &Origin, name: &str, ver: &str) -> bool {
        if !origin.is_crates_io() {
            return false;
        }
        watch("builds", self.docs_rs.builds(name, ver)).await.unwrap_or(true) // fail open
    }

    fn is_same_url(a: Option<&str>, b: Option<&str>) -> bool {
        fn trim(s: &str) -> &str {
            let s = s.trim_start_matches("http://").trim_start_matches("https://");
            s.split('#').next().unwrap().trim_end_matches("/index.html").trim_end_matches('/')
        }

        match (a, b) {
            (Some(a), Some(b)) if trim(a).eq_ignore_ascii_case(trim(b)) => true,
            _ => false,
        }
    }

    pub fn all_dependencies_flattened(&self, krate: &RichCrateVersion) -> Result<DepInfMap, KitchenSinkErr> {
        match krate.origin() {
            Origin::CratesIo(name) => {
                self.index.all_dependencies_flattened(self.index.crates_io_crate_by_lowercase_name(name).map_err(KitchenSinkErr::Deps)?).map_err(KitchenSinkErr::Deps)
            },
            _ => self.index.all_dependencies_flattened(krate).map_err(KitchenSinkErr::Deps),
        }
    }

    pub fn all_crates_io_versions(&self, origin: &Origin) -> Result<Vec<CratesIndexVersion>, KitchenSinkErr> {
        match origin {
            Origin::CratesIo(name) => {
                Ok(self.index.crates_io_crate_by_lowercase_name(name).map_err(KitchenSinkErr::Deps)?.versions().to_vec())
            },
            _ => Err(KitchenSinkErr::NoVersions),
        }
    }

    #[inline]
    fn iter_crates_io_version_matching_requirement_by_lowercase_name(&self, crate_name: &str, req: &str) -> Result<impl Iterator<Item=(SemVer, &CratesIndexVersion)> + '_, KitchenSinkErr> {
        assert!(crate_name.as_bytes().iter().all(|c| !c.is_ascii_uppercase()));

        let req = VersionReq::parse(req).map_err(|_| KitchenSinkErr::NoVersions)?;

        Ok(self.index.crates_io_crate_by_lowercase_name(crate_name).map_err(KitchenSinkErr::Deps)?
            .versions()
            .iter()
            .filter_map(move |v| {
                let semver = v.version().parse().ok()?;
                if req.matches(&semver) {
                    Some((semver, v))
                } else {None}
            }))
    }

    pub fn newest_crates_io_version_matching_requirement_by_lowercase_name(&self, crate_name: &str, req: &str) -> Result<(SemVer, CratesIndexVersion), KitchenSinkErr> {
        self.iter_crates_io_version_matching_requirement_by_lowercase_name(crate_name, req)?
            .max_by(|(a, _), (b, _)| a.cmp(b))
            .map(|(s, v)| (s, v.clone()))
            .ok_or(KitchenSinkErr::NoVersions)
    }

    pub fn lowest_crates_io_version_matching_requirement_by_lowercase_name(&self, crate_name: &str, req: &str) -> Result<(SemVer, CratesIndexVersion), KitchenSinkErr> {
        self.iter_crates_io_version_matching_requirement_by_lowercase_name(crate_name, req)?
            .min_by(|(a, _), (b, _)| a.cmp(b))
            .map(|(s, v)| (s, v.clone()))
            .ok_or(KitchenSinkErr::NoVersions)
    }

    #[inline]
    pub async fn prewarm(&self) {
        let idx = Arc::clone(&self.index);
        let _ = tokio::task::spawn(async move { let _ = idx.deps_stats().await; }).await;
    }

    pub fn reload_indexed_crate(&self, origin: &Origin) {
        self.loaded_rich_crate_version_cache.write().remove(origin);
        self.crate_rustc_compat_cache.write().remove(origin);
    }

    pub fn force_crate_reindexing(&self, origin: &Origin) {
        if let Origin::CratesIo(crate_name) = origin {
            let _ = self.crates_io_owners_cache.delete(crate_name);
        }
        let origin_str = origin.to_str();
        let _ = self.derived_storage.delete((origin_str.as_str(), ""));
        self.loaded_rich_crate_version_cache.write().remove(origin);
    }

    pub async fn update(&self) {
        let crev = self.crev.clone();
        let rustsec = self.rustsec.clone();
        rayon::spawn(move || {
            let _ = rustsec.lock().unwrap().update().map_err(|e| error!("crev update: {e}"));
            let _ = crev.update().map_err(|e| error!("crev update: {e}"));
        });
        self.index.update().await;
    }

    pub async fn crates_io_all_rev_deps_counts(&self) -> Result<StatsHistogram, KitchenSinkErr> {
        let stats = self.index.deps_stats().await.map_err(KitchenSinkErr::Deps)?;
        let mut tmp = HashMap::new();
        for (o, r) in stats.counts.iter() {
            let cnt = r.runtime.all() + r.build.all() + r.dev as u32;
            let t = tmp.entry(cnt).or_insert((0, Vec::new()));
            t.0 += 1;
            if t.1.len() < 50 {
                t.1.push((r.direct.all(), o.to_string()));
            }
        }

        Ok(tmp.into_iter().map(|(k, (cnt, mut examples))| {
            examples.sort_unstable_by_key(|(dcnt,_)| !dcnt);
            (k, (cnt, examples.into_iter().take(8).map(|(_, n)| n).collect()))
        }).collect())
    }

    #[inline]
    pub async fn crates_io_dependents_stats_of(&self, origin: &Origin) -> Result<Option<&RevDependencies>, KitchenSinkErr> {
        match origin {
            Origin::CratesIo(crate_name) => Ok(self.index.deps_stats().await.map_err(KitchenSinkErr::Deps)?.counts.get(&**crate_name)),
            _ => Ok(None),
        }
    }

    /// Crev reviews
    pub fn reviews_for_crate(&self, origin: &Origin) -> Vec<creviews::Review> {
        match origin {
            Origin::CratesIo(name) => self.crev.reviews_for_crate(name).unwrap_or_default(),
            _ => vec![],
        }
    }

    /// Rustsec reviews
    pub fn advisories_for_crate(&self, origin: &Origin) -> Vec<creviews::security::Advisory> {
        match origin {
            Origin::CratesIo(name) => self.rustsec.lock().unwrap().advisories_for_crate(name).into_iter().cloned().collect(),
            _ => vec![],
        }
    }

    /// (latest, pop)
    /// 0 = not used
    /// 1 = everyone uses it
    #[inline]
    pub async fn version_popularity(&self, crate_name: &str, requirement: &VersionReq) -> CResult<Option<VersionPopularity>> {
        let mut lost_popularity = false;
        let (matches_latest, mut pop) = match self.index.version_popularity(crate_name, requirement).await.map_err(KitchenSinkErr::Deps)? {
            Some(res) => res,
            None => return Ok(None),
        };

        if let Some((former_glory, _)) = self.former_glory(&Origin::from_crates_io_name(crate_name)).await? {
            if former_glory < 0.5 {
                lost_popularity = true;
            }
            pop *= former_glory as f32;
        }
        Ok(Some(VersionPopularity {
            lost_popularity, pop, matches_latest,
        }))
    }

    #[inline]
    pub async fn crate_all_owners(&self) -> CResult<Vec<crate_db::CrateOwnerStat>> {
        Ok(self.crate_db.crate_all_owners().await?)
    }

    /// "See also"
    #[inline]
    pub async fn related_categories(&self, slug: &str) -> CResult<Vec<String>> {
        Ok(self.crate_db.related_categories(slug).await?)
    }

    /// Recommendations for similar and related crates
    pub async fn related_crates(&self, krate: &RichCrateVersion, min_recent_downloads: u32) -> CResult<(Vec<Origin>, Vec<ArcRichCrateVersion>)> {
        let (mut same_namespace, mut replacements, mut see_also) = futures::try_join!(
            self.related_namespace_crates(krate),
            self.crate_db.replacement_crates(krate.short_name()).map_err(From::from),
            self.crate_db.related_crates(krate.origin(), min_recent_downloads).map_err(From::from),
        )?;

        // Dedupe all
        let mut dupe_origins: HashSet<_> = same_namespace.iter().map(|k| k.origin()).collect();
        replacements.retain(|r| dupe_origins.get(r).is_none());
        dupe_origins.extend(replacements.iter());
        see_also.retain(|r| dupe_origins.get(&r.0).is_none());

        let replacements = join_all(replacements.into_iter().map(|origin| async move {
            let rank = self.crate_db.crate_rank(&origin).await.unwrap_or(0.);
            (origin, rank as f32)
        })).await;

        see_also.extend(replacements);
        see_also.sort_unstable_by(|a,b| b.1.total_cmp(&a.1));
        see_also.truncate(15);

        let best_ranking = see_also.iter().map(|&(_,ranking)| ranking).max_by(|a,b| a.total_cmp(b)).unwrap_or(0.);
        let cut_off = (0.3f32).min(best_ranking / 2.);
        see_also.retain(|&(_, ranking)| ranking >= cut_off);

        // there may still be related among similar, and may not even share name prefix (e.g. rand + getrandom)
        let see_also_related = self.filter_namespace_related_origins(krate, see_also.clone()).await;
        for r in see_also_related {
            if let Some(pos) = see_also.iter().position(|(o, _)| o == r.origin()) {
                see_also.remove(pos);
                same_namespace.insert(0, r);
            }
        }

        let see_also = see_also.into_iter().map(|(o, _)| o).take(10).collect();

        Ok((see_also, same_namespace))
    }

    async fn related_namespace_crates(&self, krate: &RichCrateVersion) -> CResult<Vec<ArcRichCrateVersion>> {
        let repo_crates = if let Some(repo) = krate.repository() {
            self.crate_db.crates_in_repo(repo).await?
        } else {
            vec![]
        };

        let dedup: HashSet<_> = repo_crates.iter().chain(Some(krate.origin())).collect();
        let valid_ns_prefixes: HashSet<_> = dedup.iter()
            .map(|o| crate_name_namespace_prefix(o.short_crate_name()))
            .collect();

        let mut candidates = Vec::with_capacity(32);
        let mut dedup2 = HashSet::new();

        let owners = self.crate_owners(krate.origin(), CrateOwners::Strict).await?;
        for github_id in owners.iter().filter_map(|o| o.github_id).take(20) {
            let crates = self.crate_db.crates_of_author(github_id).await?;
            candidates.extend(crates.into_iter().take(1000)
                .filter(|c| !dedup.contains(&c.origin))
                .filter(|c| {
                    let name = c.origin.short_crate_name();
                    let suffix = name.rsplit(|c:char| c == '_' || c == '-').next().unwrap();

                    suffix != "internal" && suffix != "internals" &&
                        valid_ns_prefixes.get(crate_name_namespace_prefix(name)).is_some()
                })
                .filter(|c| dedup2.insert(c.origin.clone()))
                .map(|c| {
                    // move shorter (root) crate names to the beginning
                    let hyphens = (1 + c.origin.short_crate_name().bytes().filter(|&b| b == b'_' || b == b'-').count()) as f32;
                    (c.origin, c.crate_ranking / hyphens, c.crate_ranking)
                })
                .take(30));
        }
        // Kill crates like serde_macros that is dead
        let best_ranking = candidates.iter().map(|&(_,_,ranking)| ranking).max_by(|a,b| a.total_cmp(b)).unwrap_or(0.);
        let cut_off = (0.2f32).min(best_ranking / 3.);
        candidates.retain(|&(_, _, ranking)| ranking >= cut_off);

        candidates.sort_by(|a, b| b.1.total_cmp(&a.1)); // needs stable sort to preserve original ranking
        candidates.truncate(20);

        let origins: Vec<_> = repo_crates.into_iter().map(|o| (o,1.))
            .chain(candidates.into_iter().map(|(a,_,c)| (a,c))).collect();
        Ok(self.filter_namespace_related_origins(krate, origins).await)
    }

    async fn filter_namespace_related_origins<'a>(&self, krate: &RichCrateVersion, origins: Vec<(Origin, f32)>) -> Vec<ArcRichCrateVersion> {
        let deadline_at = Instant::now() + Duration::from_secs(3);
        let iter = origins.into_iter()
        .filter(|(o, _)| o != krate.origin())
        .map(|(origin, ranking)| async move {
            let other = deadline("rel-ns", deadline_at, self.rich_crate_version_stale_is_ok(&origin)).await.ok()?;
            if other.is_yanked() {
                return None;
            }
            self.are_namespace_related(krate, &other).await.ok()?.map(|related| (other, ranking * related))
        });
        let mut tmp = futures::stream::iter(iter)
            .buffered(5)
            .filter_map(|res| async move { res })
            .take(15)
            .collect::<Vec<_>>()
            .await;
        tmp.sort_unstable_by(|a,b| b.1.total_cmp(&a.1));
        tmp.into_iter().map(|(k,_)| k).collect()
    }

    /// Returns (nth, slug)
    pub async fn top_category(&self, krate: &RichCrateVersion) -> Option<(u32, Box<str>)> {
        let crate_origin = krate.origin();
        let cats = join_all(krate.category_slugs().iter().cloned().map(|slug| async move {
            let c = timeout("top category", 6, self.top_crates_in_category(&slug)).await?;
            Ok::<_, CError>((c, slug))
        })).await;
        cats.into_iter().filter_map(|cats| cats.ok()).filter_map(|(cat, slug)| {
            cat.iter().position(|o| o == crate_origin).map(|pos| {
                (pos as u32 +1, slug)
            })
        })
        .min_by_key(|a| a.0)
    }

    /// Returns (nth, keyword)
    #[inline]
    pub async fn top_keyword(&self, krate: &RichCrate) -> CResult<Option<(u32, String)>> {
        Ok(self.crate_db.top_keyword(krate.origin()).await?)
    }

    /// Maintenance: add user to local db index
    pub(crate) async fn index_user_m(&self, user: &MinimalUser, commit: &GitCommitAuthor) -> CResult<()> {
        if stopped() {return Err(KitchenSinkErr::Stopped.into());}
        let user = self.gh.user_by_login(&user.login).await?.ok_or_else(|| KitchenSinkErr::AuthorNotFound(user.login.clone()))?;
        if !self.user_db.email_has_github(&commit.email)? {
            println!("{} => {}", commit.email, user.login);
            self.user_db.index_user(&user, Some(&commit.email), commit.name.as_deref())?;
        }
        Ok(())
    }

    /// Maintenance: add user to local db index
    pub fn index_user(&self, user: &User, commit: &GitCommitAuthor) -> CResult<()> {
        if stopped() {return Err(KitchenSinkErr::Stopped.into());}
        if !self.user_db.email_has_github(&commit.email)? {
            println!("{} => {}", commit.email, user.login);
            self.user_db.index_user(user, Some(&commit.email), commit.name.as_deref())?;
        }
        Ok(())
    }

    /// Maintenance: add user to local db index
    pub async fn index_email(&self, email: &str, name: Option<&str>) -> CResult<()> {
        if stopped() {return Err(KitchenSinkErr::Stopped.into());}
        if !self.user_db.email_has_github(email)? {
            match self.gh.user_by_email(email).await {
                Ok(Some(users)) => {
                    for user in users {
                        println!("{} == {} ({:?})", user.login, email, name);
                        self.user_db.index_user(&user, Some(email), name)?;
                    }
                },
                Ok(None) => println!("{} not found on github", email),
                Err(e) => error!("•••• {}", e),
            }
        }
        Ok(())
    }

    /// Maintenance: add crate to local db index
    pub async fn index_crate(&self, k: &RichCrate, score: f64) -> CResult<()> {
        if stopped() {return Err(KitchenSinkErr::Stopped.into());}
        self.crate_db.index_versions(k, score, self.downloads_per_month(k.origin()).await?).await?;
        Ok(())
    }

    pub fn index_crate_downloads(&self, crates_io_name: &str, by_ver: &HashMap<&str, &[(Date<Utc>, u32, bool)]>) -> CResult<()> {
        if stopped() {return Err(KitchenSinkErr::Stopped.into());}
        let mut year_data = HashMap::new();
        for (version, date_dls) in by_ver {
            let version = MiniVer::from(match semver::Version::parse(version) {
                Ok(v) => v,
                Err(e) => {
                    warn!("Bad version: {} {} {}", crates_io_name, version, e);
                    continue;
                }
            });
            for (day, dls, overwrite) in date_dls.iter() {
                let curr_year = day.year() as u16;
                let mut curr_year_data = match year_data.entry(curr_year) {
                    Vacant(e) => {
                        e.insert((false, self.yearly.get_crate_year(crates_io_name, curr_year)?.unwrap_or_default()))
                    },
                    Occupied(e) => e.into_mut(),
                };

                let day_of_year = day.ordinal0() as usize;
                let year_dls = curr_year_data.1.entry(version.clone()).or_insert_with(Default::default);
                if year_dls.0[day_of_year] < *dls || *overwrite {
                    curr_year_data.0 = true;
                    year_dls.0[day_of_year] = *dls;
                }
            }
        }
        for (curr_year, (modified, curr_year_data)) in year_data {
            if modified {
                self.yearly.set_crate_year(crates_io_name, curr_year, &curr_year_data)?;
            }
        }
        Ok(())
    }

    pub async fn index_crate_highest_version(&self, origin: &Origin, maintenance_only_reindexing: bool) -> CResult<()> {
        tokio::task::yield_now().await;
        if stopped() {return Err(KitchenSinkErr::Stopped.into());}
        info!("Indexing {:?}", origin);

        if !self.crate_exists(origin) {
            let _ = self.crate_db.delete_crate(origin).await;
        }

        timeout("before-index", 5, self.crate_db.before_index_latest(origin).map_err(anyhow::Error::from)).await?;

        let (source_data, manifest, mut warnings, cache_key) = self.fetch_rich_crate_version_data(origin).await?;

        let _ = tokio::task::block_in_place(|| self.index_msrv_from_manifest(origin, &manifest)).map_err(|e| error!("msrv {}", e));

        // direct deps are used as extra keywords for similarity matching,
        // but we're taking only niche deps to group similar niche crates together
        let raw_deps_stats = timeout("raw-r-deps", 25, self.index.deps_stats().map_err(KitchenSinkErr::Deps)).await?;
        let mut weighed_deps = Vec::<(&str, f32)>::new();
        let all_deps = manifest.direct_dependencies();
        let all_deps = [(all_deps.0, 1.0), (all_deps.2, 0.33)];
        // runtime and (lesser) build-time deps
        for (deps, overall_weight) in all_deps.iter() {
            for dep in deps {
                if let Some(rev) = raw_deps_stats.counts.get(&*dep.package) {
                    let right_popularity = rev.direct.all() > 2 && rev.direct.all() < 200 && rev.runtime.def < 500 && rev.runtime.opt < 800;
                    if Self::dep_interesting_for_index(&dep.package).unwrap_or(right_popularity) {
                        let weight = overall_weight / (1 + rev.direct.all()) as f32;
                        weighed_deps.push((&dep.package, weight));
                    }
                }
            }
        }
        let (is_build, is_dev) = self.is_build_or_dev(origin).await?;
        let package = manifest.package();
        let readme_text = source_data.readme.as_ref().map(|r| render_readme::Renderer::new(None).visible_text(&r.markup));
        let repository = package.repository.as_ref().and_then(|r| Repo::new(r).ok());
        let authors = package.authors.iter().map(|a| Author::new(a)).collect::<Vec<_>>();

        let mut bad_categories = Vec::new();
        let mut category_slugs = categories::Categories::fixed_category_slugs(&package.categories, &mut bad_categories);
        for c in &bad_categories {
            // categories invalid for lib.rs may still be valid for crates.io (they've drifted over time)
            if is_valid_crates_io_category_not_on_lib_rs(c) {
                continue;
            }
            warnings.insert(Warning::BadCategory(c.as_str().into()));
        }

        if category_slugs.is_empty() && bad_categories.is_empty() {
            warnings.insert(Warning::NoCategories);
        }

        if package.keywords.is_empty() {
            warnings.insert(Warning::NoKeywords);
        }

        let tmp: Vec<_>;
        if let Some(overrides) = self.category_overrides.get(origin.short_crate_name()) {
            tmp = overrides.iter().map(|k| k.to_string()).collect();
            category_slugs = categories::Categories::fixed_category_slugs(&tmp, &mut bad_categories);
        }

        category_slugs.iter().for_each(|k| debug_assert!(categories::CATEGORIES.from_slug(k).1, "'{}' must exist", k));

        let extracted_auto_keywords = feat_extractor::auto_keywords(&manifest, source_data.github_description.as_deref(), readme_text.as_deref());

        let db_index = self.crate_db.index_latest(CrateVersionData {
            cache_key,
            category_slugs: &category_slugs,
            bad_categories: &bad_categories,
            authors: &authors,
            origin,
            repository: repository.as_ref(),
            deps_stats: &weighed_deps,
            is_build, is_dev,
            manifest: &manifest,
            source_data: &source_data,
            extracted_auto_keywords,
        });
        let d = timeout("db-index", 16, db_index.map_err(anyhow::Error::from)).await?;

        for w in &warnings {
            debug!("{}: {}", package.name, w);
        }

        let cached = CachedCrate {
            manifest,
            cache_key,
            derived: rich_crate::Derived {
                categories: d.categories,
                keywords: d.keywords,
                path_in_repo: source_data.path_in_repo,
                vcs_info_git_sha1: source_data.vcs_info_git_sha1,
                language_stats: source_data.language_stats,
                crate_compressed_size: source_data.crate_compressed_size,
                crate_decompressed_size: source_data.crate_decompressed_size,
                is_nightly: source_data.is_nightly,
                capitalized_name: source_data.capitalized_name,
                readme: source_data.readme,
                lib_file: source_data.lib_file,
                bin_file: source_data.bin_file,
                has_buildrs: source_data.has_buildrs,
                has_code_of_conduct: source_data.has_code_of_conduct,
                is_yanked: source_data.is_yanked,
            },
            warnings,
        };
        self.derived_storage.set_serialize((&origin.to_str(), ""), &cached)?;

        if !maintenance_only_reindexing {
            self.event_log.post(&SharedEvent::CrateIndexed(origin.to_str()))?;
        }
        Ok(())
    }

    async fn fetch_rich_crate_version_data(&self, origin: &Origin) -> CResult<(CrateVersionSourceData, Manifest, HashSet<Warning>, u64)> {
        let ((source_data, manifest, warnings), cache_key) = match origin {
            Origin::CratesIo(ref name) => {
                let cache_key = self.index.cache_key_for_crate(name)?;
                let ver = self.index.crate_highest_version(name, false).context("rich_crate_version2")?;
                let res = watch("reindexing-cio-data", self.rich_crate_version_data_from_crates_io(ver)).await.context("rich_crate_version_data_from_crates_io")?;
                (res, cache_key)
            },
            Origin::GitHub { .. } | Origin::GitLab { .. } => {
                if !self.crate_exists(origin) {
                    return Err(KitchenSinkErr::GitCrateNotAllowed(origin.to_owned()).into())
                }
                let res = watch("reindexing-repodata", self.rich_crate_version_from_repo(origin)).await?;
                (res, 0)
            },
        };
        Ok((source_data, manifest, warnings, cache_key))
    }

    fn capitalized_name<'a>(name: &str, source_sentences: impl Iterator<Item = &'a str>) -> String {
        let mut ch = name.chars();
        let mut first_capital = String::with_capacity(name.len());
        first_capital.extend(ch.next().unwrap().to_uppercase());
        first_capital.extend(ch.map(|c| if c == '_' { ' ' } else { c }));

        let mut words = HashMap::with_capacity(100);
        let lcname = name.to_ascii_lowercase();
        let shouty = name.to_ascii_uppercase();
        for s in source_sentences {
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

        let can_capitalize = words.get(&first_capital).is_some();
        if let Some((name, _)) = words.into_iter().max_by_key(|&(_, v)| v) {
            name
        } else if can_capitalize {
            first_capital
        } else {
            name.to_owned()
        }
    }

    // deps that are closely related to crates in some category
    fn dep_interesting_for_index(name: &str) -> Option<bool> {
        match name {
            "futures" | "async-trait" | "tokio" | "actix-web" | "warp" | "rocket_codegen" | "iron" | "rusoto_core" | "rocket" | "router" | "async-std" |
            "constant_time_eq" | "digest" | "subtle" |
            "quoted_printable" | "mime" | "rustls" | "websocket" | "hyper" |
            "piston2d-graphics" | "amethyst_core" | "amethyst" | "specs" | "piston" | "allegro" | "minifb" | "bevy" |
            "rgb" | "imgref" | "gstreamer" | "gtk" | "gtk4" |
            "bare-metal" | "usb-device" |
            "core-foundation" |
            "proc-macro2" | "proc-macro-hack" | "darling" | "quote" |
            "cargo" | "cargo_metadata" | "git2" | "dbus" |
            "hound" | "lopdf" |
            "nom" | "lalrpop" | "combine" | "pest" | "unicode-xid" |
            "clap" | "structopt" | "ansi_term" |
            "alga" | "bio" | "nalgebra" |
            "syntect" | "stdweb" | "parity-wasm" | "wasm-bindgen" |
            "solana-program" | "ethabi" | "bitcoin" | "ink_primitives" | "parity-scale-codec" | "ethnum" | "borsh" | "solana-sdk" | "anchor-lang" | "mpl-token-metadata" | "spl-token" => Some(true),
            /////////
            "threadpool" | "rayon" | "md5" | "arrayref" | "memmmap" | "xml" | "crossbeam" | "pyo3" |
            "rustc_version" | "crossbeam-channel" | "cmake" | "errno" | "zip" | "enum_primitive" | "pretty_env_logger" |
            "skeptic" | "crc" | "hmac" | "sha1" | "serde_macros" | "serde_codegen" | "derive_builder" |
            "derive_more" | "ron" | "fxhash" | "simple-logger" | "chan" | "stderrlog" => Some(false),
            _ => None,
        }
    }

    pub async fn inspect_repo_manifests(&self, repo: &Repo) -> CResult<Vec<FoundManifest>> {
        let checkout = self.checkout_repo(repo.clone(), true).await?;
        let (has, _) = tokio::task::block_in_place(|| crate_git_checkout::find_manifests(&checkout))?;
        Ok(has)
    }

    async fn checkout_repo(&self, repo: Repo, shallow: bool) -> Result<crate_git_checkout::Repository, KitchenSinkErr> {
        if stopped() {return Err(KitchenSinkErr::Stopped);}

        let git_checkout_path = self.git_checkout_path.clone();
        timeout("checkout", 300, spawn_blocking(move || {
            crate_git_checkout::checkout(&repo, &git_checkout_path, shallow)
                .map_err(|e| { error!("{}", e); KitchenSinkErr::GitCheckoutFailed })
        }).map_err(|_| KitchenSinkErr::GitCheckoutFailed)).await?
    }

    pub async fn index_repo(&self, repo: &Repo, as_of_version: &str) -> CResult<()> {
        let _f = self.throttle.acquire().await;
        if stopped() {return Err(KitchenSinkErr::Stopped.into());}
        let checkout = self.checkout_repo(repo.clone(), false).await?;
        let url = repo.canonical_git_url().into_owned();
        let (checkout, manif) = spawn_blocking(move || {
            let (manif, warnings) = crate_git_checkout::find_manifests(&checkout)
                .with_context(|| format!("find manifests in {}", url))?;
            for warn in warnings {
                warn!("warning: {}", warn.0);
            }
            Ok::<_, CError>((checkout, manif.into_iter().filter_map(|found| {
                let inner_path = found.inner_path;
                let manifest = found.manifest;
                let commit = found.commit;
                manifest.package.map(move |p| (inner_path, p.name, commit.to_string()))
            })))
        }).await??;
        self.crate_db.index_repo_crates(repo, manif).await.context("index rev repo")?;

        if let Repo { host: RepoHost::GitHub(ref repo), .. } = repo {
            if let Some(commits) = watch("commits", self.repo_commits(repo, as_of_version)).await? {
                for c in commits {
                    if let Some(a) = c.author {
                        self.index_user_m(&a, &c.commit.author).await?;
                    }
                    if let Some(a) = c.committer {
                        self.index_user_m(&a, &c.commit.committer).await?;
                    }
                }
            }
        }

        tokio::task::yield_now().await;
        if stopped() {return Err(KitchenSinkErr::Stopped.into());}

        let mut changes = Vec::new();
        tokio::task::block_in_place(|| {
            let url = repo.canonical_git_url();
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
                                    if !is_alnum(dep1) {
                                        error!("Bad crate name {} in {}", dep1, url);
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
                        if !is_alnum(&crate_name) {
                            error!("Bad crate name {} in {}", crate_name, url);
                            continue;
                        }

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
            })
        })?;
        self.crate_db.index_repo_changes(repo, &changes).await?;

        Ok(())
    }

    #[inline]
    pub fn login_by_github_id(&self, id: u64) -> CResult<String> {
        Ok(self.user_db.login_by_github_id(id)?)
    }

    #[inline]
    pub fn user_by_email(&self, email: &str) -> CResult<Option<User>> {
        self.user_db.user_by_email(email).context("user_by_email")
    }

    pub async fn user_by_github_login(&self, github_login: &str) -> CResult<Option<User>> {
        if github_login.contains(':') {
            warn!("bad login {github_login}");
            anyhow::bail!("bad login {github_login}");
        }
        tokio::task::yield_now().await;
        let db_fetched = tokio::task::block_in_place(|| self.user_db.user_by_github_login(github_login));
        if let Ok(Some(gh)) = db_fetched.map_err(|e| error!("db user {}: {}", github_login, e)) {
            if gh.created_at.is_some() { // unfortunately name is optional and can be None
                return Ok(Some(gh));
            } else {
                debug!("db gh user missing created_at {}", github_login);
            }
        } else {
            debug!("gh user cache miss {}", github_login);
        }
        let u = Box::pin(self.gh.user_by_login(github_login)).await.map_err(|e| {error!("gh user {}: {}", github_login, e); e})?; // errs on 404
        if let Some(u) = &u {
            let _ = tokio::task::block_in_place(|| {
                self.user_db.index_user(u, None, None)
                    .map_err(|e| error!("index user {}: {} {:?}", github_login, e, u))
            });
        }
        debug!("fetched user {:?}", u);
        Ok(u)
    }

    pub fn rustc_compatibility_no_deps(&self, all: &RichCrate) -> Result<CompatByCrateVersion, KitchenSinkErr> {
        let db = self.build_db()?;
        self.rustc_compatibility_inner_non_recursive(all, db, 0)
    }

    fn rustc_compatibility_inner_non_recursive(&self, all: &RichCrate, db: &BuildDb, bump_min_expected_rust: u16) -> Result<CompatByCrateVersion, KitchenSinkErr> {
        let mut c = db.get_compat(all.origin())
            .map_err(|e| error!("bad compat: {}", e))
            .unwrap_or_default();

        // insert versions that aren't in the db, to have the full list.
        // doing so before postproc will copy data to these.
        for ver in all.versions().iter().filter(|v| !v.yanked) {
            let semver: SemVer = match ver.num.parse() {
                Ok(v) => v,
                Err(e) => {
                    error!("bad semver: {} {}", ver.num, e);
                    continue;
                },
            };
            let c = c.entry(semver).or_insert_with(Default::default);

            let created = ver.created_at;
            if let Some(expected_rust) = Self::rustc_release_from_date(&created) {
                c.add_compat(expected_rust.saturating_sub(bump_min_expected_rust), Compat::ProbablyWorks, Some("Assumed from release date".into()));
            }
        }
        // this is needed to copy build failures from non-matching versions to matching versions
        BuildDb::postprocess_compat(&mut c);
        Ok(c)
    }

    fn crate_rustc_compat_get_cached(&self, origin: &Origin) -> Option<CompatByCrateVersion> {
        self.crate_rustc_compat_cache.read().get(origin).cloned()
    }

    fn crate_rustc_compat_set_cached(&self, origin: Origin, val: CompatByCrateVersion) {
        let mut w = self.crate_rustc_compat_cache.write();
        if w.len() > 5000 {
            w.clear();
        }
        w.insert(origin, val);
    }

    pub fn index_msrv_from_manifest(&self, origin: &Origin, manifest: &Manifest) -> CResult<RustcMinorVersion> {
        let db = self.build_db()?;
        let package = manifest.package();
        let mut working_msrv = None;

        let (mut msrv, mut reason) = match package.edition {
            Edition::E2015 => (1u16, "???"),
            Edition::E2018 => (31, "edition 2018"),
            Edition::E2021 => (56, "edition 2021"),
        };

        if msrv < 37 && package.default_run.is_some() {
            msrv = 37;
            reason = "default-run";
        }

        // uses Option as iter
        let mut profiles = manifest.profile.release.iter().chain(&manifest.profile.dev).chain(&manifest.profile.test).chain(&manifest.profile.bench).chain(&manifest.profile.doc);
        if msrv < 41 && profiles.any(|p| p.build_override.is_some() || !p.package.is_empty()) {
            msrv = 41;
            reason = "build-override";
        }

        if msrv < 51 && matches!(package.resolver, Some(rich_crate::Resolver::V2)) {
            msrv = 51;
            reason = "resolver = 2";
        }

        if msrv < 59 && profiles.any(|p| p.strip.is_some()) {
            msrv = 59;
            reason = "strip";
        }

        if msrv < 60 && manifest.features.values().flat_map(|f| f.iter()).any(|req| req.starts_with("dep:") || req.contains('?')) {
            msrv = 60;
            reason = "dep:feature";
        }

        if let Some(minor) = package.rust_version.as_ref().and_then(|v| v.split('.').nth(1)).and_then(|minor| minor.parse().ok()) {
            if msrv < minor {
                reason = "rust-version";
                msrv = minor;
                working_msrv = Some(minor);
            }
        }

        if msrv > 1 {
            debug!("Detected {:?} as msrv 1.{} ({})", origin, msrv, reason);
            let latest_bad_rustc = msrv - 1;
            let ver = SemVer::parse(package.version())?;
            db.set_compat(origin, &ver, latest_bad_rustc, Compat::DefinitelyIncompatible, reason)?;
            if let Some(working_msrv) = working_msrv {
                db.set_compat(origin, &ver, working_msrv, Compat::ProbablyWorks, "rust-version")?;
            }
        }
        Ok(msrv)
    }

    pub async fn rustc_compatibility(&self, all: &RichCrate) -> Result<CompatByCrateVersion, KitchenSinkErr> {
        let in_progress = Arc::new(Mutex::new(HashSet::new()));
        Ok(self.rustc_compatibility_inner(all, in_progress, 0).await?.unwrap())
    }

    /// Relaxes heuristics to run more builds
    pub async fn rustc_compatibility_for_builder(&self, all: &RichCrate) -> Result<CompatByCrateVersion, KitchenSinkErr> {
        let in_progress = Arc::new(Mutex::new(HashSet::new()));
        Ok(self.rustc_compatibility_inner(all, in_progress, 1).await?.unwrap())
    }

    pub fn all_crate_compat(&self) -> CResult<HashMap<Origin, CompatByCrateVersion>> {
        let db = self.build_db()?;
        let mut all = db.get_all_compat_by_crate()?;
        // TODO: apply rustc_release_from_date
        all.iter_mut().for_each(|(_, v)| BuildDb::postprocess_compat(v));
        Ok(all)
    }

    fn build_db(&self) -> Result<&BuildDb, KitchenSinkErr> {
        if stopped() {return Err(KitchenSinkErr::Stopped);}

        self.crate_rustc_compat_db
            .get_or_try_init(|| BuildDb::new(self.data_path.join("builds.db")))
            .map_err(|_| KitchenSinkErr::BadRustcCompatData)
    }

    fn rustc_compatibility_inner<'a>(&'a self, all: &'a RichCrate, in_progress: Arc<Mutex<HashSet<Origin>>>, bump_min_expected_rust: u16) -> BoxFuture<'a, Result<Option<CompatByCrateVersion>, KitchenSinkErr>> { async move {
        if let Some(cached) = self.crate_rustc_compat_get_cached(all.origin()) {
            return Ok(Some(cached));
        }
        if !in_progress.lock().unwrap().insert(all.origin().clone()) {
            return Ok(None);
        }

        let db = self.build_db()?;
        let mut c = self.rustc_compatibility_inner_non_recursive(all, db, bump_min_expected_rust)?;

        // crates most often fail to compile because their dependencies fail
        if let Ok(vers) = self.all_crates_io_versions(all.origin()) {
            let vers = vers.iter()
            .filter_map(|v| {
                let crate_ver: SemVer = v.version().parse().ok()?;
                Some((v, crate_ver))
            })
            .map(|(v, crate_ver)| {
                let mut deps_to_check = Vec::with_capacity(v.dependencies().len());
                for dep in v.dependencies() {
                    if dep.is_optional() {
                        continue;
                    }
                    if dep.kind() == DependencyKind::Dev {
                        continue;
                    }
                    if let Some(t) = dep.target() {
                        debug!("ignoring {} because target {}", dep.name(), t);
                        continue;
                    }
                    if let Ok(req) = VersionReq::parse(dep.requirement()) {
                        let dep_origin = Origin::from_crates_io_name(dep.crate_name());
                        if &dep_origin == all.origin() {
                            // recursive check not supported
                            continue;
                        }
                        deps_to_check.push((dep_origin, req));
                    }
                }
                (Arc::new(crate_ver), deps_to_check)
            });

            // group by origin to avoid requesting rich_crate_async too much
            let mut by_dep = HashMap::new();
            for (crate_ver, deps_to_check) in vers {
                for (origin, req) in deps_to_check {
                    by_dep.entry(origin).or_insert_with(Vec::new).push((crate_ver.clone(), req));
                }
            }

            // fetch crate meta in parallel
            let deps = futures::future::join_all(by_dep.into_iter().map(|(dep_origin, reqs)| {
                let in_progress = Arc::clone(&in_progress);
                async move {
                    let dep_compat = if let Some(cached) = self.crate_rustc_compat_get_cached(&dep_origin) {
                        cached
                    } else {
                        debug!("recursing to get compat of {}", dep_origin.short_crate_name());
                        let dep_rich_crate = self.rich_crate_async(&dep_origin).await.ok()?;
                        self.rustc_compatibility_inner(&dep_rich_crate, in_progress, bump_min_expected_rust).await.ok()??
                    };
                    Some((dep_compat, dep_origin, reqs))
                }
            })).await;

            for (dep_compat, dep_origin, reqs) in deps.into_iter().flatten() {
                for (crate_ver, req) in reqs {
                    let c = match c.get_mut(&crate_ver) {
                        Some(c) => c,
                        None => {
                            c.insert((*crate_ver).clone(), Default::default());
                            c.get_mut(&crate_ver).unwrap()
                        },
                    };

                    // make note of dependencies that affect their dependee's msrv
                    let parent_newest_bad = c.newest_bad().unwrap_or(31);
                    // but for crates where we don't know real msrv yet, don't freak out about super old rustc compat
                    let parent_oldest_ok_lower_limit = c.oldest_ok().unwrap_or(0).saturating_sub(17); // min. 2 years back-compat assumed
                    let mut dependency_affects_msrv = false;

                    // find known-bad dep to bump msrv
                    let most_compatible_dependency = dep_compat.iter()
                        .filter(|(semver, _)| req.matches(semver))
                        .filter_map(|(semver, compat)| {
                            let n = compat.newest_bad().or_else(|| Some(compat.oldest_ok()?.saturating_sub(1)))?;
                            Some((semver, n))
                        })
                        .inspect(|&(_, n)| {
                            if n > parent_newest_bad && n >= parent_oldest_ok_lower_limit {
                                dependency_affects_msrv = true;
                            }
                        })
                        .min_by_key(|&(v, n)| (n, Reverse(v)));

                    if let Some((dep_found_ver, best_compat)) = most_compatible_dependency {
                        let dep_newest_bad = dep_compat.iter()
                            .filter(|(semver, _)| req.matches(semver))
                            .filter_map(|(_, compat)| compat.newest_bad())
                            .min().unwrap_or(0)
                            .min(best_compat); // sparse data with only few failures may be too pessimistic, because other versions may be compatible
                        if c.newest_bad().unwrap_or(0) < dep_newest_bad {
                            if dep_newest_bad > 19 {
                                debug!("{} {} MSRV went from {} to {} because of https://lib.rs/compat/{} {} = {}", all.name(), crate_ver, c.newest_bad().unwrap_or(0), dep_newest_bad, dep_origin.short_crate_name(), req, dep_found_ver);
                                let reason = format!("{} {}={} has MSRV {}", dep_origin.short_crate_name(), req, dep_found_ver, dep_newest_bad);
                                c.add_compat(dep_newest_bad, Compat::BrokenDeps, Some(reason.clone()));

                                // setting this will make builder skip this version.
                                // propagate problem only if the failure is certain, because dep_newest_bad
                                // may be too pessimistic if the dep lacks positive build data.
                                let dep_newest_bad_certain = dep_compat.iter()
                                    .filter(|(semver, _)| req.matches(semver))
                                    .filter_map(|(_, compat)| {
                                        compat.newest_bad_likely().or_else(|| Some(compat.oldest_ok()?.saturating_sub(1)))
                                    })
                                    .min()
                                    .unwrap_or(0);
                                if c.newest_bad().unwrap_or(0) < dep_newest_bad_certain {
                                    let _ = db.set_compat(all.origin(), &crate_ver, dep_newest_bad, Compat::BrokenDepsLikely, &reason);
                                }
                            }
                        } else if dependency_affects_msrv {
                            // keep track of this only if it's not reflected in `BrokenDeps` (i.e. happens sometimes, not always)
                            c.requires_dependency_version(dep_origin.short_crate_name(), dep_found_ver.clone(), best_compat);
                        }
                    }
                }
            }
        }
        // postprocess (again) to update
        BuildDb::postprocess_compat(&mut c);

        self.crate_rustc_compat_set_cached(all.origin().clone(), c.clone());
        Ok(Some(c))
    }.boxed()}

    fn rustc_release_from_date(date: &DateTime<Utc>) -> Option<u16> {
        let zero = Utc.ymd(2015,5,15).and_hms(0,0,0);
        let age = date.signed_duration_since(zero);
        let weeks = age.num_weeks();
        if weeks < 0 { return None; }
        Some(((weeks+1)/6) as _)
    }

    /// List of all notable crates
    /// Returns origin, rank, last updated unix timestamp
    #[inline]
    pub async fn sitemap_crates(&self) -> CResult<Vec<(Origin, f64, i64)>> {
        Ok(self.crate_db.sitemap_crates().await?)
    }

    /// If given crate is a sub-crate, return crate that owns it.
    /// The relationship is based on directory layout of monorepos.
    #[inline]
    pub async fn parent_crate_same_repo_unverified(&self, child: &RichCrateVersion) -> Option<Origin> {
        if !child.has_path_in_repo() {
            return None;
        }
        let repo = child.repository()?;
        let res = self.crate_db.parent_crate(repo, child.short_name()).await.ok()?;
        if res.as_ref().map_or(false, |p| p == child.origin()) {
            error!("Buggy parent_crate for: {:?}", child.origin());
            return None;
        }
        res
    }

    pub async fn parent_crate(&self, child: &RichCrateVersion) -> Option<ArcRichCrateVersion> {
        let parent_origin = self.parent_crate_same_repo_unverified(child).await.or_else(|| {
            // See if there's a crate that is a prefix for this name (even if it doesn't share a repo)
            let mut rest = child.short_name();
            while let Some((name, _)) = rest.rsplit_once(|c:char| c == '_' || c == '-') {
                let origin = Origin::try_from_crates_io_name(name)?;
                if self.crate_exists(&origin) {
                    return Some(origin);
                }
                rest = name;
            }
            None
        })?;
        // dependencies are not enough to establish relationship, because they're uni-directional,
        // and neither parent nor child may want false association

        let parent = self.rich_crate_version_stale_is_ok(&parent_origin).await.ok()?;
        if parent.is_yanked() {
            return None;
        }
        if self.are_namespace_related(&parent, child).await.ok()?.is_some() {
            Some(parent)
        } else {
            None
        }
    }

    /// Do these crate belong to the same project? (what would have been a namespace)
    /// None if unrelated, float 0..1 how related they are
    ///
    /// a is more trusted here, b is matched against it.
    pub async fn are_namespace_related(&self, a: &RichCrateVersion, b: &RichCrateVersion) -> CResult<Option<f32>> {
        // Don't recommend cryptocrap to normal people
        let a_is_dodgy = a.category_slugs().iter().any(|s| &**s == "cryptography::cryptocurrencies");
        let b_is_dodgy = b.category_slugs().iter().any(|s| &**s == "cryptography::cryptocurrencies");
        if a_is_dodgy != b_is_dodgy {
            return Ok(None);
        }

        let mut commonalities = 0;

        let a_repo_owner = a.repository().and_then(|r| r.github_host()).and_then(|gh| gh.owner_name());
        let have_same_repo_owner = {
            let b_repo_owner = b.repository().and_then(|r| r.github_host()).and_then(|gh| gh.owner_name());
            match (a_repo_owner, b_repo_owner) {
                (Some(a), Some(b)) if a.eq_ignore_ascii_case(b) => true,
                _ => false
            }
        };
        if have_same_repo_owner {
            commonalities += 1;
        }
        let have_same_homepage = match (a.homepage(), b.homepage()) {
            (Some(a), Some(b)) if a.trim_end_matches('/') == b.trim_end_matches('/') => true,
            _ => true,
        };
        if have_same_homepage {
            commonalities += 1;
        }
        let have_same_name_prefix = {
            let a_first_word = crate_name_namespace_prefix(a.short_name());
            let b_first_word = crate_name_namespace_prefix(b.short_name());
            a_first_word == b_first_word
        };
        if have_same_name_prefix {
            commonalities += 1;
        }

        // They need to look at least a bit related (these factors can be faked/squatted, so alone aren't enough)
        if 0 == commonalities {
            return Ok(None);
        }

        let commonalities_frac = commonalities as f32 / 3.;

        // this is strong
        let (common, max_owners) = self.common_real_owners(a.origin(), b.origin()).await?;
        if common > 0 {
            debug!("{} and {} are related {common}/{max_owners} owners", a.short_name(), b.short_name());
            return Ok(Some((common+1) as f32 / (max_owners+1) as f32 * commonalities_frac));
        }
        // they're related if have same repo owner, but make sure the repo url isn't fake
        if have_same_repo_owner && self.has_verified_repository_link(a).await && self.has_verified_repository_link(b).await {
            debug!("{} and {} are related not by owners, but by repo {} + {commonalities}", a.short_name(), b.short_name(), a_repo_owner.unwrap());
            return Ok(Some(0.1 * commonalities_frac));
        }
        Ok(None)
    }

    /// (common, out of how many). A is considered more important, b is matched against it.
    async fn common_real_owners(&self, a: &Origin, b: &Origin) -> CResult<(usize, usize)> {
        let (a_owners, b_owners) = futures::try_join!(self.crate_owners(a, CrateOwners::Strict), self.crate_owners(b, CrateOwners::Strict))?;
        let a_owners: HashSet<_> = a_owners.into_iter().filter(|o| !self.is_crates_io_login_on_shitlist(&o.crates_io_login)).filter_map(|o| o.github_id).collect();
        let b_owners: HashSet<_> = b_owners.into_iter().filter(|o| !self.is_crates_io_login_on_shitlist(&o.crates_io_login)).filter_map(|o| o.github_id).collect();
        let max = a_owners.len();
        let common_owners = a_owners.intersection(&b_owners).count();
        Ok((common_owners, max))
    }

    /// Crates are spilt into foo and foo-core. The core is usually uninteresting/duplicate.
    pub async fn is_sub_component(&self, k: &RichCrateVersion) -> bool {
        let name = k.short_name();
        if let Some(pos) = name.rfind(|c: char| c == '-' || c == '_') {
            match name.get(pos+1..) {
                Some("core" | "shared" | "runtime" | "codegen" | "private" | "internals" | "internal" |
                    "derive" | "macros" | "utils" | "util" | "lib" | "types" | "common" | "impl" | "fork" | "unofficial" | "hack") => {
                    if let Some(parent_name) = name.get(..pos-1) {
                        if Origin::try_from_crates_io_name(parent_name).map_or(false, |name| self.crate_exists(&name)) {
                            // TODO: check if owners overlap?
                            return true;
                        }
                    }
                    if self.parent_crate_same_repo_unverified(k).await.is_some() {
                        return true;
                    }
                },
                _ => {},
            }
        }
        false
    }

    /// Crates are spilt into foo and foo-core. The core is usually uninteresting/duplicate.
    pub fn is_internal_crate(&self, k: &RichCrateVersion) -> bool {
        let name = k.short_name();
        if let Some(pos) = name.rfind(|c: char| c == '-' || c == '_') {
            match name.get(pos+1..) {
                Some("private" | "internals" | "internal" | "impl") => {
                    if let Some(parent_name) = name.get(..pos-1) {
                        if Origin::try_from_crates_io_name(parent_name).map_or(false, |name| self.crate_exists(&name)) {
                            // TODO: check if owners overlap?
                            return true;
                        }
                    }
                },
                _ => {},
            }
        }
        false
    }

    async fn cachebust_string_for_repo(&self, crate_repo: &Repo) -> CResult<String> {
        if stopped() {return Err(KitchenSinkErr::Stopped.into());}

        Ok(self.crate_db.crates_in_repo(crate_repo).await
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
                let weeks = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).expect("clock").as_secs() / (3600 * 24 * 7);
                format!("w{}", weeks)
            }))
    }

    #[inline]
    pub async fn user_github_orgs(&self, github_login: &str) -> CResult<Option<Vec<UserOrg>>> {
        trace!("gh orgs {}", github_login);
        Ok(self.gh.user_orgs(github_login).await?)
    }

    #[inline]
    pub async fn github_org(&self, github_login: &str) -> CResult<Option<Org>> {
        trace!("gh org {}", github_login);
        Ok(self.gh.org(github_login).await?)
    }

    /// Returns (contrib, github user)
    async fn contributors_from_repo(&self, crate_repo: &Repo, owners: &[CrateOwner], found_crate_in_repo: bool) -> CResult<(bool, HashMap<String, (f64, User)>)> {
        let mut hit_max_contributor_count = false;
        match crate_repo.host() {
            // TODO: warn on errors?
            RepoHost::GitHub(ref repo) => {
                // don't use repo URL if it's not verified to belong to the crate
                if !found_crate_in_repo && !owners.iter().filter(|o| !o.contributor_only)
                        .filter_map(|o| o.github_login())
                        .any(|owner| owner.eq_ignore_ascii_case(&repo.owner)) {
                    return Ok((false, HashMap::new()));
                }

                // multiple crates share a repo, which causes cache churn when version "changes"
                // so pick one of them and track just that one version
                let cachebust = self.cachebust_string_for_repo(crate_repo).await.context("contrib")?;
                debug!("getting contributors for {:?}", repo);
                let contributors = match tokio::time::timeout(Duration::from_secs(10), self.gh.contributors(repo, &cachebust)).await {
                    Ok(c) => c.context("contributors")?.unwrap_or_default(),
                    Err(_timeout) => vec![],
                };
                if contributors.len() >= 100 {
                    hit_max_contributor_count = true;
                }
                let mut by_login: HashMap<String, (f64, User)> = HashMap::new();
                for contr in contributors {
                    if let Some(author) = contr.author {
                        if author.user_type == UserType::Bot {
                            continue;
                        }
                        let count = contr.weeks.iter()
                            .map(|w| {
                                w.commits.max(0) as f64 +
                                ((w.added_l.abs() + w.deleted_l.abs()*2) as f64).sqrt()
                            }).sum::<f64>();
                        use std::collections::hash_map::Entry;
                        match by_login.entry(author.login.to_ascii_lowercase()) {
                            Entry::Vacant(e) => {
                                if let Ok(Some(user)) = self.user_by_github_login(&author.login).await {
                                    e.insert((count, user));
                                }
                            },
                            Entry::Occupied(mut e) => {
                                e.get_mut().0 += count;
                            },
                        }
                    }
                }
                Ok((hit_max_contributor_count, by_login))
            },
            RepoHost::BitBucket(..) |
            RepoHost::GitLab(..) |
            RepoHost::Other => Ok((false, HashMap::new())), // TODO: could use git checkout...
        }
    }

    pub async fn has_verified_repository_link(&self, k: &RichCrateVersion) -> bool {
        let repo = match k.repository() {
            Some(repo) => repo,
            None => return false,
        };
        if let Ok(Some(_)) = self.crate_db.path_in_repo(repo, k.short_name()).await {
            return true;
        }
        if let Some(repo_owner) = repo.github_host().and_then(|gh| gh.owner_name()) {
            if let Ok(owners) = self.crate_owners(k.origin(), CrateOwners::Strict).await {
                return owners.iter().filter_map(|o| o.github_login()).any(|gh| gh == repo_owner);
            }
        }
        false
    }

    /// Merge authors, owners, contributors
    pub async fn all_contributors<'a>(&self, krate: &'a RichCrateVersion) -> CResult<(Vec<CrateAuthor<'a>>, Vec<CrateAuthor<'a>>, bool, usize)> {
        let owners = self.crate_owners(krate.origin(), CrateOwners::All).await?;

        let (hit_max_contributor_count, mut contributors_by_login) = match krate.repository().as_ref() {
            // Only get contributors from github if the crate has been found in the repo,
            // otherwise someone else's repo URL can be used to get fake contributor numbers
            Some(crate_repo) => watch("contrib", self.contributors_from_repo(crate_repo, &owners, self.has_verified_repository_link(krate).await)).await?,
            None => (false, HashMap::new()),
        };

        let mut authors = HashMap::with_capacity(krate.authors().len());
        for (i, author) in krate.authors().iter().enumerate() {
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
                        authors.insert(AuthorId::GitHub(id), ca);
                        continue;
                    }
                }
                if let Some(ref url) = author.url {
                    let gh_url = "https://github.com/";
                    if url.to_ascii_lowercase().starts_with(gh_url) {
                        let login = url[gh_url.len()..].split('/').next().expect("can't happen");
                        if let Ok(Some(gh)) = self.user_by_github_login(login).await {
                            let id = gh.id;
                            ca.github = Some(gh);
                            authors.insert(AuthorId::GitHub(id), ca);
                            continue;
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
                            authors.insert(AuthorId::GitHub(id), ca);
                            continue;
                        }
                    }
                }
                let key = author.email.as_ref().map(|e| AuthorId::Email(e.to_ascii_lowercase()))
                    .or_else(|| author.name.as_ref().map(|n| AuthorId::Name(n.to_lowercase())))
                    .unwrap_or(AuthorId::Meh(i));
                authors.insert(key, ca);
            }

        for owner in owners {
            if let Ok(user) = self.owners_github(&owner).await {
                match authors.entry(AuthorId::GitHub(user.id)) {
                    Occupied(mut e) => {
                        let e = e.get_mut();
                        if !owner.contributor_only {
                            e.owner = true;
                        }
                        if e.info.is_none() {
                            e.info = Some(Cow::Owned(Author {
                                name: Some(owner.name().to_owned()).filter(|n| !n.is_empty()),
                                email: None,
                                url: owner.url.as_deref().map(|u| u.into()),
                            }));
                        }
                        if e.github.is_none() {
                            e.github = Some(user);
                        } else if let Some(ref mut gh) = e.github {
                            if gh.name.is_none() && !owner.name().is_empty() {
                                gh.name = Some(owner.name().into());
                            }
                        }
                    },
                    Vacant(e) => {
                        e.insert(CrateAuthor {
                            contribution: 0.,
                            github: Some(user),
                            info: Some(Cow::Owned(Author {
                                name: Some(owner.name().to_owned()).filter(|n| !n.is_empty()),
                                email: None,
                                url: owner.url.map(|u| u.to_string()),
                            })),
                            nth_author: None,
                            owner: !owner.contributor_only,
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

        for author in authors.values_mut() {
            if let Some(ref mut gh) = author.github {
                if gh.name.is_none() {
                    let res = self.user_by_github_login(&gh.login).await;
                    if let Ok(Some(new_gh)) = res {
                        *gh = new_gh
                    }
                }
            }
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
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal))
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


        authors.sort_unstable_by(|a, b| {
            fn score(a: &CrateAuthor<'_>) -> f64 {
                let o = if a.owner { 200. } else { 1. };
                o * (a.contribution + 10.) / (1 + a.nth_author.unwrap_or(99)) as f64
            }
            score(b).partial_cmp(&score(a)).unwrap_or(Ordering::Equal)
        });


        // this is probably author from contribs + author from authors field, which means it's a dupe.
        // this needs to undo previous merge for the next check
        if authors.len() == 2 && authors.iter().any(|a| !a.owner) && owners.is_empty() {
            if let Some(index) = authors.iter().position(|a| a.owner) {
                owners.push(authors.remove(index));
            }
        }
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

        authors.truncate(20); // long lists look spammy

        let owners_partial = authors.iter().any(|a| a.owner);
        Ok((authors, owners, owners_partial, if hit_max_contributor_count { 100 } else { contributors }))
    }

    #[inline]
    async fn owners_github(&self, owner: &CrateOwner) -> CResult<User> {
        // This is a bit weak, since logins are not permanent
        if let Some(user) = self.user_by_github_login(owner.github_login().ok_or(KitchenSinkErr::OwnerWithoutLogin)?).await? {
            return Ok(user);
        }
        Err(KitchenSinkErr::OwnerWithoutLogin.into())
    }

    #[inline]
    pub async fn crates_of_author(&self, aut: &RichAuthor) -> CResult<Vec<CrateOwnerRow>> {
        Ok(self.crate_db.crates_of_author(aut.github.id).await?)
    }

    fn owners_from_audit(current_owners: Vec<CrateOwner>, meta: CrateMetaFile) -> Vec<CrateOwner> {
        let mut current_owners_by_login: HashMap<_, _> = current_owners.into_iter().map(|o| (o.crates_io_login.to_ascii_lowercase(), o)).collect();

        // latest first
        let mut actions: Vec<_> = meta.versions.into_iter().flat_map(|v| v.audit_actions).collect();
        actions.sort_unstable_by(|a,b| b.time.cmp(&a.time));

        // audit actions contain logins which are not present in owners, probably because they're GitHub team members
        for (idx, a) in actions.into_iter().enumerate() {
            if let Some(o) = current_owners_by_login.get_mut(&a.user.login.to_ascii_lowercase()) {
                if o.invited_at.as_ref().map_or(true, |l| l < &a.time) {
                    o.invited_at = Some(a.time.clone());
                }
                if o.last_seen_at.as_ref().map_or(true, |l| l < &a.time) {
                    o.last_seen_at = Some(a.time);
                }
            } else {
                current_owners_by_login.insert(a.user.login.to_ascii_lowercase(), CrateOwner {
                    kind: OwnerKind::User,
                    url: Some(format!("https://github.com/{}", a.user.login).into()),
                    crates_io_login: a.user.login,
                    name: a.user.name,
                    github_id: None,
                    avatar: a.user.avatar,
                    last_seen_at: Some(a.time.clone()),
                    invited_at: Some(a.time),
                    invited_by_github_id: None,
                    // most recent action is assumed to be done by an owner,
                    // but we can't be sure about past ones
                    contributor_only: idx > 0,
                });
            }
        }
        current_owners_by_login.into_iter().map(|(_, v)| v).collect()
    }

    /// true if it's verified current owner, false if may be a past owner (contributor now)
    pub async fn crate_owners(&self, origin: &Origin, set: CrateOwners) -> CResult<Vec<CrateOwner>> {
        let mut owners: Vec<_> = match origin {
            Origin::CratesIo(crate_name) => {
                let current_owners = async {
                    Ok(match self.crates_io_owners_cache.get(crate_name)? {
                        Some(o) => o,
                        None => timeout("owners-fallback", 3, self.crates_io.crate_owners(crate_name, "fallback").map(|r| r.map_err(KitchenSinkErr::from))).await?.unwrap_or_default(),
                    })
                };
                if set == CrateOwners::Strict {
                    let mut owners = current_owners.await?;
                    // anyone can join rust-bus, so it's meaningless as a common owner between crates
                    owners.retain(|o| !is_shared_collective_login(&o.crates_io_login) && o.github_id != Some(38887296));
                    owners
                } else {
                    let (current_owners, meta) = futures::try_join!(current_owners, self.crates_io_meta(crate_name))?;
                    Self::owners_from_audit(current_owners, meta)
                }
            },
            Origin::GitLab {..} => vec![],
            Origin::GitHub {repo, ..} => vec![
                CrateOwner {
                    avatar: None,
                    // FIXME: read from GH
                    url: Some(format!("https://github.com/{}", repo.owner).into()),
                    // FIXME: read from GH
                    crates_io_login: repo.owner.clone(),
                    kind: OwnerKind::User, // FIXME: crates-io uses teams, and we'd need to find the right team? is "owners" a guaranteed thing?
                    name: None,

                    invited_at: None,
                    github_id: None,
                    invited_by_github_id: None,
                    last_seen_at: None,
                    contributor_only: false,
                }
            ],
        };
        let _ = join_all(owners.iter_mut().map(|owner: &mut CrateOwner| async move {
            if owner.github_id.is_none() && owner.github_login().is_some() {
                match self.user_by_github_login(owner.github_login().unwrap()).await {
                    Ok(Some(user)) => {
                        owner.github_id = Some(user.id);
                    },
                    Ok(None) => warn!("owner {} of {origin:?} not found", owner.crates_io_login),
                    Err(e) => warn!("can't get owner {} of {origin:?}: {e}", owner.crates_io_login),
                }
            }
        })).await;
        Ok(owners)
    }

    pub fn crate_tarball_download_url(&self, k: &RichCrateVersion) -> Option<String> {
        if k.origin().is_crates_io() {
            Some(self.crates_io.crate_data_url(k.short_name(), k.version()))
        } else {
            None
        }
    }

    pub fn index_stats_histogram(&self, kind: &str, data: StatsHistogram) -> CResult<()> {
        Ok(self.stats_histograms.set(kind, data)?)
    }

    pub fn get_stats_histogram(&self, kind: &str) -> CResult<Option<StatsHistogram>> {
        Ok(self.stats_histograms.get(kind)?)
    }

    /// Direct reverse dependencies, but with release dates (when first seen or last used)
    pub fn index_dependers_liveness_ranges(&self, origin: &Origin, ranges: Vec<DependerChanges>) -> CResult<()> {
        self.depender_changes.set(origin.to_str(), ranges)?;
        Ok(())
    }

    /// Direct reverse dependencies, but with release dates (when first seen or last used)
    pub fn depender_changes(&self, origin: &Origin) -> CResult<Vec<DependerChangesMonthly>> {
        let daily_changes = self.depender_changes.get(origin.to_str().as_str())?.unwrap_or_default();
        if daily_changes.is_empty() {
            return Ok(Vec::new());
        }

        // We're going to use weirdo 30-day months, with excess going into december
        // which makes data more even and pads the December's holiday drop a bit
        let mut by_month = HashMap::with_capacity(daily_changes.len());
        for d in &daily_changes {
            let w = by_month.entry((d.at.y, (d.at.o / 30).min(11))).or_insert(DependerChangesMonthly {
                year: d.at.y,
                month0: (d.at.o/30).min(11),
                added: 0, added_total: 0,
                removed: 0, removed_total: 0,
                expired: 0, expired_total: 0,
            });
            w.added += d.added as u32;
            w.removed += d.removed as u32;
            w.expired += d.expired as u32;
        }

        let first = &daily_changes[0];
        let last = daily_changes.last().unwrap();
        let mut curr = (first.at.y, (first.at.o / 30).min(11));
        let end = (last.at.y, (last.at.o / 30).min(11));
        let mut monthly = Vec::with_capacity(by_month.len());
        let mut added_total = 0;
        let mut removed_total = 0;
        let mut expired_total = 0;
        while curr <= end {
            let mut e = by_month.get(&curr).copied().unwrap_or(DependerChangesMonthly {
                year: curr.0, month0: curr.1,
                added: 0, removed: 0, expired: 0,
                added_total: 0, removed_total: 0, expired_total: 0,
            });
            added_total += e.added;
            expired_total += e.expired;
            removed_total += e.removed;
            e.added_total = added_total;
            e.expired_total = expired_total;
            e.removed_total = removed_total;
            monthly.push(e);
            curr.1 += 1;
            if curr.1 > 11 {
                curr.0 += 1;
                curr.1 = 0;
            }
        }
        Ok(monthly)
    }

    /// 1.0 - still at its peak
    /// < 1 - heading into obsolescence
    /// < 0.3 - dying
    ///
    /// And returns number of active direct deps
    pub async fn former_glory(&self, origin: &Origin) -> CResult<Option<(f64, u32)>> {
        let mut direct_rev_deps = 0;
        let mut indirect_reverse_optional_deps = 0;
        if let Some(deps) = self.crates_io_dependents_stats_of(origin).await? {
            direct_rev_deps = deps.direct.all();
            indirect_reverse_optional_deps = (deps.runtime.def as u32 + deps.runtime.opt as u32)
                .max(deps.dev as u32)
                .max(deps.build.def as u32 + deps.build.opt as u32);
        }

        let depender_changes = self.depender_changes(origin)?;
        if let Some(current_active) = depender_changes.last() {
            let peak_active = depender_changes.iter().map(|m| m.running_total()).max().unwrap_or(0);
            // laplace smooth unpopular crates
            let min_relevant_dependers = 15;
            let former_glory = 1f64.min((current_active.running_total() + min_relevant_dependers + 1) as f64 / (peak_active + min_relevant_dependers) as f64);

            // If a crate is used mostly indirectly, it matters less whether it's losing direct users
            let indirect_to_direct_ratio = 1f64.min((direct_rev_deps * 3) as f64 / indirect_reverse_optional_deps.max(1) as f64);
            let indirect_to_direct_ratio = (0.9 + indirect_to_direct_ratio) * 0.5;
            let former_glory = former_glory * indirect_to_direct_ratio + (1. - indirect_to_direct_ratio);

            // if it's being mostly removed, accelerate its demise. laplace smoothed for small crates
            let removals_fraction = 1. - (current_active.expired_total + 10) as f64 / (current_active.removed_total + current_active.expired_total + 10) as f64;
            let mut powf = 1.0 + removals_fraction * 0.7;

            // if it's clearly declining, accelerate its demise
            if let Some(last_quarter) = depender_changes.get(depender_changes.len().saturating_sub(3)) {
                if last_quarter.running_total() > current_active.running_total() {
                    powf += 0.5;
                }
            }
            Ok(Some((former_glory.powf(powf), current_active.running_total())))
        } else {
            Ok(None)
        }
    }

    pub async fn index_crates_io_crate_all_owners(&self, all_owners: Vec<(Origin, Vec<CrateOwner>)>) -> CResult<()> {
        if stopped() {return Err(KitchenSinkErr::Stopped.into());}
        self.crate_db.index_crate_all_owners(&all_owners).await?;

        let users = all_owners.iter().flat_map(|(_, owners)| owners.iter().filter_map(|o| {
            let login = if o.crates_io_login.starts_with("github:") {
                o.github_login().unwrap().into()
            } else {
                o.crates_io_login.clone()
            };
            Some(User {
                id: o.github_id?,
                login,
                name: o.name.clone(),
                avatar_url: o.avatar.clone(),
                gravatar_id: None,
                html_url: o.url.as_deref().unwrap_or("").into(),
                blog: None,
                user_type: match o.kind {
                    OwnerKind::Team => UserType::Org,
                    OwnerKind::User => UserType::User,
                },
                created_at: None,
                two_factor_authentication: None,
            })
        })).collect::<Vec<_>>();

        if stopped() {return Err(KitchenSinkErr::Stopped.into());}
        self.user_db.index_users(&users)?;

        for (origin, owners) in all_owners {
            if let Origin::CratesIo(name) = origin {
                self.crates_io_owners_cache.set(name, owners)?;
            }
        }
        Ok(())
    }

    // Sorted from the top, returns origins
    pub async fn top_crates_in_category(&self, slug: &str) -> CResult<Arc<Vec<Origin>>> {
        use std::collections::hash_map::Entry::*;

        let cell = {
            let mut cache = self.top_crates_cached.lock().unwrap();
            match cache.entry(slug.to_owned()) {
                Occupied(e) => Arc::clone(e.get()),
                Vacant(e) => {
                    let cell = Arc::new(DoubleCheckedCell::new());
                    e.insert(cell.clone());
                    cell
                }
            }
        };

        let res = cell.get_or_try_init(async {
            watch("topcc", async {
                let (total_count, _) = self.category_crate_count(slug).await?;
                let wanted_num = ((total_count / 2 + 25) / 50 * 50).max(100);

                let mut crates = if slug == "uncategorized" {
                    self.crate_db.top_crates_uncategorized(wanted_num + 50).await?
                } else {
                    self.crate_db.top_crates_in_category_partially_ranked(slug, wanted_num + 50).await?
                };
                watch("dupes", self.knock_duplicates(&mut crates)).await;
                let crates: Vec<_> = crates.into_iter().map(|(o, _)| o).take(wanted_num as usize).collect();
                Ok::<_, anyhow::Error>(Arc::new(crates))
            }).await
        }).await?;
        Ok(Arc::clone(res))
    }

    /// To make categories more varied, lower score of crates by same authors, with same keywords
    async fn knock_duplicates(&self, crates: &mut Vec<(Origin, f64)>) {
        let deadline = Instant::now() + Duration::from_secs(4);
        let with_owners = futures::stream::iter(crates.drain(..))
        .map(|(o, score)| async move {
            if Instant::now() > deadline {
                warn!("Everything timed out in ranking");
                return Some((o, score, vec![], vec![]));
            }
            let get_crate = tokio::time::timeout_at(deadline, self.rich_crate_version_stale_is_ok(&o));
            let (k, owners) = futures::join!(get_crate, self.crate_owners(&o, CrateOwners::All));
            let keywords = match k {
                Ok(Ok(c)) => {
                    if c.is_yanked() {
                        return None;
                    }
                    c.keywords().to_owned()
                },
                Err(_) => {
                    warn!("{:?} Timed out in ranking", o);
                    Vec::new()
                },
                Ok(Err(e)) => {
                    error!("Skipping dedup {:?} because {}", o, e);
                    for e in e.chain() {
                        error!(" • because {}", e);
                    }
                    return None;
                },
            };
            let owners = owners.unwrap_or_default();
            Some((o, score, owners, keywords))
        })
        .buffer_unordered(16)
        .filter_map(|x| async {x})
        .collect::<Vec<_>>().await;

        let mut top_keywords = HashMap::default();
        for (_, _, _, keywords) in &with_owners {
            for k in keywords {
                *top_keywords.entry(k).or_insert(0u32) += 1;
            }
        }
        let mut top_keywords: Vec<_> = top_keywords.into_iter().collect();
        top_keywords.sort_unstable_by(|a, b| b.1.cmp(&a.1));
        let top_keywords: HashSet<_> = top_keywords.iter().copied().take((top_keywords.len() / 10).min(10).max(2)).map(|(k, _)| k.to_string()).collect();

        crates.clear();
        let mut seen_owners = HashMap::new();
        let mut seen_keywords = HashMap::new();
        let mut seen_owner_keywords = HashMap::new();
        for (origin, score, owners, keywords) in &with_owners {
            let mut weight_sum = 0;
            let mut score_sum = 0.0;
            for owner in owners.iter().take(5) {
                let n = seen_owners.entry(&owner.crates_io_login).or_insert(0u32);
                score_sum += (*n).saturating_sub(3) as f64; // authors can have a few crates with no penalty
                weight_sum += 2;
                *n += 2;
            }
            let primary_owner_id = owners.get(0).map(|o| o.crates_io_login.as_str()).unwrap_or("");
            for keyword in keywords.iter().take(5) {
                // obvious keywords are too repetitive and affect innocent crates
                if !top_keywords.contains(keyword.as_str()) {
                    let n = seen_keywords.entry(keyword.clone()).or_insert(0u32);
                    score_sum += (*n).saturating_sub(4) as f64; // keywords are expected to repeat a bit
                    weight_sum += 1;
                    *n += 1;
                }

                // but same owner AND same keyword needs extra bonus for being extra boring
                let n = seen_owner_keywords.entry((primary_owner_id, keyword)).or_insert(0);
                score_sum += *n as f64;
                weight_sum += 2;
                *n += 3;
            }
            // it's average, because new fresh keywords should reduce penalty
            let dupe_points = score_sum / (weight_sum + 10) as f64; // +10 reduces penalty for crates with few authors, few keywords (higher chance of dupe)

            // +7 here allows some duplication, and penalizes harder only after a few crates
            // adding original score means it'll never get lower than 1/3rd
            let new_score = score * 0.5 + (score + 7.) / (7. + dupe_points);
            crates.push((origin.to_owned(), new_score));
        }
        crates.sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
    }

    pub async fn top_keywords_in_category(&self, cat: &Category) -> CResult<Vec<String>> {
        let mut keywords = self.crate_db.top_keywords_in_category(&cat.slug).await?;
        keywords.retain(|k| !cat.obvious_keywords.contains(k));
        keywords.truncate(10);
        Ok(keywords)
    }

    /// true if it's useful as a keyword page
    #[inline]
    pub async fn is_it_a_keyword(&self, k: &str) -> bool {
        self.crate_db.crates_with_keyword(k).await.map(|n| n >= 5).unwrap_or(false)
    }

    /// True if there are multiple crates with that keyword. Populated first.
    pub async fn keywords_populated(&self, krate: &RichCrateVersion) -> Vec<(String, bool)> {
        let mut keywords: Vec<_> = join_all(krate.keywords().iter()
        .map(|k| async move {
            let populated = self.crate_db.crates_with_keyword(&k.to_lowercase()).await.unwrap() >= 3;
            (k.to_owned(), populated)
        })).await;
        keywords.sort_unstable_by_key(|&(_, v)| !v); // populated first; relies on stable sort
        keywords
    }

    #[inline]
    pub async fn recently_updated_crates_in_category(&self, slug: &str) -> CResult<Vec<Origin>> {
        Ok(self.crate_db.recently_updated_crates_in_category(slug).await?)
    }

    /// Case sensitive!
    pub fn rustacean_for_github_login(&self, login: &str) -> Option<Rustacean> {
        if !is_alnum(login) {
            return None;
        }

        let path = self.data_path.join(format!("rustaceans/data/{}.json", login));
        let json = std::fs::read(path).ok()?;
        serde_json::from_slice(&json).ok()
    }

    #[inline]
    pub async fn notable_recently_updated_crates(&self, limit: u32) -> CResult<Vec<(Origin, f64)>> {
        let mut crates = self.crate_db.recently_updated_crates(limit).await?;
        if limit < 750 {
            self.knock_duplicates(&mut crates).await;
        }
        Ok(crates)
    }

    #[inline]
    pub async fn most_downloaded_crates(&self, limit: u32) -> CResult<Vec<(Origin, u32)>> {
        Ok(self.crate_db.most_downloaded_crates(limit).await?)
    }

    /// raw number, despammed weight
    pub async fn category_crate_count(&self, slug: &str) -> Result<(u32, f64), KitchenSinkErr> {
        if slug == "uncategorized" {
            return Ok((300, 0.));
        }
        self.category_crate_counts
            .get_or_init(async {match self.crate_db.category_crate_counts().await {
                Ok(res) => Some(res),
                Err(err) => {
                    error!("error: can't get category counts: {}", err);
                    None
                },
            }})
            .await
            .as_ref()
            .ok_or(KitchenSinkErr::CategoryQueryFailed)
            .and_then(|h| {
                h.get(slug).copied().ok_or_else(|| {
                    KitchenSinkErr::CategoryNotFound(slug.to_string())
                })
            })
    }

    #[inline]
    async fn repo_commits(&self, repo: &SimpleRepo, as_of_version: &str) -> CResult<Option<Vec<github_info::CommitMeta>>> {
        Ok(self.gh.commits(repo, as_of_version).await?)
    }

    /// Prepare for drop: purge buffers, free memory
    pub fn cleanup(&self) {
        let _ = self.crates_io_owners_cache.save();
        let _ = self.depender_changes.save();
        let _ = self.stats_histograms.save();
        let _ = self.url_check_cache.save();
        let _ = self.readme_check_cache.save();
        let _ = self.yearly.save();
        self.crate_rustc_compat_cache.write().clear();
        self.crates_io.cleanup();
        self.loaded_rich_crate_version_cache.write().clear();
        self.index.clear_cache();
    }

    #[inline]
    pub async fn author_by_login(&self, login: &str) -> CResult<RichAuthor> {
        let github = self.user_by_github_login(login).await?.ok_or_else(|| KitchenSinkErr::AuthorNotFound(login.into()))?;
        Ok(RichAuthor { github })
    }
}

/// Any crate can get such owner. Takes crates_io_login with host prefix
fn is_shared_collective_login(login: &str) -> bool {
    login.eq_ignore_ascii_case("rust-bus-owner") || login.eq_ignore_ascii_case("rust-bus") || login.starts_with("github:rust-bus:")
}

/// foo & libfoo-sys && cargo-foo && rust-foo
fn crate_name_namespace_prefix(crate_name: &str) -> &str {
    crate_name
        .trim_start_matches("rust-")
        .trim_start_matches("cargo-")
        .trim_start_matches("lib")
        .split(|c: char| c == '_' || c == '-').find(|&n| n.len() > 1)
        .unwrap_or(crate_name)
}

impl Drop for KitchenSink {
    fn drop(&mut self) {
        self.cleanup()
    }
}

#[derive(Debug, Copy, Clone)]
pub struct VersionPopularity {
    pub pop: f32,
    pub matches_latest: bool,
    pub lost_popularity: bool,
}

#[derive(Debug, Clone)]
pub struct RichAuthor {
    pub github: User,
}

impl RichAuthor {
    pub fn name(&self) -> &str {
        match &self.github.name {
            Some(n) if !n.is_empty() => n,
            _ => &self.github.login,
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct Rustacean {
    pub name: Option<String>,
    /// email address. Will appear in a mailto link.
    pub email: Option<String>,
    /// homepage URL.
    pub website: Option<String>,
    /// URL for your blog.
    pub blog: Option<String>,
    /// username on Discourse.
    pub discourse: Option<String>,
    /// username on Reddit
    pub reddit: Option<String>,
    /// username on Twitter, including the @.
    pub twitter: Option<String>,
    /// any notes you lik
    pub notes: Option<String>,
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
        unattributed_name && (self.name().ends_with(" Developers") || self.name().ends_with(" contributors"))
    }

    pub fn name(&self) -> &str {
        if let Some(name) = self.info.as_ref().and_then(|i| i.name.as_deref()) {
            if !name.trim_start().is_empty() {
                return name;
            }
        }
        if let Some(ref gh) = self.github {
            match &gh.name {
                Some(name) if !name.trim_start().is_empty() => name,
                _ => &gh.login,
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

#[derive(Debug, Clone, Copy, Serialize, Hash, Deserialize, Ord, Eq, PartialEq, PartialOrd)]
pub struct MiniDate {
    /// year
    y: u16,
    /// ordinal (day of year)
    o: u16,
}

impl MiniDate {
    pub fn new(from: Date<Utc>) -> Self {
        Self {
            y: from.year() as u16,
            o: from.ordinal0() as u16,
        }
    }


    /// Screw leap years
    pub fn days_later(self, days: i32) -> Self {
        let n = self.y as i32 * 365 + self.o as i32 + days;
        let y = (n/365) as u16;
        let o = (n%365) as u16;
        Self {
            y, o
        }
    }

    pub fn half_way(self, other: Self) -> Self {
        let diff = (other.y as i32 - self.y as i32) * 365 + (other.o as i32 - self.o as i32);
        self.days_later(diff / 2)
    }
}

#[test]
fn minidate() {
    let a = MiniDate::new(Utc.ymd(2020, 2, 2));
    let b = MiniDate::new(Utc.ymd(2024, 4, 4));
    let c = MiniDate::new(Utc.ymd(2022, 3, 5));
    assert_eq!(c, a.half_way(b));

    let d = MiniDate::new(Utc.ymd(1999, 12, 31));
    let e = MiniDate::new(Utc.ymd(2000, 1, 1));
    assert_eq!(e, d.days_later(1));
    assert_eq!(d, e.days_later(-1));
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DependerChanges {
    pub at: MiniDate,
    pub added: u16,
    /// Crate has released a new version without this dependency
    pub removed: u16,
    /// Crate has this dependnecy, but is not active any more
    pub expired: u16,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CachedCrate {
    pub manifest: Manifest,
    pub derived: Derived,
    pub cache_key: u64,
    #[serde(default)]
    pub warnings: HashSet<Warning>,
}

#[test]
fn is_build_or_dev_test() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(rt.spawn(async move {
        let c = KitchenSink::new_default().await.expect("uhg");
        assert_eq!((false, false), c.is_build_or_dev(&Origin::from_crates_io_name("semver")).await.expect("test1"));
        assert_eq!((false, true), c.is_build_or_dev(&Origin::from_crates_io_name("version-sync")).await.expect("test2"));
        assert_eq!((true, false), c.is_build_or_dev(&Origin::from_crates_io_name("cc")).await.expect("test3"));
    })).unwrap();
}

#[test]
fn fetch_uppercase_name_and_tarball() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(rt.spawn(async move {
        let k = KitchenSink::new_default().await.expect("Test if configured");
        let _ = k.rich_crate_async(&Origin::from_crates_io_name("Inflector")).await.unwrap();
        let _ = k.rich_crate_async(&Origin::from_crates_io_name("inflector")).await.unwrap();


        let testk = k.index.crates_io_crate_by_lowercase_name("dssim-core").unwrap();
        let meta = k.crate_files_summary_from_crates_io_tarball("dssim-core", testk.versions()[8].version()).await.unwrap();
        assert_eq!(meta.path_in_repo.as_deref(), Some("dssim-core"), "{:#?}", meta);
        assert_eq!(meta.vcs_info_git_sha1.as_ref().unwrap(), b"\xba\x0a\x40\xd1\x3b\x1d\x11\xb0\x19\xf6\xb6\x6a\x77\x2e\xbd\xa7\xd0\xf9\x45\x0c");
    })).unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn index_test() {
    let idx = Index::new(&KitchenSink::data_path().unwrap()).unwrap();
    let stats = idx.deps_stats().await.unwrap();
    assert!(stats.total > 13800);
    let lode = stats.counts.get("lodepng").unwrap();
    assert!(lode.runtime.def >= 14 && lode.runtime.def < 50, "{:?}", lode);
}

fn is_alnum(q: &str) -> bool {
    q.as_bytes().iter().copied().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}

struct DropWatch(bool, &'static str);
impl Drop for DropWatch {
    fn drop(&mut self) {
        if !self.0 {
            log::warn!("Aborted: {}", self.1);
        }
    }
}

#[inline(always)]
fn watch<'a, T>(label: &'static str, f: impl Future<Output = T> + Send + 'a) -> Pin<Box<dyn Future<Output = T> + Send + 'a>> {
    debug!("starting: {}", label);
    Box::pin(NonBlock::new(label, async move {
        let mut is_ok = DropWatch(false, label); // await dropping will run this
        tokio::task::yield_now().await;
        let res = f.await;
        is_ok.0 = true;
        res
    }))
}

#[inline(always)]
fn timeout<'a, T, E: From<KitchenSinkErr>>(label: &'static str, time: u16, f: impl Future<Output = Result<T, E>> + Send + 'a) -> Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>> {
    // yield in case something is blocking-busy and would cause a timeout just by starving the runtime
    let f = tokio::task::yield_now()
        .then(move |_| tokio::time::timeout(Duration::from_secs(time.into()), f));
    watch(label, f.map(move |r| r.map_err(|_| {
        info!("Timed out: {} {}", label, time);
        E::from(KitchenSinkErr::TimedOut(label, time))
    }).and_then(|x| x)))
}

#[inline(always)]
fn deadline<'a, T, E: From<KitchenSinkErr>>(label: &'static str, deadline: Instant, f: impl Future<Output = Result<T, E>> + Send + 'a) -> Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>> {
    let time_left = deadline.saturating_duration_since(Instant::now()).as_secs() as u16;
    let f = tokio::time::timeout_at(deadline, f);
    watch(label, f.map(move |r| r.map_err(|_| {
        info!("Timed out: {} {}s", label, time_left);
        E::from(KitchenSinkErr::TimedOut(label, time_left))
    }).and_then(|x| x)))
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum CrateOwners {
    /// Includes guesswork from audit log and rust-bus
    All,
    /// Only actual owners, no guesses, no rust-bus
    Strict,
}

#[test]
fn rustc_rel_dates() {
    static RUST_RELEASE_DATES: [(u16,u8,u8, u16); 71] = [
        (2015,05,15, 0), //.0
        (2015,06,25, 1), //.0
        (2015,08,07, 2), //.0
        (2015,09,17, 3), //.0
        (2015,10,29, 4), //.0
        (2015,12,10, 5), //.0
        (2016,01,21, 6), //.0
        (2016,03,03, 7), //.0
        (2016,04,14, 8), //.0
        (2016,05,26, 9), //.0
        (2016,07,07, 10), //.0
        (2016,08,18, 11), //.0
        (2016,09,29, 12), //.0
        (2016,10,20, 12), //.1
        (2016,11,10, 13), //.0
        (2016,12,22, 14), //.0
        (2017,02,02, 15), //.0
        (2017,02,09, 15), //.1
        (2017,03,16, 16), //.0
        (2017,04,27, 17), //.0
        (2017,06,08, 18), //.0
        (2017,07,20, 19), //.0
        (2017,08,31, 20), //.0
        (2017,10,12, 21), //.0
        (2017,11,22, 22), //.0
        (2017,11,22, 22), //.1
        (2018,01,04, 23), //.0
        (2018,02,15, 24), //.0
        (2018,03,01, 24), //.1
        (2018,03,29, 25), //.0
        (2018,05,10, 26), //.0
        (2018,05,29, 26), //.1
        (2018,06,05, 26), //.2
        (2018,06,21, 27), //.0
        (2018,07,10, 27), //.1
        (2018,07,20, 27), //.2
        (2018,08,02, 28), //.0
        (2018,09,13, 29), //.0
        (2018,09,25, 29), //.1
        (2018,10,11, 29), //.2
        (2018,10,25, 30), //.0
        (2018,11,08, 30), //.1
        (2018,12,06, 31), //.0
        (2018,12,20, 31), //.1
        (2019,01,17, 32), //.0
        (2019,02,28, 33), //.0
        (2019,04,11, 34), //.0
        (2019,04,25, 34), //.1
        (2019,05,14, 34), //.2
        (2019,05,23, 35), //.0
        (2019,07,04, 36), //.0
        (2019,08,15, 37), //.0
        (2019,09,20, 38), //.0
        (2019,11,07, 39), //.0
        (2019,12,19, 40), //.0
        (2020,01,30, 41), //.0
        (2020,02,27, 41), //.1
        (2020,03,12, 42), //.0
        (2020,04,23, 43), //.0
        (2020,05,07, 43), //.1
        (2020,06,04, 44), //.0
        (2020,06,18, 44), //.1
        (2020,07,16, 45), //.0
        (2020,07,30, 45), //.1
        (2020,08,03, 45), //.2
        (2020,08,27, 46), //.0
        (2020,10,08, 47), //.0
        (2020,11,19, 48), //.0
        (2020,12,31, 49), //.0
        (2021,02,11, 50), //.0
        (2021,03,25, 51), //.0
    ];
    for (y,m,d, ver) in RUST_RELEASE_DATES.iter().copied() {
        let date = Utc.ymd(y as _,m as _,d as _).and_hms(0, 0, 0);
        assert_eq!(ver, KitchenSink::rustc_release_from_date(&date).unwrap());
    }
}

fn is_valid_crates_io_category_not_on_lib_rs(slug: &str) -> bool {
    matches!(slug,
    "aerospace::drones" |
    "aerospace::protocols" |
    "aerospace::simulation" |
    "aerospace::space-protocols" |
    "aerospace::unmanned-aerial-vehicles" |
    "aerospace" |
    "api-bindings" |
    "computer-vision" |
    "external-ffi-bindings" |
    "game-engines" |
    "graphics" |
    "localization" |
    "mathematics" |
    "multimedia::encoding" |
    "os::freebsd-apis" |
    "os::linux-apis" |
    "science::robotics")
}
