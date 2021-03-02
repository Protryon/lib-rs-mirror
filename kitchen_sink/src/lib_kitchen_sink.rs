#[macro_use] extern crate failure;

#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate log;

mod yearly;
pub use crate::yearly::*;
pub use deps_index::*;
use tokio::task::spawn_blocking;
pub mod filter;

mod ctrlcbreak;
pub use crate::ctrlcbreak::*;
mod nonblock;
pub use crate::nonblock::*;
mod tarball;

pub use crate_db::builddb::Compat;
pub use crate_db::builddb::CompatibilityInfo;
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
pub use github_info::Org;
pub use github_info::User;
pub use github_info::UserOrg;
pub use github_info::UserType;
pub use rich_crate::DependerChangesMonthly;
pub use rich_crate::Edition;
pub use rich_crate::MaintenanceStatus;
use rich_crate::ManifestExt;
pub use rich_crate::Markup;
pub use rich_crate::Origin;
pub use rich_crate::RichCrate;
pub use rich_crate::RichCrateVersion;
pub use rich_crate::RichDep;
pub use rich_crate::{Cfg, Target};
pub use semver::Version as SemVer;

use crate::tarball::CrateFile;
use cargo_toml::Manifest;
use cargo_toml::Package;
use categories::Category;
use chrono::prelude::*;
use chrono::DateTime;
use crate_db::{builddb::BuildDb, CrateDb, CrateVersionData, RepoChange};
use creviews::Creviews;
use double_checked_cell_async::DoubleCheckedCell;
use failure::ResultExt;
use futures::future::join_all;
use futures::stream::StreamExt;
use futures::Future;
use github_info::GitCommitAuthor;
use github_info::GitHubRepo;
use github_info::MinimalUser;
use itertools::Itertools;
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
use smol_str::SmolStr;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::hash_map::Entry::*;
use std::collections::HashMap;
use std::collections::HashSet;
use std::convert::TryInto;
use std::env;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Mutex;
use std::time::Duration;
use std::time::SystemTime;
use tokio::time::timeout;
use triomphe::Arc;

pub type ArcRichCrateVersion = Arc<RichCrateVersion>;

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
    #[fail(display = "category count not found in crates db: {}", _0)]
    CategoryNotFound(String),
    #[fail(display = "category query failed")]
    CategoryQueryFailed,
    #[fail(display = "crate not found: {:?}", _0)]
    CrateNotFound(Origin),
    #[fail(display = "author not found: {}", _0)]
    AuthorNotFound(String),
    #[fail(display = "crate {} not found in repo {}", _0, _1)]
    CrateNotFoundInRepo(String, String),
    #[fail(display = "crate is not a package: {:?}", _0)]
    NotAPackage(Origin),
    #[fail(display = "data not found, wanted {}", _0)]
    DataNotFound(String),
    #[fail(display = "crate has no versions")]
    NoVersions,
    #[fail(display = "cached data has different version than the index")]
    CacheExpired,
    #[fail(display = "Environment variable CRATES_DATA_DIR is not set.\nChoose a dir where it's OK to store lots of data, and export it like CRATES_DATA_DIR=/var/lib/crates.rs")]
    CratesDataDirEnvVarMissing,
    #[fail(display = "{} does not exist\nPlease get data files from https://lib.rs/data and put them in that directory, or set CRATES_DATA_DIR to their location.", _0)]
    CacheDbMissing(String),
    #[fail(display = "Error when parsing verison")]
    SemverParsingError,
    #[fail(display = "Stopped")]
    Stopped,
    #[fail(display = "Deps stats timeout")]
    DepsNotAvailable,
    #[fail(display = "Crate data timeout")]
    DataTimedOut,
    #[fail(display = "Crate derived cache timeout")]
    DerivedDataTimedOut,
    #[fail(display = "Missing github login for crate owner")]
    OwnerWithoutLogin,
    #[fail(display = "Git index parsing failed: {}", _0)]
    GitIndexParse(String),
    #[fail(display = "Git index {:?}: {}", _0, _1)]
    GitIndexFile(PathBuf, String),
    #[fail(display = "Git crate '{:?}' can't be indexed, because it's not on the list", _0)]
    GitCrateNotAllowed(Origin),
    #[fail(display = "Deps err: {}", _0)]
    Deps(DepsErr),
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
    readme_check_cache: TempCache<()>,
    crate_db: CrateDb,
    user_db: user_db::UserDb,
    gh: github_info::GitHub,
    loaded_rich_crate_version_cache: RwLock<FxHashMap<Origin, ArcRichCrateVersion>>,
    category_crate_counts: DoubleCheckedCell<Option<HashMap<String, u32>>>,
    top_crates_cached: Mutex<FxHashMap<String, Arc<DoubleCheckedCell<Arc<Vec<Origin>>>>>>,
    git_checkout_path: PathBuf,
    main_cache_dir: PathBuf,
    yearly: AllDownloads,
    category_overrides: HashMap<String, Vec<Cow<'static, str>>>,
    crates_io_owners_cache: TempCache<Vec<CrateOwner>>,
    depender_changes: TempCache<Vec<DependerChanges>>,
    throttle: tokio::sync::Semaphore,
    auto_indexing_throttle: tokio::sync::Semaphore,
    crev: Arc<Creviews>,
    data_path: PathBuf,
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
        let data_path = Self::data_path()?;
        Self::new(&data_path, &github_token).await
    }

    pub async fn new(data_path: &Path, github_token: &str) -> CResult<Self> {
        let _ = env_logger::try_init();

        tokio::task::block_in_place(|| {
        let main_cache_dir = data_path.to_owned();

        let ((crates_io, gh), (index, crev)) = rayon::join(|| rayon::join(
                || crates_io_client::CratesIoClient::new(data_path),
                || github_info::GitHub::new(&data_path.join("github.db"), github_token)),
            || rayon::join(
                || Index::new(data_path),
                || Creviews::new(),
            ));
        Ok(Self {
            crev: Arc::new(crev?),
            crates_io: crates_io.context("cratesio")?,
            index: index.context("index")?,
            url_check_cache: TempCache::new(&data_path.join("url_check.db")).context("urlcheck")?,
            readme_check_cache: TempCache::new(&data_path.join("readme_check.db")).context("readmecheck")?,
            docs_rs: docs_rs_client::DocsRsClient::new(data_path.join("docsrs.db")).context("docs")?,
            crate_db: CrateDb::new(Self::assert_exists(data_path.join("crate_data.db"))?).context("db")?,
            user_db: user_db::UserDb::new(Self::assert_exists(data_path.join("users.db"))?).context("udb")?,
            gh: gh.context("gh")?,
            loaded_rich_crate_version_cache: RwLock::new(FxHashMap::default()),
            git_checkout_path: data_path.join("git"),
            category_crate_counts: DoubleCheckedCell::new(),
            top_crates_cached: Mutex::new(FxHashMap::default()),
            yearly: AllDownloads::new(&main_cache_dir),
            main_cache_dir,
            category_overrides: Self::load_category_overrides(&data_path.join("category_overrides.txt"))?,
            crates_io_owners_cache: TempCache::new(&data_path.join("cio-owners.tmp"))?,
            depender_changes: TempCache::new(&data_path.join("deps-changes2.tmp"))?,
            throttle: tokio::sync::Semaphore::new(40),
            auto_indexing_throttle: tokio::sync::Semaphore::new(4),
            data_path: data_path.into(),
        })
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
            let categories: Vec<_> = parts.next().expect("overrides broken").split(',')
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

    #[inline]
    fn summed_year_downloads(&self, crate_name: &str, curr_year: u16) -> CResult<[u32; 366]> {
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
                        if s.rev_dep_names.iter().any(|parent| crates_present.contains(parent)) {
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
            *score *= 0.5 + self.crate_db.crate_rank(origin).await.unwrap_or(0.);
        }
        top.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));

        watch("knock_duplicates", self.knock_duplicates(&mut top)).await;
        top.truncate(top_n);
        top
    }

    // actually gives 2*top_n…
    fn trending_crates_raw(&self, top_n: usize) -> Vec<(Origin, f64)> {
        let now = Utc::today();
        let curr_year = now.year() as u16;
        let day_of_year = now.ordinal0() as usize;
        if day_of_year < 14 {
            return Vec::new(); // December stats are useless anyway
        }
        let shortlen = (day_of_year / 2).min(14);
        let longerlen = (day_of_year / 2).min(6 * 7);

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
                    if prev_week_avg < 80. { // it's too easy to trend from zero downloads!
                        return None;
                    }

                    let this_week_avg = average_nonzero(&d[day_of_year-shortlen .. day_of_year], 8);
                    if prev_week_avg >= this_week_avg {
                        return None;
                    }

                    let prev_4w_avg = average_nonzero(&d[day_of_year-longerlen*2 .. day_of_year-longerlen], 7).max(average_nonzero(&d[.. day_of_year-longerlen*2], 7));
                    let this_4w_avg = average_nonzero(&d[day_of_year-longerlen .. day_of_year], 14);
                    if prev_4w_avg >= this_4w_avg || prev_4w_avg >= prev_week_avg || prev_4w_avg >= this_week_avg {
                        return None;
                    }

                    let ratio1 = (800. + this_week_avg) / (900. + prev_week_avg) * prev_week_avg.sqrt().min(10.);
                    // 0.9, because it's less interesting
                    let ratio4 = 0.9 * (700. + this_4w_avg) / (600. + prev_4w_avg) * prev_4w_avg.sqrt().min(9.);

                    // combine short term and long term trends
                    Some((origin, ratio1, ratio4))
                },
                _ => None,
            }
        }).collect::<Vec<_>>();

        ratios.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal));
        let len = ratios.len();
        let mut top: Vec<_> = ratios.drain(len.saturating_sub(top_n)..).map(|(o, s, _)| (o, s as f64)).collect();
        ratios.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(Ordering::Equal));
        let len = ratios.len();
        top.extend(ratios.drain(len.saturating_sub(top_n)..).map(|(o, _, s)| (o, s as f64)).take(top_n));
        top
    }

    // Monthly downloads, sampled from last few days or weeks
    pub async fn recent_downloads_by_version(&self, origin: &Origin) -> CResult<HashMap<MiniVer, u32>> {

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
            (k.clone(), (v as usize * actual_downloads_per_month / total as usize) as u32)
        ).collect())
    }

    /// Gets cratesio download data, but not from the API, but from our local copy
    pub fn weekly_downloads(&self, k: &RichCrate, num_weeks: u16) -> CResult<Vec<DownloadWeek>> {
        let mut res = Vec::with_capacity(num_weeks.into());
        let mut now = Utc::today();

        let mut curr_year = now.year() as u16;
        let mut summed_days = self.summed_year_downloads(k.name(), curr_year)?;

        let day_of_year = now.ordinal0();
        let missing_data_days = summed_days[0..day_of_year as usize].iter().cloned().rev().take_while(|&s| s == 0).count();

        if missing_data_days > 0 {
            now = now - chrono::Duration::days(missing_data_days as _);
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
                self.rich_crate_async(&Origin::from_crates_io_name(&*name)).await.map_err(|e| error!("{}: {}", name, e)).ok()
            })
            .buffer_unordered(8)
            .filter_map(|x| async {x});
        let mut crates = stream.filter(move |k| {
            let latest = k.versions().iter().map(|v| v.created_at.as_str()).max().unwrap_or("");
            let res = if let Ok(timestamp) = DateTime::parse_from_rfc3339(latest) {
                timestamp.timestamp() >= min_timestamp as i64
            } else {
                error!("Can't parse {} of {}", latest, k.name());
                true
            };
            async move { res }
        })
        .collect::<Vec<_>>().await;

        let mut crates2 = futures::stream::iter(self.crate_db.crates_to_reindex().await?.into_iter())
            .map(move |origin| async move {
                self.rich_crate_async(&origin).await.map_err(|e| {
                    error!("Can't reindex {:?}: {}", origin, e);
                    for e in e.iter_chain() {
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
        if stopped() {Err(KitchenSinkErr::Stopped)?;}
        match origin {
            Origin::CratesIo(name) => {
                let (meta, owners) = futures::try_join!(
                    self.crates_io_meta(name),
                    self.crate_owners(origin),
                )?;
                let versions = meta.versions().map(|c| CrateVersion {
                    num: c.num,
                    updated_at: c.updated_at,
                    created_at: c.created_at,
                    yanked: c.yanked,
                }).collect();
                Ok(RichCrate::new(origin.clone(), owners, meta.krate.name, versions))
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
        let gh = self.gh.repo(repo, &cachebust).await?
            .ok_or_else(|| KitchenSinkErr::CrateNotFound(origin.clone()))
            .context(format!("ghrepo {:?} not found", repo))?;
        let versions = self.get_repo_versions(origin, &host, &cachebust).await?;
        Ok(RichCrate::new(origin.clone(), gh.owner.into_iter().map(|o| {
            CrateOwner {
                avatar: o.avatar_url,
                url: Some(o.html_url),
                login: o.login,
                kind: OwnerKind::User, // FIXME: crates-io uses teams, and we'd need to find the right team? is "owners" a guaranteed thing?
                name: o.name,
                github_id: o.id,

                invited_at: None,
                invited_by_github_id: None,
            }
        }).collect(),
        format!("github/{}/{}", repo.owner, package),
        versions))
    }

    async fn rich_crate_gitlab(&self, origin: &Origin, repo: &SimpleRepo, package: &str) -> CResult<RichCrate> {
        let host = RepoHost::GitLab(repo.clone()).try_into().map_err(|_| KitchenSinkErr::CrateNotFound(origin.clone())).context("ghrepo host bad")?;
        let cachebust = self.cachebust_string_for_repo(&host).await.context("ghrepo")?;
        let versions = self.get_repo_versions(origin, &host, &cachebust).await?;
        Ok(RichCrate::new(origin.clone(), vec![], format!("gitlab/{}/{}", repo.owner, package), versions))
    }

    async fn get_repo_versions(&self, origin: &Origin, repo: &Repo, cachebust: &str) -> CResult<Vec<CrateVersion>> {
        let package = match origin {
            Origin::GitLab { package, .. } => package,
            Origin::GitHub { repo, package } => {
                let releases = self.gh.releases(repo, cachebust).await?.ok_or_else(|| KitchenSinkErr::CrateNotFound(origin.clone())).context("releases not found")?;
                let versions: Vec<_> = releases.into_iter().filter_map(|r| {
                    let date = r.published_at.or(r.created_at)?;
                    let num_full = r.tag_name?;
                    let num = num_full.trim_start_matches(|c:char| !c.is_numeric());
                    // verify that it semver-parses
                    let _ = SemVer::parse(num).map_err(|e| warn!("{:?}: ignoring {}, {}", origin, num_full, e)).ok()?;
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

        let versions: Vec<_> = self.crate_db.crate_versions(origin).await?.into_iter().map(|(num, timestamp)| {
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
        let _f = self.throttle.acquire().await;
        info!("Need to scan repo {:?}", repo);
        let git_checkout_path = self.git_checkout_path.clone();
        let origin = origin.clone();
        let repo = repo.clone();
        let package = package.clone();
        spawn_blocking(move || {
            let checkout = crate_git_checkout::checkout(&repo, &git_checkout_path)?;
            let mut pkg_ver = crate_git_checkout::find_versions(&checkout)?;
            if let Some(v) = pkg_ver.remove(&*package) {
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
        }).await?
    }

    #[inline]
    pub async fn downloads_per_month(&self, origin: &Origin) -> CResult<Option<usize>> {
        self.downloads_recent_90_days(origin).await.map(|dl| dl.map(|n| n / 3))
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

    #[inline]
    async fn downloads_recent_90_days(&self, origin: &Origin) -> CResult<Option<usize>> {
        Ok(match origin {
            Origin::CratesIo(name) => {
                let meta = self.crates_io_meta(name).await?;
                meta.krate.recent_downloads
            },
            _ => None,
        })
    }

    async fn crates_io_meta(&self, name: &str) -> CResult<CrateMetaFile> {
        let krate = tokio::task::block_in_place(|| {
            self.index.crates_io_crate_by_lowercase_name(name).context("rich_crate")
        })?;
        let latest_in_index = krate.latest_version().version(); // most recently published version
        let meta = self.crates_io.crate_meta(name, latest_in_index).await
            .with_context(|_| format!("crates.io meta for {} {}", name, latest_in_index))?;
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
        watch("rcv-1", self.rich_crate_version_async_opt(origin, false))
    }

    /// Same as rich_crate_version_async, but it won't try to refresh the data. Just fails if there's no cached data.
    pub fn rich_crate_version_stale_is_ok<'a>(&'a self, origin: &'a Origin) -> Pin<Box<dyn Future<Output = CResult<ArcRichCrateVersion>> + Send + 'a>> {
        watch("rcv-2", self.rich_crate_version_async_opt(origin, true))
    }

    async fn rich_crate_version_async_opt(&self, origin: &Origin, allow_stale: bool) -> CResult<ArcRichCrateVersion> {
        if stopped() {Err(KitchenSinkErr::Stopped)?;}

        if let Some(krate) = self.loaded_rich_crate_version_cache.read().get(origin) {
            trace!("rich_crate_version_async HIT {:?}", origin);
            return Ok(krate.clone());
        }
        trace!("rich_crate_version_async MISS {:?}", origin);

        let mut maybe_data = timeout(Duration::from_secs(3), self.crate_db.rich_crate_version_data(origin))
            .await.map_err(|_| {
                warn!("db data fetch for {:?} timed out", origin);
                KitchenSinkErr::DerivedDataTimedOut
            })?;

        if let Ok(cached) = &maybe_data {
            match origin {
                Origin::CratesIo(name) => {
                    if !allow_stale {
                        let expected_cache_key = self.index.cache_key_for_crate(name).context("error finding crates-io index data")?;
                        if expected_cache_key != cached.cache_key {
                            info!("Ignoring derived cache of {}, because it changed", name);
                            maybe_data = Err(KitchenSinkErr::CacheExpired.into());
                        }
                    }
                },
                _ => {}, // TODO: figure out when to invalidate cache of git-repo crates
            }
        }

        let data = match maybe_data {
            Ok(data) => data,
            Err(e) => {
                if allow_stale {
                    return Err(e);
                }
                debug!("Getting/indexing {:?}: {}", origin, e);
                let _th = timeout(Duration::from_secs(30), self.auto_indexing_throttle.acquire()).await?;
                let reindex = timeout(Duration::from_secs(60), self.index_crate_highest_version(origin));
                watch("reindex", reindex).await.map_err(|_| KitchenSinkErr::DataTimedOut)?.with_context(|_| format!("reindexing {:?}", origin))?; // Pin to lower stack usage
                let get_data = timeout(Duration::from_secs(10), self.crate_db.rich_crate_version_data(origin));
                match watch("get_data", get_data)
                    .await
                    .map_err(|_| {
                        warn!("rich_crate_version_data timeout");
                        KitchenSinkErr::DerivedDataTimedOut
                    })
                    .context("getting data after reindex")?
                {
                    Ok(v) => v,
                    Err(e) => return Err(e),
                }
            },
        };

        let krate = Arc::new(RichCrateVersion::new(origin.clone(), data.manifest, data.derived));
        if !allow_stale {
            let mut cache = self.loaded_rich_crate_version_cache.write();
            if cache.len() > 4000 {
                cache.clear();
            }
            cache.insert(origin.clone(), krate.clone());
        }
        Ok(krate)
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

    async fn package_in_repo_host(&self, origin: Origin) -> CResult<(CrateFile, String)> {
        let (repo, package): (Repo, _) = match &origin {
            Origin::GitHub { repo, package } => (RepoHost::GitHub(repo.clone()).try_into().expect("repohost"), package.clone()),
            Origin::GitLab { repo, package } => (RepoHost::GitLab(repo.clone()).try_into().expect("repohost"), package.clone()),
            _ => unreachable!(),
        };
        let git_checkout_path = self.git_checkout_path.clone();

        tokio::task::spawn(timeout(Duration::from_secs(600), async move {
            let checkout = crate_git_checkout::checkout(&repo, &git_checkout_path)?;
            let (path_in_repo, tree_id, manifest) = crate_git_checkout::path_in_repo(&checkout, &package)?
                .ok_or_else(|| {
                    let (has, err) = crate_git_checkout::find_manifests(&checkout).unwrap_or_default();
                    for e in err {
                        warn!("parse err: {}", e.0);
                    }
                    for h in has {
                        info!("has: {} -> {}", h.0, h.2.package.as_ref().map(|p| p.name.as_str()).unwrap_or("?"));
                    }
                    KitchenSinkErr::CrateNotFoundInRepo(package.to_string(), repo.canonical_git_url().into_owned())
                })?;


            let mut meta = tarball::read_repo(&checkout, tree_id)?;
            debug_assert_eq!(meta.manifest.package, manifest.package);
            let package = meta.manifest.package.as_mut().ok_or_else(|| KitchenSinkErr::NotAPackage(origin))?;

            // Allowing any other URL would allow spoofing
            package.repository = Some(repo.canonical_git_url().into_owned());
            Ok::<_, CError>((meta, path_in_repo))
        })).await?.map_err(|_| KitchenSinkErr::GitCheckoutFailed)?
    }

    async fn rich_crate_version_from_repo(&self, origin: &Origin) -> CResult<(CrateVersionSourceData, Manifest, Warnings)> {

        tokio::task::yield_now().await;
        let _f = self.throttle.acquire().await;
        let (mut meta, path_in_repo) = self.package_in_repo_host(origin.clone()).await?;

        let package = meta.manifest.package.as_mut().ok_or_else(|| KitchenSinkErr::NotAPackage(origin.clone()))?;
        let mut warnings = HashSet::new();
        let has_readme = meta.readme.is_some();
        if !has_readme {
            let maybe_repo = package.repository.as_ref().and_then(|r| Repo::new(r).ok());
            warnings.insert(Warning::NoReadmeProperty);
            warnings.extend(self.add_readme_from_repo(&mut meta, maybe_repo.as_ref()));
        }
        self.rich_crate_version_data_common(origin.clone(), meta, 0, false, path_in_repo, warnings).await
    }

    async fn tarball(&self, name: &str, ver: &str) -> Result<Vec<u8>, KitchenSinkErr> {
        self.crates_io.crate_data(name, ver).await
            .map_err(|e| KitchenSinkErr::DataNotFound(format!("{}-{}: {}", name, ver, e)))
    }

    async fn rich_crate_version_data_from_crates_io(&self, latest: &CratesIndexVersion) -> CResult<(CrateVersionSourceData, Manifest, Warnings)> {
        let _f = self.throttle.acquire().await;

        let mut warnings = HashSet::new();

        let name = latest.name();
        let name_lower = name.to_ascii_lowercase();
        let ver = latest.version();
        let origin = Origin::from_crates_io_name(name);

        let (crate_tarball, crates_io_meta) = futures::join!(
            self.tarball(name, ver),
            self.crates_io_meta(&name_lower));

        let crates_io_krate = crates_io_meta?.krate;
        let crate_tarball = crate_tarball?;
        let crate_compressed_size = crate_tarball.len();
        let mut meta = spawn_blocking({
            let name = name.to_owned();
            let ver = ver.to_owned();
            move || {
                crate::tarball::read_archive(&crate_tarball[..], &name, &ver)
            }
        }).await??;

        let package = meta.manifest.package.as_mut().ok_or_else(|| KitchenSinkErr::NotAPackage(origin.clone()))?;

        // it may contain data from "nowhere"! https://github.com/rust-lang/crates.io/issues/1624
        if package.homepage.is_none() {
            if let Some(url) = crates_io_krate.homepage {
                package.homepage = Some(url);
            }
        }
        if package.documentation.is_none() {
            if let Some(url) = crates_io_krate.documentation {
                package.documentation = Some(url);
            }
        }

        // Guess repo URL if none was specified; must be done before getting stuff from the repo
        if package.repository.is_none() {
            warnings.insert(Warning::NoRepositoryProperty);
            // it may contain data from nowhere! https://github.com/rust-lang/crates.io/issues/1624
            if let Some(repo) = crates_io_krate.repository {
                package.repository = Some(repo);
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
                warnings.extend(self.add_readme_from_repo(&mut meta, maybe_repo.as_ref()));
            }
        }

        let path_in_repo = match maybe_repo.as_ref() {
            Some(r) => self.crate_db.path_in_repo(r, name).await?,
            None => None,
        }.unwrap_or_default();

        self.rich_crate_version_data_common(origin, meta, crate_compressed_size as u32, latest.is_yanked(), path_in_repo, warnings).await
    }

    ///// Fixing and faking the data
    async fn rich_crate_version_data_common(&self, origin: Origin, mut meta: CrateFile, crate_compressed_size: u32, is_yanked: bool, path_in_repo: String, mut warnings: Warnings) -> CResult<(CrateVersionSourceData, Manifest, Warnings)> {
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
            }
            topics.retain(|t| match t.as_str() {
                "rust" | "rs" | "rustlang" | "rust-lang" | "crate" | "crates" | "library" => false,
                _ => true,
            });
            if !topics.is_empty() {
                github_keywords = Some(topics);
            }
        }

        if origin.is_crates_io() {
            // Delete the original docs.rs link, because we have our own
            // TODO: what if the link was to another crate or a subpage?
            if package.documentation.as_ref().map_or(false, |s| Self::is_docs_rs_link(s)) {
                if self.has_docs_rs(&origin, &package.name, &package.version).await {
                    package.documentation = None; // docs.rs is not proper docs
                }
            }
        }

        warnings.extend(self.remove_redundant_links(package, maybe_repo.as_ref()).await);

        let mut github_description = None;
        let mut github_name = None;
        if let Some(ref crate_repo) = maybe_repo {
            if let Some(ghrepo) = self.github_repo(crate_repo).await? {
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
                words.push(&lib);
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

        let readme = meta.readme.map(|(readme_path, markup)| {
            let (base_url, base_image_url) = match maybe_repo {
                Some(repo) => {
                    // Not parsing github URL, because "aboslute" path should not be allowed to escape the repo path,
                    // but it needs to normalize ../readme paths
                    let url = url::Url::parse(&format!("http://localhost/{}", path_in_repo)).and_then(|u| u.join(&readme_path));
                    let in_repo_url_path = url.as_ref().map_or("", |u| u.path().trim_start_matches('/'));
                    (Some(repo.readme_base_url(in_repo_url_path)), Some(repo.readme_base_image_url(in_repo_url_path)))
                },
                None => (None, None),
            };
            Readme {
                markup,
                base_url,
                base_image_url,
            }
        });

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
        let eq = |a: &str, b: &str| -> bool { a.eq_ignore_ascii_case(b) };

        for cat in &mut package.categories {
            if cat.as_bytes().iter().any(|c| c.is_ascii_uppercase()) {
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
                if package.keywords.iter().any(|k| eq(k, "bitcoin") || eq(k, "ethereum") || eq(k, "ledger") || eq(k, "exonum") || eq(k, "blockchain")) {
                    *cat = "cryptography::cryptocurrencies".into();
                }
            }
            // crates-io added a better category
            if cat == "game-engines" {
                *cat = "game-development".to_string();
            }
            if cat == "games" {
                if package.keywords.iter().any(|k| {
                    k == "game-dev" || k == "game-development" || eq(k,"gamedev") || eq(k,"framework") || eq(k,"utilities") || eq(k,"parser") || eq(k,"api")
                }) {
                    *cat = "game-development".into();
                }
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

    async fn github_repo(&self, crate_repo: &Repo) -> CResult<Option<GitHubRepo>> {
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
                .and_then(|checkout| crate_git_checkout::find_readme(&checkout, package));
            match res {
                Ok(Some(mut readme)) => {
                    // Make the path absolute, because the readme is now absolute relative to repo root,
                    // rather than relative to crate's dir within the repo
                    if !readme.0.starts_with('/') {
                        readme.0.insert(0, '/');
                    }
                    meta.readme = Some(readme);
                },
                Ok(None) => {
                    warnings.insert(Warning::NoReadmeInRepo(repo.canonical_git_url().to_string()));
                },
                Err(err) => {
                    warnings.insert(Warning::ErrorCloning(repo.canonical_git_url().to_string()));
                    error!("Checkout of {} ({}) failed: {}", package.name, repo.canonical_git_url(), err);
                },
            }
        }
        warnings
    }

    async fn add_readme_from_crates_io(&self, meta: &mut CrateFile, name: &str, ver: &str) {
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

        if let Some(url) = package.homepage.as_ref() {
            if !self.check_url_is_valid(url).await {
                warnings.insert(Warning::BrokenLink("homepage".to_string(), package.homepage.as_ref().unwrap().to_string()));
                package.homepage = None;
            }
        }

        if let Some(url) = package.documentation.as_ref() {
            if !self.check_url_is_valid(url).await {
                warnings.insert(Warning::BrokenLink("documentation".to_string(), package.documentation.as_ref().unwrap().to_string()));
                package.documentation = None;
            }
        }
        warnings
    }

    async fn check_url_is_valid(&self, url: &str) -> bool {
        if let Ok(Some(res)) = self.url_check_cache.get(url) {
            return res;
        }
        watch("urlchk", async {
            debug!("CHK: {}", url);
            let req = reqwest::Client::builder().build().unwrap();
            let res = req.get(url).send().await.map(|res| {
                res.status().is_success()
            })
            .unwrap_or(false);
            self.url_check_cache.set(url, res).unwrap();
            res
        }).await
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
                self.index.all_dependencies_flattened(self.index.crates_io_crate_by_lowercase_name(name).map_err(KitchenSinkErr::Deps)?).map_err(KitchenSinkErr::Deps)
            },
            _ => self.index.all_dependencies_flattened(krate).map_err(KitchenSinkErr::Deps),
        }
    }

    #[inline]
    pub async fn prewarm(&self) {
        let _ = self.index.deps_stats().await;
    }

    pub async fn update(&self) {
        let crev = self.crev.clone();
        rayon::spawn(move || {
            let _ = crev.update().map_err(|e| debug!("crev update: {}", e));
        });
        self.index.update().await;
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

    /// (latest, pop)
    /// 0 = not used
    /// 1 = everyone uses it
    #[inline]
    pub async fn version_popularity(&self, crate_name: &str, requirement: &VersionReq) -> CResult<Option<(bool, f32)>> {
        let mut res = self.index.version_popularity(crate_name, requirement).await.map_err(KitchenSinkErr::Deps)?;
        if let Some((ref mut matches_latest, ref mut pop)) = &mut res {
            if let Some((former_glory, _)) = self.former_glory(&Origin::from_crates_io_name(crate_name)).await? {
                if former_glory < 0.5 {
                    *matches_latest = false;
                }
                *pop *= former_glory as f32;
            }
        }
        Ok(res)
    }

    /// "See also"
    #[inline]
    pub async fn related_categories(&self, slug: &str) -> CResult<Vec<String>> {
        self.crate_db.related_categories(slug).await
    }

    /// Recommendations
    pub async fn related_crates(&self, krate: &RichCrateVersion, min_recent_downloads: u32) -> CResult<Vec<Origin>> {
        let (replacements, related) = futures::try_join!(
            self.crate_db.replacement_crates(krate.short_name()),
            self.crate_db.related_crates(krate.origin(), min_recent_downloads),
        )?;

        Ok(replacements.into_iter()
            .chain(related)
            .unique()
            .take(10)
            .collect())

    }

    /// Returns (nth, slug)
    pub async fn top_category(&self, krate: &RichCrateVersion) -> Option<(u32, String)> {
        let crate_origin = krate.origin();
        let cats = join_all(krate.category_slugs().map(|slug| slug.into_owned()).map(|slug| async move {
            let c = timeout(Duration::from_secs(6), self.top_crates_in_category(&slug)).await??;
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
        Ok(self.crate_db.top_keyword(&krate.origin()).await?)
    }

    /// Maintenance: add user to local db index
    pub(crate) async fn index_user_m(&self, user: &MinimalUser, commit: &GitCommitAuthor) -> CResult<()> {
        if stopped() {Err(KitchenSinkErr::Stopped)?;}
        let user = self.gh.user_by_login(&user.login).await?.ok_or_else(|| KitchenSinkErr::AuthorNotFound(user.login.clone()))?;
        if !self.user_db.email_has_github(&commit.email)? {
            println!("{} => {}", commit.email, user.login);
            self.user_db.index_user(&user, Some(&commit.email), commit.name.as_deref())?;
        }
        Ok(())
    }

    /// Maintenance: add user to local db index
    pub fn index_user(&self, user: &User, commit: &GitCommitAuthor) -> CResult<()> {
        if stopped() {Err(KitchenSinkErr::Stopped)?;}
        if !self.user_db.email_has_github(&commit.email)? {
            println!("{} => {}", commit.email, user.login);
            self.user_db.index_user(&user, Some(&commit.email), commit.name.as_deref())?;
        }
        Ok(())
    }

    /// Maintenance: add user to local db index
    pub async fn index_email(&self, email: &str, name: Option<&str>) -> CResult<()> {
        if stopped() {Err(KitchenSinkErr::Stopped)?;}
        if !self.user_db.email_has_github(&email)? {
            match self.gh.user_by_email(&email).await {
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
        if stopped() {Err(KitchenSinkErr::Stopped)?;}
        self.crate_db.index_versions(k, score, self.downloads_per_month(k.origin()).await?).await?;
        Ok(())
    }

    pub fn index_crate_downloads(&self, crates_io_name: &str, by_ver: &HashMap<&str, &[(Date<Utc>, u32)]>) -> CResult<()> {
        if stopped() {Err(KitchenSinkErr::Stopped)?;}
        let mut year_data = HashMap::new();
        for (version, date_dls) in by_ver {
            let version = MiniVer::from(semver::Version::parse(version)?);
            for (day, dls) in date_dls.iter() {
                let curr_year = day.year() as u16;
                let mut curr_year_data = match year_data.entry(curr_year) {
                    Vacant(e) => {
                        e.insert((false, self.yearly.get_crate_year(crates_io_name, curr_year)?.unwrap_or_default()))
                    },
                    Occupied(e) => e.into_mut(),
                };

                let day_of_year = day.ordinal0() as usize;
                let year_dls = curr_year_data.1.entry(version.clone()).or_insert_with(Default::default);
                if year_dls.0[day_of_year] < *dls {
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

    pub async fn index_crate_highest_version(&self, origin: &Origin) -> CResult<()> {
        if stopped() {Err(KitchenSinkErr::Stopped)?;}
        debug!("Indexing {:?}", origin);

        self.crate_db.before_index_latest(origin).await?;

        let ((source_data, manifest, _warn), cache_key) = match origin {
            Origin::CratesIo(ref name) => {
                let cache_key = self.index.cache_key_for_crate(name)?;
                let ver = self.index.crate_highest_version(name, false).context("rich_crate_version2")?;
                let res = watch("rcv-3", self.rich_crate_version_data_from_crates_io(ver)).await.context("rich_crate_version_data_from_crates_io")?;
                (res, cache_key)
            },
            Origin::GitHub { .. } | Origin::GitLab { .. } => {
                if !self.crate_exists(origin) {
                    Err(KitchenSinkErr::GitCrateNotAllowed(origin.to_owned()))?
                }
                let res = watch("rcv-4", self.rich_crate_version_from_repo(&origin)).await?;
                (res, 0)
            },
        };

        // direct deps are used as extra keywords for similarity matching,
        // but we're taking only niche deps to group similar niche crates together
        let raw_deps_stats = self.index.deps_stats().await?;
        let mut weighed_deps = Vec::<(&str, f32)>::new();
        let all_deps = manifest.direct_dependencies();
        let all_deps = [(all_deps.0, 1.0), (all_deps.2, 0.33)];
        // runtime and (lesser) build-time deps
        for (deps, overall_weight) in all_deps.iter() {
            for dep in deps {
                if let Some(rev) = raw_deps_stats.counts.get(&*dep.package) {
                    let right_popularity = rev.direct.all() > 1 && rev.direct.all() < 150 && rev.runtime.def < 500 && rev.runtime.opt < 800;
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

        let tmp;
        let category_slugs = if let Some(overrides) = self.category_overrides.get(origin.short_crate_name()) {
            &overrides
        } else {
            let mut warnings = Vec::new();
            tmp = categories::Categories::fixed_category_slugs(&package.categories, &mut warnings);
            if !warnings.is_empty() {
                warn!("{}: {}", package.name, warnings.join("; "));
            }
            &tmp
        };

        let extracted_auto_keywords = feat_extractor::auto_keywords(&manifest, source_data.github_description.as_deref(), readme_text.as_deref());

        self.crate_db.index_latest(CrateVersionData {
            cache_key,
            category_slugs,
            authors: &authors,
            origin,
            repository: repository.as_ref(),
            deps_stats: &weighed_deps,
            is_build, is_dev,
            manifest: &manifest,
            source_data: &source_data,
            extracted_auto_keywords,
        }).await?;
        Ok(())
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
        } else {
            if can_capitalize {
                first_capital
            } else {
                name.to_owned()
            }
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

    pub async fn index_repo(&self, repo: &Repo, as_of_version: &str) -> CResult<()> {
        let _f = self.throttle.acquire().await;
        if stopped() {Err(KitchenSinkErr::Stopped)?;}
        let (checkout, manif) = tokio::task::spawn_blocking({
            let git_checkout_path = self.git_checkout_path.clone();
            let repo = repo.clone();
            move || {
            let url = repo.canonical_git_url();
            let checkout = crate_git_checkout::checkout(&repo, &git_checkout_path)?;

            let (manif, warnings) = crate_git_checkout::find_manifests(&checkout)
                .with_context(|_| format!("find manifests in {}", url))?;
            for warn in warnings {
                warn!("warning: {}", warn.0);
            }
            Ok::<_, CError>((checkout, manif.into_iter().filter_map(|(subpath, _, manifest)| {
                manifest.package.map(|p| (subpath, p.name))
            })))
        }}).await??;
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

        if stopped() {Err(KitchenSinkErr::Stopped)?;}

        let mut changes = Vec::new();
        tokio::task::yield_now().await;
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
    pub fn user_by_email(&self, email: &str) -> CResult<Option<User>> {
        Ok(self.user_db.user_by_email(email).context("user_by_email")?)
    }

    pub async fn user_by_github_login(&self, github_login: &str) -> CResult<Option<User>> {
        if let Some(gh) = self.user_db.user_by_github_login(github_login)? {
            if gh.name.is_some() {
                return Ok(Some(gh));
            }
        }
        debug!("gh user cache miss {}", github_login);
        Ok(Box::pin(self.gh.user_by_login(github_login)).await?) // errs on 404
    }

    pub fn rustc_compatibility(&self, origin: &Origin) -> CResult<Vec<CompatibilityInfo>> {
        let db = BuildDb::new(self.main_cache_dir().join("builds.db"))?;
        Ok(db.get_compat(origin)?)
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
    pub async fn parent_crate(&self, child: &RichCrateVersion) -> Option<Origin> {
        if !child.has_path_in_repo() {
            return None;
        }
        let repo = child.repository()?;
        self.crate_db.parent_crate(repo, child.short_name()).await.ok()?
    }

    /// Crates are spilt into foo and foo-core. The core is usually uninteresting/duplicate.
    pub async fn is_sub_component(&self, k: &RichCrateVersion) -> bool {
        let name = k.short_name();
        if let Some(pos) = name.rfind(|c: char| c == '-' || c == '_') {
            match name.get(pos+1..) {
                Some("core") | Some("shared") | Some("utils") | Some("common") |
                Some("impl") | Some("fork") | Some("unofficial") => {
                    if let Some(parent_name) = name.get(..pos-1) {
                        if Origin::try_from_crates_io_name(parent_name).map_or(false, |name| self.crate_exists(&name)) {
                            // TODO: check if owners overlap?
                            return true;
                        }
                    }
                    if self.parent_crate(k).await.is_some() {
                        return true;
                    }
                },
                _ => {},
            }
        }
        false
    }

    async fn cachebust_string_for_repo(&self, crate_repo: &Repo) -> CResult<String> {
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
                if !found_crate_in_repo && !owners.iter().any(|owner| owner.login.eq_ignore_ascii_case(&repo.owner)) {
                    return Ok((false, HashMap::new()));
                }

                // multiple crates share a repo, which causes cache churn when version "changes"
                // so pick one of them and track just that one version
                let cachebust = self.cachebust_string_for_repo(crate_repo).await.context("contrib")?;
                debug!("getting contributors for {:?}", repo);
                let contributors = match timeout(Duration::from_secs(10), self.gh.contributors(repo, &cachebust)).await {
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
                                w.commits as f64 +
                                ((w.added + w.deleted*2) as f64).sqrt()
                            }).sum::<f64>();
                        use std::collections::hash_map::Entry;
                        match by_login.entry(author.login.to_ascii_lowercase()) {
                            Entry::Vacant(e) => {
                                if let Ok(Some(user)) = self.gh.user_by_login(&author.login).await {
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

    /// Merge authors, owners, contributors
    pub async fn all_contributors<'a>(&self, krate: &'a RichCrateVersion) -> CResult<(Vec<CrateAuthor<'a>>, Vec<CrateAuthor<'a>>, bool, usize)> {
        let owners = self.crate_owners(krate.origin()).await?;

        let (hit_max_contributor_count, mut contributors_by_login) = match krate.repository().as_ref() {
            // Only get contributors from github if the crate has been found in the repo,
            // otherwise someone else's repo URL can be used to get fake contributor numbers
            Some(crate_repo) => watch("contrib", self.contributors_from_repo(crate_repo, &owners, krate.has_path_in_repo())).await?,
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
                        let login = url[gh_url.len()..].splitn(1, '/').next().expect("can't happen");
                        if let Ok(Some(gh)) = self.gh.user_by_login(login).await {
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
                        e.owner = true;
                        if e.info.is_none() {
                            e.info = Some(Cow::Owned(Author{
                                name: Some(owner.name().to_owned()),
                                email: None,
                                url: owner.url.clone(),
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
                                url: owner.url.clone(),
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


        authors.sort_by(|a, b| {
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

        let owners_partial = authors.iter().any(|a| a.owner);
        Ok((authors, owners, owners_partial, if hit_max_contributor_count { 100 } else { contributors }))
    }

    #[inline]
    async fn owners_github(&self, owner: &CrateOwner) -> CResult<User> {
        // This is a bit weak, since logins are not permanent
        if let Some(user) = self.gh.user_by_login(owner.github_login().ok_or(KitchenSinkErr::OwnerWithoutLogin)?).await? {
            return Ok(user);
        }
        Err(KitchenSinkErr::OwnerWithoutLogin)?
    }

    #[inline]
    pub async fn crates_of_author(&self, aut: &RichAuthor) -> CResult<Vec<CrateOwnerRow>> {
        self.crate_db.crates_of_author(aut.github.id).await
    }

    pub async fn crate_owners(&self, origin: &Origin) -> CResult<Vec<CrateOwner>> {
        match origin {
            Origin::CratesIo(name) => {
                if let Some(o) = self.crates_io_owners_cache.get(name)? {
                    return Ok(o);
                }
                Ok(Box::pin(self.crates_io.crate_owners(name, "fallback")).await?.unwrap_or_default())
            },
            Origin::GitLab {..} => Ok(vec![]),
            Origin::GitHub {repo, ..} => Ok(vec![
                CrateOwner {
                    avatar: None,
                    // FIXME: read from GH
                    url: Some(format!("https://github.com/{}", repo.owner)),
                    // FIXME: read from GH
                    login: repo.owner.to_string(),
                    kind: OwnerKind::User, // FIXME: crates-io uses teams, and we'd need to find the right team? is "owners" a guaranteed thing?
                    name: None,

                    invited_at: None,
                    github_id: None,
                    invited_by_github_id: None,
                }
            ]),
        }
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
        self.crate_db.index_crate_all_owners(&all_owners).await?;
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
                let total_count = self.category_crate_count(slug).await?;
                let wanted_num = ((total_count / 2 + 25) / 50 * 50).max(100);

                let mut crates = if slug == "uncategorized" {
                    self.crate_db.top_crates_uncategorized(wanted_num + 50).await?
                } else {
                    self.crate_db.top_crates_in_category_partially_ranked(slug, wanted_num + 50).await?
                };
                self.knock_duplicates(&mut crates).await;
                let crates: Vec<_> = crates.into_iter().map(|(o, _)| o).take(wanted_num as usize).collect();
                Ok::<_, failure::Error>(Arc::new(crates))
            }).await
        }).await?;
        Ok(Arc::clone(res))
    }

    /// To make categories more varied, lower score of crates by same authors, with same keywords
    async fn knock_duplicates(&self, crates: &mut Vec<(Origin, f64)>) {
        let with_owners = futures::stream::iter(crates.drain(..))
        .map(|(o, score)| async move {
            let get_crate = timeout(Duration::from_secs(1), self.rich_crate_version_stale_is_ok(&o));
            let (k, owners) = futures::join!(get_crate, self.crate_owners(&o));
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
                    for e in e.iter_chain() {
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

        let mut top_keywords = HashMap::new();
        for (_, _, _, keywords) in &with_owners {
            for k in keywords {
                *top_keywords.entry(k).or_insert(0u32) += 1;
            }
        }
        let mut top_keywords: Vec<_> = top_keywords.into_iter().collect();
        top_keywords.sort_by(|a, b| b.1.cmp(&a.1));
        let top_keywords: HashSet<_> = top_keywords.iter().copied().take((top_keywords.len() / 10).min(10).max(2)).map(|(k, _)| k.to_string()).collect();

        crates.clear();
        let mut seen_owners = HashMap::new();
        let mut seen_keywords = HashMap::new();
        let mut seen_owner_keywords = HashMap::new();
        for (origin, score, owners, keywords) in &with_owners {
            let mut weight_sum = 0;
            let mut score_sum = 0.0;
            for owner in owners.iter().take(5) {
                let n = seen_owners.entry(&owner.login).or_insert(0u32);
                score_sum += (*n).saturating_sub(3) as f64; // authors can have a few crates with no penalty
                weight_sum += 2;
                *n += 2;
            }
            let primary_owner_id = owners.get(0).map(|o| o.login.as_str()).unwrap_or("");
            for keyword in keywords.into_iter().take(5) {
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
        crates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
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
        keywords.sort_by_key(|&(_, v)| !v); // populated first; relies on stable sort
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
        self.knock_duplicates(&mut crates).await;
        Ok(crates)
    }

    pub async fn category_crate_count(&self, slug: &str) -> Result<u32, KitchenSinkErr> {
        if slug == "uncategorized" {
            return Ok(300);
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
        self.index.clear_cache();
        let _ = self.crates_io_owners_cache.save();
        let _ = self.depender_changes.save();
        let _ = self.url_check_cache.save();
        let _ = self.readme_check_cache.save();
        self.loaded_rich_crate_version_cache.write().clear();
        self.crates_io.cleanup();
    }

    #[inline]
    pub async fn author_by_login(&self, login: &str) -> CResult<RichAuthor> {
        let github = self.gh.user_by_login(login).await?.ok_or_else(|| KitchenSinkErr::AuthorNotFound(login.to_owned()))?;
        Ok(RichAuthor { github })
    }
}

#[derive(Debug, Clone)]
pub struct RichAuthor {
    pub github: User,
}

impl RichAuthor {
    pub fn name(&self) -> &str {
        match &self.github.name {
            Some(n) if !n.is_empty() => &n,
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

#[test]
fn is_build_or_dev_test() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(rt.spawn(async move {
        let c = KitchenSink::new_default().await.expect("uhg");
        assert_eq!((false, false), c.is_build_or_dev(&Origin::from_crates_io_name("semver")).await.unwrap());
        assert_eq!((false, true), c.is_build_or_dev(&Origin::from_crates_io_name("version-sync")).await.unwrap());
        assert_eq!((true, false), c.is_build_or_dev(&Origin::from_crates_io_name("cc")).await.unwrap());
    })).unwrap();
}

#[test]
fn fetch_uppercase_name() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(rt.spawn(async move {
        let k = KitchenSink::new_default().await.expect("Test if configured");
        let _ = k.rich_crate_async(&Origin::from_crates_io_name("Inflector")).await.unwrap();
        let _ = k.rich_crate_async(&Origin::from_crates_io_name("inflector")).await.unwrap();
    })).unwrap();
}

#[tokio::test]
async fn index_test() {
    let idx = Index::new(&KitchenSink::data_path().unwrap()).unwrap();
    let stats = idx.deps_stats().await.unwrap();
    assert!(stats.total > 13800);
    let lode = stats.counts.get("lodepng").unwrap();
    assert!(lode.runtime.def >= 15 && lode.runtime.def < 100);
}

fn is_alnum(q: &str) -> bool {
    q.as_bytes().iter().copied().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}

#[inline(always)]
fn watch<'a, T>(label: &'static str, f: impl Future<Output = T> + Send + 'a) -> Pin<Box<dyn Future<Output = T> + Send + 'a>> {
    Box::pin(NonBlock::new(label, f))
}
