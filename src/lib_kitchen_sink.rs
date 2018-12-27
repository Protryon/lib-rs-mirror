#[macro_use] extern crate failure;

use crate_files;
use crate_git_checkout;
use crates_index;
use crates_io_client;
use docs_rs_client;
use github_info;

#[macro_use]
extern crate serde_derive;

use rayon;
use reqwest;
use user_db;

mod index;
pub use crate::index::*;
pub use github_info::UserOrg;
use rayon::prelude::*;
mod deps_stats;
pub use crate::deps_stats::*;

mod ctrlcbreak;
pub use crate::ctrlcbreak::*;

pub use crates_index::Crate;
use crates_index::Version;
pub use crates_io_client::CrateDepKind;
pub use crates_io_client::CrateDependency;
pub use crates_io_client::CrateMetaVersion;
use crates_io_client::CrateOwner;
pub use crates_io_client::CratesIoCrate;
pub use github_info::User;
pub use github_info::UserType;
pub use rich_crate::Include;
pub use rich_crate::Markup;
pub use rich_crate::Origin;
pub use rich_crate::RichCrate;
pub use rich_crate::RichCrateVersion;
pub use rich_crate::RichDep;
pub use rich_crate::{Cfg, Target};

use cargo_toml::Manifest;
use cargo_toml::Package;
use chrono::DateTime;
use crate_db::{CrateDb, RepoChange};
use crate_files::CrateFile;
use failure::ResultExt;
use github_info::GitCommitAuthor;
use itertools::Itertools;
use lazyonce::LazyOnce;
use repo_url::Repo;
use repo_url::RepoHost;
use repo_url::SimpleRepo;
use rich_crate::Author;
use rich_crate::Derived;
use rich_crate::Readme;
pub use semver::Version as SemVer;
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
    #[fail(display = "{} does not exist\nPlease get data files from https://crates.rs/data and put them in that directory, or set CRATES_DATA_DIR to their location.", _0)]
    CacheDbMissing(String),
    #[fail(display = "Error when parsing verison")]
    SemverParsingError,
    #[fail(display = "Stopped")]
    Stopped,
}

/// This is a collection of various data sources. It mostly acts as a starting point and a factory for other objects.
pub struct KitchenSink {
    pub index: Index,
    crates_io: crates_io_client::CratesIoClient,
    docs_rs: docs_rs_client::DocsRsClient,
    crate_db: CrateDb,
    user_db: user_db::UserDb,
    gh: github_info::GitHub,
    crate_derived_cache: TempCache<(String, RichCrateVersionCacheData, Warnings)>,
    loaded_rich_crate_version_cache: RwLock<HashMap<Box<str>, RichCrateVersion>>,
    category_crate_counts: LazyOnce<Option<HashMap<String, u32>>>,
    removals: LazyOnce<HashMap<Origin, f64>>,
    top_crates_cached: RwLock<HashMap<String, Arc<Vec<(Origin, u32)>>>>,
    git_checkout_path: PathBuf,
    main_cache_dir: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct RichCrateVersionCacheData {
    derived: Derived,
    manifest: Manifest,
    readme: Result<Option<Readme>, ()>,
    lib_file: Option<String>,
    path_in_repo: Option<String>,
    has_buildrs: bool,
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
        let index_path = Self::assert_exists(data_path.join("index"))?;

        let ((crates_io, gh), (index, crate_derived_cache)) = rayon::join(|| rayon::join(
                || crates_io_client::CratesIoClient::new(data_path),
                || github_info::GitHub::new(&data_path.join("github.db"), github_token)),
            || rayon::join(
                || Index::new(index_path),
                || TempCache::new(&data_path.join("crate_derived.db"))));
        Ok(Self {
            crates_io: crates_io?,
            index,
            docs_rs: docs_rs_client::DocsRsClient::new(data_path.join("docsrs.db"))?,
            crate_db: CrateDb::new(Self::assert_exists(data_path.join("crate_data.db"))?)?,
            user_db: user_db::UserDb::new(Self::assert_exists(data_path.join("users.db"))?)?,
            gh: gh?,
            crate_derived_cache: crate_derived_cache?,
            loaded_rich_crate_version_cache: RwLock::new(HashMap::new()),
            git_checkout_path: data_path.join("git"),
            category_crate_counts: LazyOnce::new(),
            removals: LazyOnce::new(),
            top_crates_cached: RwLock::new(HashMap::new()),
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
    /// It returns only a thin and mostly useless data from the index itself,
    /// so `rich_crate`/`rich_crate_version` is needed to do more.
    pub fn all_crates(&self) -> &HashMap<Origin, Crate> {
        self.index.crates()
    }

    pub fn all_new_crates<'a>(&'a self) -> CResult<impl Iterator<Item = RichCrate> + 'a> {
        let min_timestamp = self.crate_db.latest_crate_update_timestamp()?.unwrap_or(0);
        let res: Vec<RichCrate> = self.index.crates()
        .par_iter().map(|(_, v)| v)
        .filter_map(move |k| {
            self.rich_crate_from_index(k).ok()
        })
        .filter(move |k| {
            let latest = k.versions().map(|v| v.created_at.as_str()).max().unwrap_or("");
            if let Ok(timestamp) = DateTime::parse_from_rfc3339(latest) {
                timestamp.timestamp() >= min_timestamp as i64
            } else {
                eprintln!("Can't parse {} of {}", latest, k.name());
                true
            }
        }).collect();
        Ok(res.into_iter())
    }

    /// Wrapper object for metadata common for all versions of a crate
    pub fn rich_crate(&self, origin: &Origin) -> CResult<RichCrate> {
        self.rich_crate_from_index(self.index.crate_by_name(origin).context("rich_crate")?)
    }

    pub fn rich_crate_from_index(&self, krate: &Crate) -> CResult<RichCrate> {
        if stopped() {Err(KitchenSinkErr::Stopped)?;}
        let name = krate.name();
        let cache_bust = krate.latest_version().version(); // most recently published version
        let meta = self.crates_io.krate(name, cache_bust)
            .with_context(|_| format!("crates.io meta for {} {}", name, cache_bust))?;
        let meta = meta.ok_or_else(|| KitchenSinkErr::CrateNotFound(Origin::from_crates_io_name(name)))?;
        Ok(RichCrate::new(meta))
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
            let (d, warn) = self.rich_crate_version_data(krate, fetch_type).with_context(|_| format!("get rich crate data for {}", key.0))?;
            if fetch_type == CrateData::Full {
                self.crate_derived_cache.set(key.0, (key.1.to_string(), d.clone(), warn.clone()))?;
            } else if fetch_type == CrateData::FullNoDerived {
                self.crate_derived_cache.delete(key.0).context("clear cache 2")?;
            }
            (d, warn)
        };
        Ok((RichCrateVersion::new(krate.clone(), d.manifest, d.derived, d.readme, d.lib_file, d.path_in_repo, d.has_buildrs), warn))
    }

    fn rich_crate_version_data(&self, latest: &crates_index::Version, fetch_type: CrateData) -> CResult<(RichCrateVersionCacheData, Warnings)> {
        let mut warnings = HashSet::new();

        let name = latest.name();
        let ver = latest.version();

        let crate_tarball = self.crates_io.crate_data(name, ver).context("crate_file")?
            .ok_or_else(|| KitchenSinkErr::DataNotFound(format!("{}-{}", name, ver)))?;
        let crate_compressed_size = crate_tarball.len();
        let mut meta = crate_files::read_archive(&crate_tarball[..], name, ver)?;
        drop(crate_tarball);

        let has_buildrs = meta.has("build.rs");

        let mut derived = Derived::default();
        mem::swap(&mut derived.language_stats, &mut meta.language_stats); // move
        derived.crate_compressed_size = crate_compressed_size;
        // sometimes uncompressed sources without junk are smaller than tarball with junk
        derived.crate_decompressed_size = meta.decompressed_size.max(crate_compressed_size);
        derived.is_nightly = meta.is_nightly;

        let origin = Origin::from_crates_io_name(name);

        let package = meta.manifest.package.as_mut().ok_or_else(|| KitchenSinkErr::NotAPackage(origin.clone()))?;

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

        // Guess repo URL if none was specified
        if package.repository.is_none() {
            warnings.insert(Warning::NoRepositoryProperty);
            if package.homepage.as_ref().map_or(false, |h| Repo::looks_like_repo_url(h)) {
                package.repository = package.homepage.take();
            }
        }

        let has_readme = meta.readme.as_ref().ok().and_then(|opt| opt.as_ref()).is_some();
        if !has_readme {
            warnings.insert(Warning::NoReadmeProperty);
            if fetch_type != CrateData::Minimal {
                warnings.extend(self.add_readme_from_repo(&mut meta, maybe_repo.as_ref()));
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

        // Guess categories if none were specified
        if package.categories.is_empty() && fetch_type == CrateData::Full {
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
                match crate_repo.host() {
                    RepoHost::GitHub(ref repo) => {
                        let cachebust = self.cachebust_string_for_repo(crate_repo).context("ghrepo")?;
                        if let Some(ghrepo) = self.gh.repo(repo, &cachebust)? {
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
                    },
                    _ => {}, // TODO
                }
            }
        }

        // lib file takes majority of space in cache, so remove it if it won't be used
        if !self.is_readme_short(meta.readme.as_ref().map(|r| r.as_ref()).map_err(|_| ())) {
            meta.lib_file = None;
        }

        Ok((RichCrateVersionCacheData {
            derived,
            has_buildrs,
            manifest: meta.manifest,
            readme: meta.readme.map_err(|_|()),
            lib_file: meta.lib_file,
            path_in_repo,
        }, warnings))
    }

    pub fn is_readme_short(&self, readme: Result<Option<&Readme>, ()>) -> bool {
        if let Ok(Some(ref r)) = readme {
            match r.markup {
                Markup::Markdown(ref s) | Markup::Rst(ref s) => s.len() < 1000,
            }
        } else {
            true
        }
    }

    fn add_readme_from_repo(&self, meta: &mut CrateFile, maybe_repo: Option<&Repo>) -> Warnings {
        let mut warnings = HashSet::new();
        let package = match meta.manifest.package.as_ref() {
            Some(p) => p,
            None => {
                warnings.insert(Warning::NotAPackage);
                return warnings;
            }
        };
        if let Some(repo) = maybe_repo {
            let res = crate_git_checkout::checkout(repo, &self.git_checkout_path)
            .map_err(From::from)
            .and_then(|checkout| {
                crate_git_checkout::find_readme(&checkout, package)
            });
            match res {
                Ok(Some(readme)) => {
                    meta.readme = Ok(Some(readme));
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

        if package.homepage.as_ref().map_or(false, |d| Self::is_docs_rs_link(d) || d.starts_with("https://crates.rs/") || d.starts_with("https://crates.io/")) {
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
        reqwest::Client::builder().build()
        .and_then(|res| res.get(url).send())
        .map(|res| {
            res.status().is_success()
        })
        .unwrap_or(false)
    }

    fn is_docs_rs_link(d: &str) -> bool {
        d.starts_with("https://docs.rs/") || d.starts_with("http://docs.rs/") ||
        d.starts_with("http://crates.fyi/") || d.starts_with("https://crates.fyi/")
    }

    pub fn has_docs_rs(&self, name: &str, ver: &str) -> bool {
        self.docs_rs.builds(name, ver).unwrap_or(true) // fail open
    }

    fn is_same_url<A: AsRef<str> + std::fmt::Debug>(a: Option<A>, b: Option<&String>) -> bool {
        fn trim_suffix(s: &str) -> &str {
            s.split('#').next().unwrap().trim_end_matches("/index.html").trim_end_matches('/')
        }

        match (a, b) {
            (Some(ref a), Some(ref b)) if trim_suffix(a.as_ref()).eq_ignore_ascii_case(trim_suffix(b)) => true,
            _ => false,
        }
    }

    pub fn all_dependencies_flattened(&self, origin: &Origin) -> Result<HashMap<Arc<str>, (DepInf, SemVer)>, KitchenSinkErr> {
        self.index.all_dependencies_flattened(self.index.crate_by_name(origin)?)
    }

    pub fn prewarm(&self) {
        self.index.deps_stats();
    }

    pub fn dependents_stats_of(&self, krate: &RichCrateVersion) -> Option<RevDependencies> {
        let deps = self.index.deps_stats();
        deps.counts.get(krate.short_name()).cloned()
    }

    /// (latest, pop)
    /// 0 = not used
    /// 1 = everyone uses it
    pub fn version_popularity(&self, crate_name: &str, requirement: &VersionReq) -> (bool, f32) {
        self.index.version_popularity(crate_name, requirement)
    }

    /// "See also"
    pub fn related_categories(&self, slug: &str) -> CResult<Vec<String>> {
        self.crate_db.related_categories(slug)
    }

    /// Recommendations
    pub fn related_crates(&self, krate: &RichCrateVersion) -> CResult<Vec<RichCrateVersion>> {
        let (replacements, related) = rayon::join(
            || self.crate_db.replacement_crates(krate.short_name()).context("related_crates1"),
            || self.crate_db.related_crates(krate.origin()).context("related_crates2"),
        );

        let replacements: Vec<_> = replacements?.into_iter()
            .map(|name| Origin::from_crates_io_name(&name))
            .chain(related?)
            .unique()
            .take(10)
            .collect();
        Ok(replacements.into_par_iter()
            .with_max_len(1)
            .map(|origin| {
                self.rich_crate_version(&origin, CrateData::Minimal)
            })
            .filter_map(|res| res.map_err(|e| eprintln!("related crate err: {}", e)).ok())
            .collect())
    }

    /// Returns (nth, slug)
    pub fn top_category<'crat>(&self, krate: &'crat RichCrateVersion) -> Option<(u32, Cow<'crat, str>)> {
        let crate_origin = krate.origin();
        krate.category_slugs(Include::Cleaned)
        .filter_map(|slug| {
            self.top_crates_in_category(&slug).ok()
            .and_then(|cat| {
                cat.iter().position(|(o, _)| o == crate_origin).map(|pos| {
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
                Ok(Some(user)) => {
                    println!("{} == {} ({:?})", user.login, email, name);
                    self.user_db.index_user(&user, Some(email), name)?;
                },
                Ok(None) => println!("{} not found on github", email),
                Err(e) => eprintln!("•••• {}", e),
            }
        }
        Ok(())
    }

    /// Maintenance: add crate to local db index
    pub fn index_crate(&self, k: &RichCrate) -> CResult<()> {
        if stopped() {Err(KitchenSinkErr::Stopped)?;}
        self.crate_db.index_versions(k)?;
        Ok(())
    }

    pub fn index_crate_highest_version(&self, v: &RichCrateVersion) -> CResult<()> {
        if stopped() {Err(KitchenSinkErr::Stopped)?;}
        self.crate_db.index_latest(v)?;
        Ok(())
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

    /// If given crate is a sub-crate, return crate that owns it.
    /// The relationship is based on directory layout of monorepos.
    pub fn parent_crate(&self, child: &RichCrateVersion) -> Option<RichCrateVersion> {
        let repo = child.repository()?;
        let name = self.crate_db.parent_crate(repo, child.short_name()).ok().and_then(|v| v)?;
        self.rich_crate_version(&Origin::from_crates_io_name(&name), CrateData::Minimal)
            .map_err(|e| eprintln!("parent crate: {} {}", e, name)).ok()
    }

    pub fn cachebust_string_for_repo(&self, crate_repo: &Repo) -> CResult<String> {
        Ok(self.crate_db.crates_in_repo(crate_repo)
            .context("db cache_bust")?
            .into_iter()
            .filter_map(|name| {
                self.index.crate_version_latest_unstable(&Origin::from_crates_io_name(&name)).ok()
            })
            .map(|k| k.version().to_string())
            .next()
            .unwrap_or_else(|| "*".to_string()))
    }

    pub fn user_github_orgs(&self, github_login: &str) -> CResult<Option<Vec<UserOrg>>> {
        Ok(self.gh.user_orgs(github_login)?)
    }

    /// Merge authors, owners, contributors
    pub fn all_contributors<'a>(&self, krate: &'a RichCrateVersion) -> CResult<(Vec<CrateAuthor<'a>>, Vec<CrateAuthor<'a>>, bool, usize)> {
        let mut contributors = match krate.repository().as_ref() {
            Some(crate_repo) => match crate_repo.host() {
                // TODO: warn on errors?
                RepoHost::GitHub(ref repo) => {
                    // multiple crates share a repo, which causes cache churn when version "changes"
                    // so pick one of them and track just that one version
                    let cachebust = self.cachebust_string_for_repo(crate_repo).context("contrib")?;
                    let contributors = self.gh.contributors(repo, &cachebust).context("contributors")?.unwrap_or_default();
                    let mut by_login = HashMap::new();
                    for contr in contributors {
                        if let Some(author) = contr.author {
                            let count = contr.weeks.iter()
                                .map(|w| {
                                    w.commits as f64 +
                                    ((w.added + w.deleted*2) as f64).sqrt()
                                }).sum::<f64>();
                            by_login.entry(author.login.to_ascii_lowercase())
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

        let hit_max_contributor_count = contributors.len() == 100;

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
                        let login = github.login.to_ascii_lowercase();
                        ca.github = Some(github);
                        return (AuthorId::GitHub(login), ca);
                    }
                }
                // name only, no email
                else if let Some(ref name) = author.name {
                    if let Some((contribution, github)) = contributors.remove(&name.to_lowercase()) {
                        let login = github.login.to_lowercase();
                        ca.github = Some(github);
                        ca.info = None; // was useless; just a login; TODO: only clear name once it's Option
                        ca.contribution = contribution;
                        return (AuthorId::GitHub(login), ca);
                    }
                }
                let key = author.email.as_ref().map(|e| AuthorId::Email(e.to_ascii_lowercase()))
                    .or_else(|| author.name.clone().map(AuthorId::Name))
                    .unwrap_or(AuthorId::Meh(i));
                (key, ca)
            }).collect();

        if let Ok(owners) = self.crate_owners(krate) {
            for owner in owners {
                if let Some(login) = owner.github_login() {
                    match authors.entry(AuthorId::GitHub(login.to_ascii_lowercase())) {
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
                                github: self.user_by_github_login(login).ok().and_then(|a|a),
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

        for (login, (contribution, github)) in contributors {
            authors.entry(AuthorId::GitHub(login))
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
            match authors_by_name.entry(a.name().to_owned()) {
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
            let gh_is_team = owners[0].github.as_ref().map_or(false, |g| g.user_type != UserType::User);
            if author_is_team == gh_is_team {
                let co = owners.remove(0);
                authors[0].github = co.github;
                authors[0].owner = co.owner;
            }
        }

        let owners_partial = authors.iter().any(|a| a.owner);
        Ok((authors, owners, owners_partial, if hit_max_contributor_count { 100 } else { contributors }))
    }

    fn crate_owners(&self, krate: &RichCrateVersion) -> CResult<Vec<CrateOwner>> {
        Ok(self.crates_io.crate_owners(krate.short_name(), krate.version()).context("crate_owners")?.unwrap_or_default())
    }

    // Sorted from the top, returns `(origin, recent_downloads)`
    pub fn top_crates_in_category(&self, slug: &str) -> CResult<Arc<Vec<(Origin, u32)>>> {
        {
            let cache = self.top_crates_cached.read().unwrap();
            if let Some(category) = cache.get(slug) {
                return Ok(category.clone());
            }
        }
        let mut cache = self.top_crates_cached.write().unwrap();
        use std::collections::hash_map::Entry::*;
        Ok(match cache.entry(slug.to_owned()) {
            Occupied(e) => Arc::clone(e.get()),
            Vacant(e) => {
                let mut crates = self.crate_db.top_crates_in_category_partially_ranked(slug, 130)?;
                let removals = self.removals.get(|| self.crate_db.removals().unwrap());
                for c in &mut crates {
                    c.2 /= 300. + removals.get(&c.0).cloned().unwrap_or(2.);
                }
                crates.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap());
                let crates: Vec<_> = crates.into_iter().map(|(o, r, _)| (o, r)).take(100).collect();
                let res = Arc::new(crates);
                e.insert(Arc::clone(&res));
                res
            },
        })
    }

    pub fn top_keywords_in_category(&self, slug: &str) -> CResult<Vec<String>> {
        Ok(self.crate_db.top_keywords_in_category(slug)?)
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

    pub fn category_crate_count(&self, slug: &str) -> Result<u32, KitchenSinkErr> {
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
                    eprintln!("Known categories: {:?}", h);
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
    GitHub(String),
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
