#[macro_use] extern crate failure;
extern crate cargo_toml;
extern crate categories;
extern crate crate_db;
extern crate crate_files;
extern crate crates_index;
extern crate crates_io_client;
extern crate crate_git_checkout;
extern crate docs_rs_client;
extern crate file;
extern crate github_info;
extern crate lazyonce;
extern crate regex;
extern crate repo_url;
extern crate rich_crate;
extern crate serde;
extern crate serde_json;
#[macro_use] extern crate serde_derive;
extern crate toml;
extern crate url;
extern crate user_db;
extern crate reqwest;
extern crate simple_cache;
extern crate itertools;

pub use crates_index::Crate;
use crates_io_client::CrateOwner;
pub use crates_io_client::CrateDependency;
pub use crates_io_client::CrateDepKind;
pub use crates_io_client::CrateMetaVersion;
pub use crates_io_client::CratesIoCrate;
pub use github_info::User;
pub use github_info::UserType;
pub use rich_crate::{Cfg, Target};
pub use rich_crate::RichCrate;
pub use rich_crate::RichCrateVersion;
pub use rich_crate::Origin;

use simple_cache::SimpleCache;
use crate_files::CrateFile;
use cargo_toml::TomlLibOrBin;
use cargo_toml::TomlManifest;
use cargo_toml::TomlPackage;
use failure::ResultExt;
use github_info::GitCommitAuthor;
use lazyonce::LazyOnce;
use repo_url::Repo;
use repo_url::RepoHost;
use repo_url::SimpleRepo;
use rich_crate::Author;
use rich_crate::Derived;
use rich_crate::Readme;
use itertools::Itertools;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::hash_map::Entry::*;
use std::collections::HashMap;
use std::path::{PathBuf, Path};
use std::env;
use crate_db::{CrateDb, RepoChange};

pub type CError = failure::Error;
pub type CResult<T> = Result<T, CError>;

#[derive(Debug, Clone, Fail)]
pub enum KitchenSinkErr {
    #[fail(display = "git checkout meh")]
    GitCheckoutFailed,
    #[fail(display = "category not found")]
    CategoryNotFound,
    #[fail(display = "category query failed")]
    CategoryQueryFailed,
    #[fail(display = "crate not found: {:?}", _0)]
    CrateNotFound(Origin),
    #[fail(display = "Environment variable CRATES_DATA_DIR is not set.\nChoose a dir where it's OK to store lots of data, and export it like CRATES_DATA_DIR=/var/lib/crates.rs")]
    CratesDataDirEnvVarMissing,
    #[fail(display = "{} does not exist\nPlease get data files from https://crates.rs/data and put them in that directory, or set CRATES_DATA_DIR to their location.", _0)]
    CacheDbMissing(String),
}

/// This is a collection of various data sources. It mostly acts as a starting point and a factory for other objects.
pub struct KitchenSink {
    index: crates_index::Index,
    crates_io: crates_io_client::CratesIoClient,
    docs_rs: docs_rs_client::DocsRsClient,
    crate_db: CrateDb,
    user_db: user_db::UserDb,
    gh: github_info::GitHub,
    crate_derived_cache: SimpleCache,
    category_crate_counts: LazyOnce<Option<HashMap<String, u32>>>,
    crate_path_index: LazyOnce<HashMap<Origin, PathBuf>>,
    git_checkout_path: PathBuf,
    main_cache_path: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct RichCrateVersionCacheData {
    derived: Derived,
    manifest: TomlManifest,
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
        let main_cache_path = Self::assert_exists(data_path.join("cache.db"))?;
        Ok(Self {
            index: crates_index::Index::new(Self::assert_exists(data_path.join("index"))?),
            docs_rs: docs_rs_client::DocsRsClient::new(&main_cache_path)?,
            crate_db: CrateDb::new(Self::assert_exists(data_path.join("crate_meta.db"))?)?,
            user_db: user_db::UserDb::new(Self::assert_exists(data_path.join("users.db"))?)?,
            gh: github_info::GitHub::new(&Self::assert_exists(data_path.join("github.db"))?, github_token)?,
            crates_io: crates_io_client::CratesIoClient::new(data_path)?,
            crate_derived_cache: SimpleCache::new(&data_path.join("crate_derived.db"))?,
            git_checkout_path: data_path.join("git"),
            crate_path_index: LazyOnce::new(),
            category_crate_counts: LazyOnce::new(),
            main_cache_path,
        })
    }

    fn assert_exists(path: PathBuf) -> Result<PathBuf, KitchenSinkErr> {
        if !path.exists() {
            Err(KitchenSinkErr::CacheDbMissing(path.display().to_string()))
        } else {
            Ok(path)
        }
    }

    fn data_path() -> Result<PathBuf, KitchenSinkErr> {
        match env::var("CRATES_DATA_DIR") {
            Ok(d) => {
                if !Path::new(&d).join("cache.db").exists() {
                    return Err(KitchenSinkErr::CacheDbMissing(d));
                }
                Ok(d.into())
            },
            Err(_) => {
                for path in &["../data", "./data", "/var/lib/crates.rs/data", "/www/crates.rs/data"] {
                    let path = Path::new(path);
                    if path.exists() && path.join("cache.db").exists() {
                        return Ok(path.to_owned());
                    }
                }
                Err(KitchenSinkErr::CratesDataDirEnvVarMissing)
            },
        }
    }

    pub fn main_cache_path(&self) -> &Path {
        &self.main_cache_path
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
    pub fn all_crates(&self) -> crates_index::Crates {
        self.index.crates()
    }

    pub fn crate_by_name(&self, name: &Origin) -> Result<Crate, KitchenSinkErr> {
        self.crate_path_index.get(|| {
            self.index.crate_index_paths()
                .filter_map(|p| {
                    let f = p.file_name().and_then(|f| f.to_str()).map(|s| s.to_lowercase());
                    f.map(|f| (Origin::from_crates_io_name(&f), p))
                })
                .collect()
        })
        .get(name)
        .map(Crate::new)
        .ok_or_else(|| KitchenSinkErr::CrateNotFound(name.clone()))
    }

    /// Wrapper object for metadata common for all versions of a crate
    pub fn rich_crate(&self, origin: &Origin) -> CResult<RichCrate> {
        let krate = self.crate_by_name(origin)?;
        let name = krate.name();
        let cache_bust = krate.latest_version().version();
        let meta = self.crates_io.krate(name, cache_bust)
            .with_context(|_| format!("crates.io meta for {} {}", name, cache_bust))?;
         Ok(RichCrate::new(meta))
    }

    /// Wrapper for the latest version of a given crate.
    ///
    /// This function is quite slow, as it reads everything about the crate.
    ///
    /// There's no support for getting anything else than the latest version.
    pub fn rich_crate_version(&self, origin: &Origin, fetch_type: CrateData) -> CResult<RichCrateVersion> {
        let krate = self.crate_by_name(origin)?;
        let latest = krate.latest_version().clone();
        let cache_key = format!("{}-{}", latest.name(), latest.version());
        let d = if fetch_type != CrateData::FullNoDerived {
            if let Ok(cached) = self.crate_derived_cache.get(&cache_key)
            .with_context(|_| String::new())
            .and_then(|cached| {
                serde_json::from_slice(&cached)
                    .map_err(|e| {eprintln!("bad cache data: {} {}", cache_key, e); e})
                    .with_context(|_| format!("parse from cache: {}", cache_key))
            }) {
                eprintln!("hit! {}", cache_key);
                Some(cached)
            } else {
                None
            }
        } else {
            None
        };

        let d = if let Some(d) = d {d} else {
            let d = self.rich_crate_version_data(&latest, fetch_type).context("get rich crate data")?;
            if fetch_type == CrateData::Full {
                eprintln!("miss! {}", cache_key);
                self.crate_derived_cache.set(&cache_key, &serde_json::to_vec(&d).context("ser to cache")?).context("save to cache")?;
            } else if fetch_type == CrateData::FullNoDerived {
                self.crate_derived_cache.delete(&cache_key).context("clear cache")?;
            }
            d
        };
        Ok(RichCrateVersion::new(latest, d.manifest, d.derived, d.readme, d.lib_file, d.path_in_repo, d.has_buildrs))
    }

    fn rich_crate_version_data(&self, latest: &crates_index::Version, fetch_type: CrateData) -> CResult<RichCrateVersionCacheData> {
        let name = latest.name();
        let ver = latest.version();
        let mut meta = self.crate_file(name, ver).context("crate file")?;

        let maybe_repo = meta.manifest.package.repository.as_ref().and_then(|r| Repo::new(r).ok());

        let has_readme = meta.readme.as_ref().ok().and_then(|opt| opt.as_ref()).is_some();
        if !has_readme && fetch_type != CrateData::Minimal {
            self.add_readme_from_repo(&mut meta, maybe_repo.as_ref());
        }

        // Quick'n'dirty autobins substitute
        if meta.manifest.bin.is_empty() {
            if let Some(path) = meta.find(|p| p.starts_with("src/bin")).map(|t| t.display().to_string()) {
                meta.manifest.bin.push(TomlLibOrBin {
                    path: Some(path),
                    name: Some(meta.manifest.package.name.clone()),
                    bench: None,
                    doc: None,
                    plugin: None,
                    proc_macro: None,
                    test: None,
                    doctest: None,
                    harness: None,
                });
            }
        }

        // quick and dirty autolibs substitute
        if meta.manifest.lib.is_none() && meta.has("src/lib.rs") {
            meta.manifest.lib = Some(TomlLibOrBin {
                path: Some("src/lib.rs".to_owned()),
                name: Some(meta.manifest.package.name.clone()),
                bench: None,
                doc: None,
                plugin: None,
                proc_macro: None,
                test: None,
                doctest: None,
                harness: None,
            });
        }

        let mut derived = Derived::default();
        let origin = Origin::from_crates_io_name(name);

        // Guess keywords if none were specified
        // TODO: also ignore useless keywords that are unique db-wide
        if meta.manifest.package.keywords.is_empty() && fetch_type != CrateData::Minimal {
            let gh = maybe_repo.as_ref()
                .and_then(|repo| if let RepoHost::GitHub(ref gh) = repo.host() {
                    self.gh.topics(gh).ok()
                } else {None});
            if let Some(mut topics) = gh {
                topics.retain(|t| match t.as_str() {
                    "rust" | "rs" | "rustlang" | "rust-lang" | "crate" | "crates" | "library" => false,
                    t if t.starts_with("rust-") => false,
                    _ => true
                });
                if !topics.is_empty() {
                    meta.manifest.package.keywords = topics;
                }
            }
            if meta.manifest.package.keywords.is_empty() && fetch_type != CrateData::FullNoDerived {
                derived.keywords = Some(self.crate_db.keywords(&origin).context("keywordsdb")?);
            }
        }

        // Guess categories if none were specified
        if meta.manifest.package.categories.is_empty() && fetch_type == CrateData::Full {
            derived.categories = Some({
                let keywords_iter = meta.manifest.package.keywords.iter().map(|s| s.as_str());
                self.crate_db.crate_categories(&origin, keywords_iter).context("catdb")?
                .into_iter().map(|(_, c)| c).collect()
            });
        }

        // Guess repo URL if none was specified
        if meta.manifest.package.repository.is_none() &&
           meta.manifest.package.homepage.as_ref().map_or(false, |h| Repo::looks_like_repo_url(h)) {
            meta.manifest.package.repository = meta.manifest.package.homepage.take();
        }

        // Delete the original docs.rs link, because we have our own
        // TODO: what if the link was to another crate or a subpage?
        if meta.manifest.package.documentation.as_ref().map_or(false, |d| d.starts_with("https://docs.rs/")) {
            if self.has_docs_rs(name, ver) {
                meta.manifest.package.documentation = None; // docs.rs is not proper docs
            }
        }

        self.remove_redundant_links(&mut meta.manifest.package, maybe_repo.as_ref());

        if meta.manifest.package.homepage.is_none() && fetch_type != CrateData::Minimal {
            if let Some(ref repo) = maybe_repo {
                match repo.host() {
                    RepoHost::GitHub(ref repo) => {
                        if let Ok(ghrepo) = self.gh.repo(repo) {
                            if let Some(url) = ghrepo.github_page_url {
                                meta.manifest.package.homepage = Some(url);
                            } else if let Some(url) = ghrepo.homepage {
                                meta.manifest.package.homepage = Some(url);
                            }
                            self.remove_redundant_links(&mut meta.manifest.package, maybe_repo.as_ref());
                        }
                    },
                    _ => {}, // TODO
                }
            }
        }

        let path_in_repo = maybe_repo.as_ref().and_then(|repo| {
            if fetch_type != CrateData::Minimal {
            self.crate_db.path_in_repo(repo, name).ok()
            } else {
                None
            }
        });

        Ok(RichCrateVersionCacheData {
            derived,
            has_buildrs: meta.has("build.rs"),
            manifest: meta.manifest,
            readme: meta.readme.map_err(|_|()),
            lib_file: meta.lib_file,
            path_in_repo,
        })
    }

    fn add_readme_from_repo(&self, meta: &mut CrateFile, maybe_repo: Option<&Repo>) {
        if let Some(repo) = maybe_repo {
            let res = crate_git_checkout::checkout(repo, &self.git_checkout_path, &meta.manifest.package.name)
            .map_err(From::from)
            .and_then(|checkout| {
                crate_git_checkout::find_readme(&checkout, &meta.manifest.package)
            });
            match res {
                Ok(Some(readme)) => meta.readme = Ok(Some(readme)),
                nope => {
                    if let Err(err) = nope {
                        eprintln!("Checkout of {} failed: {}", meta.manifest.package.name, err);
                    }
                },
            }
        }
    }

    fn remove_redundant_links(&self, package: &mut TomlPackage, maybe_repo: Option<&Repo>) {

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

        if package.homepage.as_ref().map_or(false, |d| d.starts_with("https://docs.rs/") || d.starts_with("https://crates.rs/") || d.starts_with("https://crates.io/")) {
            package.homepage = None;
        }

        if package.homepage.as_ref().map_or(false, |url| !self.check_url_is_valid(url)) {
            package.homepage = None;
        }

        if package.documentation.as_ref().map_or(false, |url| !self.check_url_is_valid(url)) {
            package.documentation = None;
        }
    }

    pub fn check_url_is_valid(&self, url: &str) -> bool {
        reqwest::get(url)
        .map(|res| {
            res.status().is_success()
        })
        .unwrap_or(false)
    }

    pub fn has_docs_rs(&self, name: &str, ver: &str) -> bool {
        self.docs_rs.builds(name, ver).unwrap_or(true) // fail open
    }

    fn is_same_url<A: AsRef<str> + std::fmt::Debug>(a: Option<A>, b: Option<&String>) -> bool {
        fn trim_hash(s: &str) -> &str {
            s.split('#').next().unwrap()
        }

        match (a, b) {
            (Some(ref a), Some(ref b)) if trim_hash(a.as_ref()).eq_ignore_ascii_case(trim_hash(b)) => true,
            _ => false,
        }
    }

    /// "See also"
    pub fn related_categories(&self, slug: &str) -> CResult<Vec<String>> {
        self.crate_db.related_categories(slug)
    }

    /// Recommendations
    pub fn related_crates(&self, krate: &RichCrateVersion) -> CResult<Vec<RichCrateVersion>> {
        let replacements = self.crate_db.replacement_crates(krate.short_name()).context("related_crates1")?;
        let related = self.crate_db.related_crates(krate.origin()).context("related_crates2")?;

        Ok(replacements.into_iter()
        .map(|name| Origin::from_crates_io_name(&name))
        .chain(related)
        .unique()
        .take(10)
        .map(|origin| {
            self.rich_crate_version(&origin, CrateData::Minimal)
        })
        .filter_map(|res| res.map_err(|e| eprintln!("related crate err: {}", e)).ok())
        .collect())
    }

    /// Returns (nth, slug)
    pub fn top_category(&self, krate: &RichCrate) -> Option<(u32, String)> {
        self.crate_db.top_category(&krate.origin()).ok()
    }

    /// Returns (nth, keyword)
    pub fn top_keyword(&self, krate: &RichCrate) -> Option<(u32, String)> {
        self.crate_db.top_keyword(&krate.origin()).ok()
    }

    /// Maintenance: add user to local db index
    pub fn index_user(&self, user: &User, commit: &GitCommitAuthor) -> CResult<()> {
        if !self.user_db.email_has_github(&commit.email)? {
            println!("{} => {}", commit.email, user.login);
            self.user_db.index_user(&user, Some(&commit.email), commit.name.as_ref().map(|s|s.as_str()))?;
        }
        Ok(())
    }

    /// Maintenance: add user to local db index
    pub fn index_email(&self, email: &str, name: Option<&str>) -> CResult<()> {
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
        self.crate_db.index_versions(k)?;
        Ok(())
    }

    pub fn index_crate_latest_version(&self, v: &RichCrateVersion) -> CResult<()> {
        self.crate_db.index_latest(v)?;
        Ok(())
    }

    pub fn index_repo(&self, repo: &Repo, crate_name: &str) -> CResult<()> {
        let url = repo.canonical_git_url();
        let checkout = crate_git_checkout::checkout(repo, &self.git_checkout_path, crate_name)?;

        let (manif, warnings) = crate_git_checkout::find_manifests(&checkout)
            .with_context(|_| format!("find manifests in {}", url))?;
        for warn in warnings {
            eprintln!("{}", warn);
        }
        let manif = manif.into_iter().map(|(subpath, manifest)| (subpath, manifest.package.name));
        self.crate_db.index_repo_crates(repo, manif).context("index rev repo")?;

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
                                if dep1 == dep2 {continue;}
                                // Not really a replacement, but a recommendation if A then B
                                changes.push(RepoChange::Replaced{crate_name: dep1.to_string(), replacement: dep2.to_string(), weight})
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
                            changes.push(RepoChange::Replaced{crate_name: crate_name.clone(), replacement: replacement.to_string(), weight})
                        }
                        changes.push(RepoChange::Removed{crate_name, weight});
                    } else {
                        // ??? maybe use sliiight recommendation score based on existing (i.e. newer) state of deps in the repo?
                        changes.push(RepoChange::Removed{crate_name, weight: 0.95});
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
        return Ok(self.gh.user_by_login(github_login).ok()); // errs on 404
    }

    /// If given crate is a sub-crate, return crate that owns it.
    /// The relationship is based on directory layout of monorepos.
    pub fn parent_crate(&self, child: &RichCrateVersion) -> Option<RichCrateVersion> {
        let repo = child.repository()?;
        let name = self.crate_db.parent_crate(repo, child.short_name()).ok().and_then(|v| v)?;
        self.rich_crate_version(&Origin::from_crates_io_name(&name), CrateData::Minimal).ok()
    }

    /// Merge authors, owners, contributors
    pub fn all_contributors<'a>(&self, krate: &'a RichCrateVersion) -> (Vec<CrateAuthor<'a>>, Vec<CrateAuthor<'a>>, bool, usize) {
        let mut contributors = krate.repository().as_ref().and_then(|repo| {
            match repo.host() {
                // TODO: warn on errors?
                RepoHost::GitHub(ref repo) => self.gh.contributors(repo).ok().and_then(|contributors| {
                    let mut by_login = HashMap::new();
                    for contr in contributors {
                        let count = contr.weeks.iter()
                            .map(|w| {
                                w.commits as f64 +
                                ((w.added + w.deleted*2) as f64).sqrt()
                            }).sum::<f64>();
                        by_login.entry(contr.author.login.to_lowercase())
                            .or_insert((0., contr.author)).0 += count;
                    }
                    Some(by_login)
                }),
                RepoHost::GitLab(..) |
                RepoHost::Other => None, // TODO: could use git checkout...
            }
        }).unwrap_or_default();

        let hit_max_contributor_count = contributors.len() == 100;

        let mut authors: HashMap<AuthorId, CrateAuthor> = krate.authors()
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
                        let login = github.login.to_lowercase();
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
                let key = author.email.as_ref().map(|e| AuthorId::Email(e.to_lowercase()))
                    .or_else(|| author.name.clone().map(AuthorId::Name))
                    .unwrap_or(AuthorId::Meh(i));
                (key, ca)
            }).collect();

        if let Ok(owners) = self.crate_owners(krate) {
            for owner in owners {
                if let Some(login) = owner.github_login() {
                    match authors.entry(AuthorId::GitHub(login.to_lowercase())) {
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

        let mut authors_by_name = HashMap::<String, CrateAuthor>::new();
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

        let max_author_contribution = authors_by_name.values()
            .map(|a| if a.owner || a.nth_author.is_some() {a.contribution} else {0.})
            .max_by(|a,b| a.partial_cmp(&b).unwrap_or(Ordering::Equal))
            .unwrap_or(0.);
        let big_contribution = if max_author_contribution < 50. {200.} else {max_author_contribution/2.};

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

        authors.sort_by(|a,b| {
            fn score(a: &CrateAuthor) -> f64 {
                let o = if a.owner {200.} else {1.};
                o * (a.contribution + 10.) /
                (1+a.nth_author.unwrap_or(99)) as f64
            }
            score(b).partial_cmp(&score(a))
                .unwrap_or(Ordering::Equal)
        });

        // That's a guess
        if authors.len() == 1 && owners.len() == 1 && authors[0].github.is_none() {
            let co = owners.remove(0);
            authors[0].github = co.github;
            authors[0].owner = co.owner;
        }

        let owners_partial = authors.iter().any(|a| a.owner);
        (authors, owners, owners_partial, if hit_max_contributor_count {100} else {contributors})
    }

    fn crate_owners(&self, krate: &RichCrateVersion) -> CResult<Vec<CrateOwner>> {
        Ok(self.crates_io.crate_owners(krate.short_name(), krate.version())?)
    }

    pub fn top_crates_in_category(&self, slug: &str, limit: u32) -> CResult<Vec<(Crate, u32)>> {
        Ok(self.crate_db.top_crates_in_category(slug, limit, false)?
            .iter()
            .filter_map(|(n, d)| {
                self.crate_by_name(n).ok().map(|c| (c, *d))
            })
            .collect())
    }

    pub fn top_keywords_in_category(&self, slug: &str) -> CResult<Vec<String>> {
        Ok(self.crate_db.top_keywords_in_category(slug)?)
    }

    pub fn recently_updated_crates_in_category(&self, slug: &str) -> CResult<Vec<Origin>> {
        Ok(self.crate_db.recently_updated_crates_in_category(slug)?)
    }

    pub fn category_crate_count(&self, slug: &str) -> Result<u32, KitchenSinkErr> {
        self.category_crate_counts.get(|| {
            self.crate_db.category_crate_counts().ok()
        })
        .as_ref()
        .ok_or(KitchenSinkErr::CategoryQueryFailed)
        .and_then(|h| {
            h.get(slug).map(|&c| c)
            .ok_or(KitchenSinkErr::CategoryNotFound)
        })
    }

    /// Read tarball
    fn crate_file(&self, name: &str, ver: &str) -> CResult<crate_files::CrateFile> {
        let data = self.crates_io.crate_data(name, ver).context("crate_file")?;
        Ok(crate_files::read_archive(&data[..], name, ver)?)
    }

    pub fn repo_commits(&self, repo: &SimpleRepo) -> CResult<Vec<github_info::CommitMeta>> {
        Ok(self.gh.commits(repo)?)
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

#[derive(Debug)]
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
