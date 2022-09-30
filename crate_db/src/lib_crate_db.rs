use categories::normalize_keyword;
use categories::Synonyms;
use chrono::prelude::*;
use log::{debug, error};
use rich_crate::CrateOwner;
use rich_crate::CrateVersionSourceData;
use rich_crate::Manifest;
use rich_crate::ManifestExt;
use rich_crate::Markup;
use rich_crate::Origin;
use rich_crate::Readme;
use rich_crate::Repo;
use rich_crate::RichCrate;
use rusqlite::*;
use rusqlite::types::ToSql;
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt::Write;
use std::path::Path;
use std::sync::Arc;
use thread_local::ThreadLocal;
use tokio::sync::{Mutex, RwLock};
type FResult<T, E = Error> = std::result::Result<T, E>;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("DB sqlite error")]
    Db(#[source] #[from] rusqlite::Error),

    #[error("DB sqlite error in {1}")]
    DbCtx(#[source] rusqlite::Error, &'static str),

    #[error("DB I/O error")]
    Io(#[source] #[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}

pub mod builddb;

mod schema;
mod stopwords;
use crate::stopwords::{COND_STOPWORDS, STOPWORDS};

pub struct CrateDb {
    url: String,
    // Sqlite is awful with "database table is locked"
    concurrency_control: RwLock<()>,
    conn: Arc<ThreadLocal<std::result::Result<RefCell<Connection>, rusqlite::Error>>>,
    exclusive_conn: Mutex<Option<Connection>>,
    tag_synonyms: Synonyms,
}

pub struct CrateVersionData<'a> {
    pub origin: &'a Origin,
    pub source_data: &'a CrateVersionSourceData,
    pub manifest: &'a Manifest,
    pub deps_stats: &'a [(&'a str, f32)],
    pub is_build: bool,
    pub is_dev: bool,
    pub authors: &'a [rich_crate::Author],
    pub category_slugs: &'a [Cow<'a, str>],
    pub bad_categories: &'a [String],
    pub repository: Option<&'a Repo>,
    pub extracted_auto_keywords: Vec<(f32, String)>,
    pub cache_key: u64,
}

/// Metadata guessed
pub struct DbDerived {
    pub categories: Vec<Box<str>>,
    pub keywords: Vec<String>,
}

pub struct CrateOwnerStat {
    pub github_id: u64,
    pub created_at: (u16, u8, u8),
    pub num_crates: u32,
}

impl CrateDb {
    /// Path to sqlite db file to create/update
    pub fn new(path: impl AsRef<Path>) -> FResult<Self> {
        let path = path.as_ref();
        Self::new_with_synonyms(path, Synonyms::new(path)?)
    }

    pub fn new_with_synonyms(path: &Path, tag_synonyms: Synonyms) -> FResult<Self> {
        Ok(Self {
            tag_synonyms,
            url: format!("file:{}?cache=shared", path.display()),
            conn: Arc::new(ThreadLocal::new()),
            concurrency_control: RwLock::new(()),
            exclusive_conn: Mutex::new(None),
        })
    }

    #[inline]
    async fn with_read<F, T>(&self, context: &'static str, cb: F) -> FResult<T> where F: FnOnce(&mut Connection) -> FResult<T> {
        let mut _sqlite_sucks = self.concurrency_control.read().await;
        tokio::task::block_in_place(|| {
            let conn = self.conn.get_or(|| self.connect().map(|conn| {
                let _ = conn.busy_timeout(std::time::Duration::from_secs(4));
                RefCell::new(conn)
            }));
            match conn {
                Ok(conn) => {
                    let now = std::time::Instant::now();
                    let res = cb(&mut *conn.borrow_mut());
                    let elapsed = now.elapsed();
                    if elapsed > std::time::Duration::from_secs(3) {
                        eprintln!("{} write callback took {}s", context, elapsed.as_secs());
                    }
                    res
                },
                Err(err) => Err(Error::Other(format!("{}: {}", err, context))),
            }
        })
    }

    #[inline]
    async fn with_read_spawn<F: 'static +  Send, T: 'static +  Send>(&self, context: &'static str, cb: F) -> FResult<T> where F: Send + Sync + FnOnce(&mut Connection) -> FResult<T> {
        let mut _sqlite_sucks = self.concurrency_control.read().await;
        let c = self.conn.clone();
        let url = self.url.clone();
        tokio::task::spawn_blocking(move || {
            let conn = c.get_or(|| Self::db(&url).map(|conn| {
                let _ = conn.busy_timeout(std::time::Duration::from_secs(4));
                RefCell::new(conn)
            }));
            match conn {
                Ok(conn) => {
                    let now = std::time::Instant::now();
                    let res = cb(&mut *conn.borrow_mut());
                    let elapsed = now.elapsed();
                    if elapsed > std::time::Duration::from_secs(3) {
                        eprintln!("{} write callback took {}s", context, elapsed.as_secs());
                    }
                    res
                },
                Err(err) => Err(Error::Other(format!("{} (in {})", err, context))),
            }
        }).await.expect("spawn")
    }

    #[inline]
    async fn with_write<F, T>(&self, context: &'static str, cb: F) -> FResult<T> where F: FnOnce(&Connection) -> FResult<T> {
        tokio::task::yield_now().await; // maybe there are read ops to do first?

        let mut _sqlite_sucks = self.concurrency_control.write().await;
        let mut conn = self.exclusive_conn.lock().await;
        tokio::task::block_in_place(|| {
            let conn = conn.get_or_insert_with(|| self.connect().expect("db setup"));

            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let now = std::time::Instant::now();
            let res = cb(&tx)?;
            tx.commit().map_err(|e| Error::DbCtx(e, context))?;
            let elapsed = now.elapsed();
            if elapsed > std::time::Duration::from_secs(3) {
                eprintln!("{} write callback took {}s", context, elapsed.as_secs());
            }
            Ok(res)
        })
    }

    fn connect(&self) -> std::result::Result<Connection, rusqlite::Error> {
        Self::db(&self.url)
    }

    #[inline]
    pub async fn latest_crate_update_timestamp(&self) -> FResult<Option<u32>> {
        self.with_read("latest_crate_update_timestamp", |conn| {
            Ok(none_rows(conn.query_row("SELECT max(created) FROM crate_versions", [], |row| row.get(0)))?)
        }).await
    }

    pub async fn crate_versions(&self, origin: &Origin) -> FResult<Vec<(String, u32)>> {
        self.with_read("crate_versions", |conn| {
            let mut q = conn.prepare("SELECT v.version, v.created FROM crates c JOIN crate_versions v ON v.crate_id = c.id WHERE c.origin = ?1")?;
            let res = q.query_map(&[&origin.to_str()], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?;
            Ok(res.collect::<Result<Vec<(String, u32)>>>()?)
        }).await
    }

    pub async fn before_index_latest(&self, origin: &Origin) -> FResult<()> {
        self.with_write("before_index_latest", |tx| {
            let next_timestamp = (Utc::now().timestamp() + 3600 * 24 * 3) as u32;
            let mut mark_updated = tx.prepare_cached("UPDATE crates SET next_update = ?2 WHERE origin = ?1")?;
            let args: &[&dyn ToSql] = &[&origin.to_str(), &next_timestamp];
            mark_updated.execute(args)?;
            Ok(())
        }).await
    }

    /// Add data of the latest version of a crate to the index
    /// Score is a ranking of a crate (0 = bad, 1 = great)
    pub async fn index_latest(&self, c: CrateVersionData<'_>) -> FResult<DbDerived> {
        let origin = c.origin.to_str();
        let mut insert_keyword = self.gather_crate_keywords(&c)?;

        let mut out = String::with_capacity(200);
        write!(&mut out, "https://lib.rs/{} ", if c.origin.is_crates_io() { c.origin.short_crate_name() } else { &origin }).unwrap();

        let next_timestamp = (Utc::now().timestamp() + 3600 * 24 * 31) as u32;

        c.category_slugs.iter().for_each(|k| debug_assert!(categories::CATEGORIES.from_slug(k).1, "'{}' must exist", k));

        self.with_write("insert_crate", |tx| {
            let mut insert_crate = tx.prepare_cached("INSERT OR IGNORE INTO crates (origin, recent_downloads, ranking) VALUES (?1, ?2, ?3)")?;
            let mut mark_updated = tx.prepare_cached("UPDATE crates SET next_update = ?2 WHERE id = ?1")?;
            let mut insert_repo = tx.prepare_cached("INSERT OR REPLACE INTO crate_repos (crate_id, repo) VALUES (?1, ?2)")?;
            let mut delete_repo = tx.prepare_cached("DELETE FROM crate_repos WHERE crate_id = ?1")?;
            let mut prev_categories = tx.prepare_cached("SELECT slug FROM categories WHERE crate_id = ?1")?;
            let mut clear_categories = tx.prepare_cached("DELETE FROM categories WHERE crate_id = ?1")?;
            let mut insert_category = tx.prepare_cached("INSERT OR IGNORE INTO categories (crate_id, slug, rank_weight, relevance_weight) VALUES (?1, ?2, ?3, ?4)")?;
            let mut get_crate_id = tx.prepare_cached("SELECT id, recent_downloads FROM crates WHERE origin = ?1")?;

            insert_crate.execute(&[&origin as &dyn ToSql, &0i32, &0i32])?;
            let (crate_id, downloads): (u32, u32) = get_crate_id.query_row(&[&origin], |row| Ok((row.get_unwrap(0), row.get_unwrap(1))))
                .map_err(|e| Error::DbCtx(e, "crate id"))?;
            let is_important_ish = downloads > 2000;

            if let Some(repo) = c.repository {
                let url = repo.canonical_git_url();
                insert_repo.execute(&[&crate_id as &dyn ToSql, &url.as_ref()]).map_err(|e| Error::DbCtx(e, "insert repo"))?;
            } else {
                delete_repo.execute(&[&crate_id])?;
            }

            let prev_c = prev_categories.query_map(&[&crate_id], |row| row.get(0))?.collect::<Result<Vec<Box<str>>,_>>()?;
            clear_categories.execute(&[&crate_id]).map_err(|e| Error::DbCtx(e, "clear cat"))?;
            insert_keyword.pre_commit(tx, crate_id)?;

            // guessing categories if needed
            let categories = {
                let keywords = insert_keyword.keywords.keys().map(|k| k.as_str()).collect();
                self.extract_crate_categories(tx, &c, &keywords, is_important_ish)?
            };

            let had_explicit_categories = categories.iter().any(|c| c.explicit);
            if !had_explicit_categories {
                if categories.is_empty() {
                    write!(&mut out, "[no categories] {:?}", prev_c)
                } else {
                    write!(&mut out, "[guessed]: ")
                }.unwrap();
            }

            for c in &categories {
                if !prev_c.contains(&c.slug) {
                    write!(&mut out, ">NEW {}, ", c.slug)
                } else {
                    write!(&mut out, ">{}, ", c.slug)
                }.unwrap();
            }

            for slug in &prev_c {
                if !categories.iter().any(|old| old.slug == *slug) {
                    write!(&mut out, ">LOST {}", slug).unwrap();
                }
            }

            if !had_explicit_categories {
                let mut tmp = insert_keyword.keywords.iter().collect::<Vec<_>>();
                tmp.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap());
                write!(&mut out, " #{}", tmp.into_iter().take(10).map(|(k, _)| k.as_str()).collect::<Vec<_>>().join(" #")).unwrap();
            }

            for CategoryCandidate {rank_weight, category_relevance, slug, explicit} in &categories {
                let args: &[&dyn ToSql] = &[&crate_id, slug, rank_weight, category_relevance];
                insert_category.execute(args).map_err(|e| Error::DbCtx(e, "insert cat"))?;
                if *explicit {
                    insert_keyword.add_raw(slug.to_string(), category_relevance/3., false);
                }
            }

            // yanked crates may contain garbage, or needlessly come up in similar crates
            // so knock all keywords' importance if it's yanked
            insert_keyword.commit(tx, crate_id, if c.source_data.is_yanked {0.1} else {1.})?;

            let package = c.manifest.package.as_ref().expect("package");
            let mut keywords: Vec<_> = package.keywords.iter().filter(|k| !k.is_empty()).map(|k| normalize_keyword(k)).collect();
            if keywords.is_empty() {
                keywords = Self::keywords_tx(tx, c.origin)?;
            }

            mark_updated.execute(&[&crate_id, &next_timestamp])?;
            println!("{}", out);
            Ok(DbDerived {
                categories: categories.iter().map(|cc| cc.slug.to_owned()).collect::<Vec<_>>(),
                keywords,
            })
        }).await
    }

    fn gather_crate_keywords(&self, c: &CrateVersionData) -> Result<KeywordInsert, Error> {
        let manifest = c.manifest;
        let package = manifest.package.as_ref().expect("package");

        let mut insert_keyword = KeywordInsert::new()?;
        let all_explicit_keywords = package.keywords.iter()
            .chain(c.source_data.github_keywords.iter().flatten());
        for (i, k) in all_explicit_keywords.enumerate() {
            let w: f64 = 100. / (6 + i * 2) as f64;
            insert_keyword.add(k, w, true);
        }
        for k in c.bad_categories {
            insert_keyword.add(k, 0.5, true);
        }
        for (i, k) in package.name.split(|c: char| !c.is_alphanumeric()).enumerate() {
            let w: f64 = 100. / (8 + i * 2) as f64;
            insert_keyword.add(k, w, false);
        }
        if let Some(l) = manifest.links() {
            insert_keyword.add(l.trim_start_matches("lib"), 0.54, false);
        }
        if let Some(lib) = c.source_data.lib_file.as_ref() {
            insert_keyword.add_raw(hex_hash(lib), 1., false);
        }
        if let Some(bin) = c.source_data.bin_file.as_ref() {
            insert_keyword.add_raw(hex_hash(bin), 1., false);
        }
        if let Some(Readme { markup: Markup::Markdown(txt) | Markup::Html(txt) | Markup::Rst(txt) , ..}) = c.source_data.readme.as_ref() {
            insert_keyword.add_raw(hex_hash(txt), 1., false);
        }
        insert_keyword.add_synonyms(&self.tag_synonyms);
        for (i, (w2, k)) in c.extracted_auto_keywords.iter().enumerate() {
            let w = *w2 as f64 * 150. / (80 + i) as f64;
            insert_keyword.add(k, w, false);
        }
        if let Some(url) = &package.homepage {
            if url.len() > 5 {
                insert_keyword.add_raw(format!("url:{url}"), 1., false); // crates sharing homepage are likely same project
            }
        }
        for feat in manifest.features.keys() {
            if feat != "default" && feat != "std" && feat != "nightly" {
                insert_keyword.add_raw(format!("feature:{}", feat), 0.55, false);
            }
        }
        if manifest.is_sys(c.source_data.has_buildrs || package.build.is_some()) {
            insert_keyword.add_raw("has:is_sys".into(), 0.01, false);
        }
        if manifest.is_proc_macro() {
            insert_keyword.add_raw("has:proc_macro".into(), 0.25, false);
        }
        if manifest.has_bin() {
            insert_keyword.add_raw("has:bin".into(), 0.01, false);
            if manifest.has_cargo_bin() {
                insert_keyword.add_raw("has:cargo-bin".into(), 0.2, false);
            }
        }
        if c.is_build {
            insert_keyword.add_raw("has:is_build".into(), 0.01, false);
        }
        if c.is_dev {
            insert_keyword.add_raw("has:is_dev".into(), 0.01, false);
        }
        for &(dep, weight) in c.deps_stats {
            insert_keyword.add_raw(format!("dep:{}", dep), weight.into(), false);
        }
        for (i, k) in c.authors.iter().filter_map(|a| a.email.as_ref().or(a.name.as_ref())).enumerate() {
            let w: f64 = 50. / (100 + i) as f64;
            insert_keyword.add_raw(k.into(), w, false);
        }
        if let Some(repo) = c.repository {
            let url = repo.canonical_git_url();
            insert_keyword.add_raw(format!("repo:{url}", ), 1., false); // crates in monorepo probably belong together
            if let Some(owner) = repo.host().owner_name() {
                // TODO: check if that's a GitHub org not user
                insert_keyword.add_raw(format!("by:{owner}"), if owner.ends_with("-rs") {1.} else {0.6}, false);
            }
        }
        Ok(insert_keyword)
    }

    /// (rank-relevance, relevance, slug)
    ///
    /// Rank relevance is normalized and biased towards one top category
    fn extract_crate_categories(&self, conn: &Connection, c: &CrateVersionData<'_>, keywords: &HashSet<&str>, is_important_ish: bool) -> FResult<Vec<CategoryCandidate>> {
        let had_explicit_categories = !c.category_slugs.is_empty();
        let candidates = if had_explicit_categories {
            let cat_w = 10.0 / (9.0 + c.category_slugs.len() as f64);
            c.category_slugs
                .iter()
                .enumerate()
                .map(|(i, slug)| {
                    let w = 100. / (5 + i) as f64 * cat_w;
                    ((&**slug).into(), w)
                })
                .collect()
        } else {
            let cat_w = 0.2 + 0.2 * c.manifest.package().keywords.len() as f64;
            let mut candidates = Self::candidate_crate_categories_tx(conn, c.origin)?;
            candidates.values_mut().for_each(|w| {
                *w = (*w * cat_w).min(0.99);
            });
            candidates
        };
        let threshold = if had_explicit_categories {0.01} else if is_important_ish {0.1} else {0.25};
        let limit = if had_explicit_categories {2} else {5};
        let categories = categories::adjusted_relevance(candidates, &keywords, threshold, limit);

        debug!("categories = {categories:?}");

        let max_weight = categories.iter().map(|&(w, _)| w)
            .max_by(|a, b| a.total_cmp(b))
            .unwrap_or(0.1)
            .max(0.1); // prevents div/0, ensures odd choices stay low

        let categories = categories.into_iter()
            .map(|(category_relevance, slug)| {
                let rank_weight = category_relevance / max_weight
                    * if category_relevance >= max_weight * 0.98 { 1. } else { 0.4 } // a crate is only in 1 category
                    * if category_relevance > 0.2 { 1. } else { 0.8 }; // keep bad category guesses out of sight
                CategoryCandidate {rank_weight, category_relevance, slug, explicit: had_explicit_categories}
            })
            .collect();

        Ok(categories)
    }

    /// Update crate <> repo association
    ///
    /// Along with `index_latest` it establishes 2-way association.
    /// It solves two problems:
    ///
    /// 1. A published crate can define what repository it is from, but *any* crate
    ///    can point to *any* repo, so that alone is not enough to prove it actually
    ///    is the crate's real repository.
    ///    Checking what crates are in the repository confirms or disproves the association.
    /// 2. A repository can contain more than one crate (monorepo). Search of the repo
    ///    finds location of the crate within the repo, adding extra precision to the
    ///    crate's repo URL (needed for e.g. GitHub README relative links), and adds
    ///    interesting relationship information for crates.
    pub async fn index_repo_crates(&self, repo: &Repo, paths_and_names: impl Iterator<Item = (impl AsRef<str>, impl AsRef<str>, impl AsRef<str>)>) -> FResult<()> {
        let repo = repo.canonical_git_url();
        self.with_write("index_repo_crates", |tx| {
            let mut insert_repo = tx.prepare_cached("INSERT OR IGNORE INTO repo_crates (repo, path, crate_name, revision) VALUES (?1, ?2, ?3, ?4)")?;
            for (path, name, revision) in paths_and_names {
                let name = name.as_ref();
                let path = path.as_ref();
                let revision = revision.as_ref();
                insert_repo.execute(&[&repo.as_ref(), &path, &name, &revision]).map_err(|e| Error::DbCtx(e, "repo rev insert"))?;
            }
            Ok(())
        }).await
    }

    pub async fn crates_in_repo(&self, repo: &Repo) -> FResult<Vec<Origin>> {
        self.with_read("crates_in_repo", |conn| {
            let mut q = conn.prepare_cached("
                SELECT crate_name
                FROM repo_crates
                WHERE repo = ?1
                ORDER BY path, crate_name LIMIT 10
            ")?;
            let q = q.query_map(&[&repo.canonical_git_url()], |r| {
                let s = r.get_ref_unwrap(0).as_str()?;
                crates_io_name(s)
            })?.filter_map(|r| r.map_err(|e| error!("crepo: {}", e)).ok());
            Ok(q.collect())
        }).await
    }

    /// Returns crate name (not origin)
    pub async fn parent_crate(&self, repo: &Repo, child_name: &str) -> FResult<Option<Origin>> {
        self.with_read("parent_crate", |conn| {
            let mut paths = conn.prepare_cached("SELECT path, crate_name FROM repo_crates WHERE repo = ?1 LIMIT 100")?;
            let mut paths: HashMap<String, String> = paths
                .query_map(&[&repo.canonical_git_url()], |r| Ok((r.get_unwrap(0), r.get_unwrap(1))))?
                .collect::<std::result::Result<_, _>>()?;

            if paths.len() < 2 {
                return Ok(None);
            }

            let child_path = if let Some(a) = paths.iter().find(|(_, child)| *child == child_name)
                .map(|(path, _)| path.to_owned()) {a} else {return Ok(None)};

            paths.remove(&child_path);
            let mut child_path = child_path.as_str();

            loop {
                child_path = child_path.rsplitn(2, '/').nth(1).unwrap_or("");
                if let Some(child) = paths.get(child_path) {
                    return Ok(Origin::try_from_crates_io_name(child));
                }
                if child_path.is_empty() {
                    // in these paths "" is the root
                    break;
                }
            }

            fn unprefix(s: &str) -> &str {
                if s.starts_with("rust-") || s.starts_with("rust_") {
                    return &s[5..];
                }
                if s.ends_with("-rs") || s.ends_with("_rs") {
                    return &s[..s.len() - 3];
                }
                if let Some(derusted) = s.strip_prefix("rust") {
                    return derusted;
                }
                s
            }

            Ok(if let Some(child) = repo.repo_name().and_then(|n| paths.get(n).or_else(|| paths.get(unprefix(n)))).filter(|c| *c != child_name) {
                Origin::try_from_crates_io_name(child)
            } else if let Some(child) = repo.owner_name().and_then(|n| paths.get(n).or_else(|| paths.get(unprefix(n)))).filter(|c| *c != child_name) {
                Origin::try_from_crates_io_name(child)
            } else {
                None
            })
        }).await
    }

    /// additions and removals
    pub async fn index_repo_changes(&self, repo: &Repo, changes: &[RepoChange]) -> FResult<()> {
        let repo = repo.canonical_git_url();
        self.with_write("index_repo_changes", |tx| {
            let mut insert_change = tx.prepare_cached("INSERT OR IGNORE INTO repo_changes (repo, crate_name, replacement, weight) VALUES (?1, ?2, ?3, ?4)")?;
            for change in changes {
                match *change {
                    RepoChange::Replaced { ref crate_name, ref replacement, weight } => {
                        assert!(Origin::is_valid_crate_name(crate_name));
                        assert!(Origin::is_valid_crate_name(replacement));
                        let args: &[&dyn ToSql] = &[&repo, &crate_name.as_str(), &Some(replacement.as_str()), &weight];
                        insert_change.execute(args)
                    },
                    RepoChange::Removed { ref crate_name, weight } => {
                        assert!(Origin::is_valid_crate_name(crate_name));
                        let args: &[&dyn ToSql] = &[&repo, &crate_name.as_str(), &(None as Option<&str>), &weight];
                        insert_change.execute(args)
                    },
                }?;
            }
            Ok(())
        }).await
    }

    pub async fn path_in_repo(&self, repo: &Repo, crate_name: &str) -> FResult<Option<String>> {
        self.with_read("path_in_repo", |conn| Self::path_in_repo_tx(conn, repo, crate_name)).await
    }

    pub fn path_in_repo_tx(conn: &Connection, repo: &Repo, crate_name: &str) -> FResult<Option<String>> {
        let repo = repo.canonical_git_url();
        let mut get_path = conn.prepare_cached("SELECT path FROM repo_crates WHERE repo = ?1 AND crate_name = ?2")?;
        let args: &[&dyn ToSql] = &[&repo, &crate_name];
        Ok(none_rows(get_path.query_row(args, |row| row.get(0))).map_err(|e| Error::DbCtx(e, "path_in_repo"))?)
    }

    /// Update download counts of the crate
    pub async fn index_versions(&self, all: &RichCrate, score: f64, downloads_per_month: Option<usize>) -> FResult<()> {
        self.with_write("index_versions", |tx| {
            let mut get_crate_id = tx.prepare_cached("SELECT id FROM crates WHERE origin = ?1")?;
            let mut insert_version = tx.prepare_cached("INSERT OR IGNORE INTO crate_versions (crate_id, version, created) VALUES (?1, ?2, ?3)")?;

            let origin = all.origin().to_str();
            let crate_id: u32 = get_crate_id.query_row(&[&origin], |row| row.get(0))
                .map_err(|e| Error::DbCtx(e, "the crate hasn't been indexed yet"))?;

            let recent_90_days = downloads_per_month.unwrap_or(0) as u32 * 3;
            let mut update_recent = tx.prepare_cached("UPDATE crates SET recent_downloads = ?1, ranking = ?2 WHERE id = ?3")?;
            let args: &[&dyn ToSql] = &[&recent_90_days, &score, &crate_id];
            update_recent.execute(args)?;

            for ver in all.versions() {
                if let Ok(timestamp) = DateTime::parse_from_rfc3339(&ver.created_at) {
                    let args: &[&dyn ToSql] = &[&crate_id, &ver.num, &timestamp.timestamp()];
                    insert_version.execute(args)?;
                }
            }
            Ok(())
        }).await
    }
/*
2020-05-06 13:50:44
*/
    /// github_id, created_at, number of crates
    pub async fn crate_all_owners(&self) -> FResult<Vec<CrateOwnerStat>> {
        self.with_read_spawn("all_owners", |tx| {
            let mut query = tx.prepare_cached("SELECT github_id, min(invited_at), count(*) FROM author_crates GROUP BY github_id")?;
            let q = query.query_map([], |row| {
                let s = row.get_ref(1)?.as_str()?;
                let y = s[0..4].parse().map_err(|e| error!("{} = {}", s, e)).ok();
                let m = s[5..7].parse().map_err(|e| error!("{} = {}", s, e)).ok();
                let d = s[8..10].parse().map_err(|e| error!("{} = {}", s, e)).ok();
                Ok((row.get(0)?, (y,m,d), row.get(2)?))
            })?;
            let res: Vec<_> = q.filter_map(|row| row.map_err(|e| error!("owner: {}", e)).ok()).filter_map(|row| {
                Some(CrateOwnerStat {
                    github_id: row.0,
                    created_at: ((row.1).0?, (row.1).1?, (row.1).2?),
                    num_crates: row.2,
                })
            }).collect();
            assert!(res.len() > 1000);
            Ok(res)
        }).await
    }

    /// Replaces entire author_crates table
    pub async fn index_crate_all_owners(&self, all_owners: &[(Origin, Vec<CrateOwner>)]) -> FResult<bool> {
        self.with_write("index_crate_owners", |tx| {
            let mut get_crate_id = tx.prepare_cached("SELECT id FROM crates WHERE origin = ?1")?;
            let mut insert = tx.prepare_cached("INSERT INTO author_crates(github_id, crate_id, invited_by_github_id, invited_at) VALUES(?1, ?2, ?3, ?4)")?;
            let mut wipe = tx.prepare_cached("DELETE FROM author_crates")?;
            wipe.execute([])?;

            for (origin, owners) in all_owners {
                let crate_id: u32 = match get_crate_id.query_row(&[&origin.to_str()], |row| row.get(0)) {
                    Ok(id) => id,
                    Err(rusqlite::Error::QueryReturnedNoRows) => continue,
                    Err(e) => return Err(e.into()),
                };
                for o in owners {
                    if let Some(github_id) = o.github_id {
                        let invited_by_github_id = match o.invited_by_github_id {
                            Some(id) if id != github_id => Some(id),
                            _ => None,
                        };
                        let args: &[&dyn ToSql] = &[&github_id, &crate_id, &invited_by_github_id, &o.invited_at];
                        insert.execute(args)?;
                    }
                }
            }
            Ok(true)
        }).await
    }

    pub async fn crates_of_author(&self, github_id: u32) -> FResult<Vec<CrateOwnerRow>> {
        self.with_read_spawn("crates_of_author", move |conn| {
            let mut query = conn.prepare_cached(r#"SELECT c.origin, ac.invited_by_github_id, ac.invited_at, max(cv.created), c.ranking
                FROM author_crates ac
                JOIN crate_versions cv USING(crate_id)
                JOIN crates c ON c.id = ac.crate_id
                WHERE ac.github_id = ?1
                GROUP BY ac.crate_id
                LIMIT 2000
            "#)?;
            let q = query.query_map(&[&github_id], |row| {
                let origin = Origin::from_str(row.get_ref_unwrap(0).as_str()?);
                let invited_by_github_id: Option<u32> = row.get_unwrap(1);
                let invited_at = row.get_ref_unwrap(2).as_str().ok().map(|d| match Utc.datetime_from_str(d, "%Y-%m-%d %H:%M:%S") {
                    Ok(d) => d,
                    Err(e) => panic!("Can't parse {}, because {}", d, e),
                });
                let latest_timestamp: u32 = row.get_unwrap(3);
                let crate_ranking: f64 = row.get_unwrap(4);
                Ok(CrateOwnerRow {
                    origin,
                    crate_ranking: crate_ranking as f32,
                    invited_by_github_id,
                    invited_at,
                    latest_release: DateTime::from_utc(NaiveDateTime::from_timestamp(latest_timestamp as _, 0), Utc),
                })
            })?;
            Ok(q.filter_map(|x| x.map_err(|e| error!("by owner: {}", e)).ok()).collect())
        }).await
    }

    fn candidate_crate_categories_tx(conn: &Connection, origin: &Origin) -> FResult<HashMap<Box<str>, f64>> {
        let mut query = conn.prepare_cached(r#"
        select cc.slug, sum(cc.relevance_weight * ck.weight * relk.relevance)/(8+count(*)) as w
        from (
        ----------------------------------
            select avg(ck.weight) * srck.weight / (8000+sum(ck.weight)) as relevance, ck.keyword_id
            -- find the crate to categorize
            from crates
            -- find its keywords
            join crate_keywords srck on crates.id = srck.crate_id
            -- find other crates using these keywords
            -- ck.weight * srck.weight gives strenght of the connection
            -- and divided by count(*) for tf-idf for relevance
            join crate_keywords ck on ck.keyword_id = srck.keyword_id
            where crates.origin = ?1
            group by ck.keyword_id
            order by 1 desc
        ----------------------------------
        ) as relk
        join crate_keywords ck on ck.keyword_id = relk.keyword_id
        join categories cc on cc.crate_id = ck.crate_id
        group by slug
        order by 2 desc
        limit 10"#)?;
        let candidates = query.query_map(&[&origin.to_str()], |row| Ok((row.get_unwrap(0), row.get_unwrap(1))))?;
        let candidates = candidates.collect::<std::result::Result<HashMap<_,_>, _>>()?;
        candidates.keys().for_each(|k| debug_assert!(categories::CATEGORIES.from_slug(k).1, "'{}' must exist", k));

        Ok(candidates)
    }

    /// Find most relevant keyword for the crate
    ///
    /// Returns (top n-th for the keyword, the keyword)
    pub async fn top_keyword(&self, origin: &Origin) -> FResult<Option<(u32, String)>> {
        let origin = origin.to_str();
        self.with_read_spawn("top_keyword", move |conn| {
            let mut query = conn.prepare_cached(r#"
            select top, keyword from (
                select count(*) as total, sum(case when oc.ranking >= c.ranking then 1 else 0 end) as top, k.keyword from crates c
                        join crate_keywords ck on ck.crate_id = c.id
                        join crate_keywords ock on ock.keyword_id = ck.keyword_id
                        join crates oc on ock.crate_id = oc.id
                        join keywords k on k.id = ck.keyword_id
                        where c.origin = ?1
                        and ck.explicit
                        and ock.explicit
                        and k.visible
                        group by ck.keyword_id
                        having count(*) >= 5
                ) as tmp
            order by top + (top+30.0)/total
            "#)?;
            Ok(none_rows(query.query_row(&[&origin], |row| Ok((row.get_unwrap(0), row.get_unwrap(1)))))?)
        }).await
    }

    /// Number of crates with a given keyword
    ///
    /// NB: must be lowercase
    pub async fn crates_with_keyword(&self, keyword: &str) -> FResult<u32> {
        self.with_read("crates_with_keyword", |conn| {
            let mut query = conn.prepare_cached("SELECT count(*) FROM crate_keywords
                WHERE explicit AND keyword_id = (SELECT id FROM keywords WHERE keyword = ?1)")?;
            Ok(none_rows(query.query_row(&[&keyword], |row| row.get(0)))?.unwrap_or(0))
        }).await
    }

    /// Sorted by most popular first
    pub async fn all_explicit_keywords(&self) -> FResult<Vec<String>> {
        self.with_read("allkw", |conn| {
            let mut query = conn.prepare_cached("SELECT k.keyword
                FROM keywords k JOIN crate_keywords ck ON (ck.keyword_id = k.id)
                WHERE k.visible
                GROUP BY k.id ORDER BY sum(ck.weight)*count(*) DESC
            ")?;
            let res = query.query_map([], |row| (row.get(0)))?;
            Ok(res.collect::<std::result::Result<_,_>>()?)
        }).await
    }

    /// Categories similar to the given category
    pub async fn related_categories(&self, slug: &str) -> FResult<Vec<String>> {
        self.with_read("related_categories", |conn| {
            let mut query = conn.prepare_cached(r#"
                select sum(c2.relevance_weight * c1.relevance_weight) as w, c2.slug
                from categories c1
                join categories c2 on c1.crate_id = c2.crate_id
                where c1.slug = ?1
                and c2.slug != c1.slug
                group by c2.slug
                having w > 250
                order by 1 desc
                limit 6
            "#)?;
            let res = query.query_map(&[&slug], |row| row.get(1))?;
            Ok(res.collect::<std::result::Result<_,_>>()?)
        }).await
    }

    pub async fn replacement_crates(&self, crate_name: &str) -> FResult<Vec<Origin>> {
        self.with_read("replacement_crates", |conn| {
            let mut query = conn.prepare_cached(r#"
                SELECT sum(weight) as w, replacement
                FROM repo_changes
                WHERE crate_name = ?1
                AND replacement IS NOT NULL
                GROUP BY replacement
                HAVING w > 20
                ORDER by 1 desc
                LIMIT 4
            "#)?;
            let res = query.query_map(&[&crate_name], |row| {
                let s = row.get_ref_unwrap(1).as_str()?;
                crates_io_name(s)
            })?;
            Ok(res.collect::<std::result::Result<_,_>>()?)
        }).await
    }

    /// f32 is ranking
    pub async fn related_crates(&self, origin: &Origin, min_recent_downloads: u32) -> FResult<Vec<(Origin, f32)>> {
        let origin = origin.to_str();
        self.with_read_spawn("related_crates", move |conn| {
            let mut query = conn.prepare_cached(r#"
                SELECT sum(k2.weight * k1.weight) as w, c2.origin, c2.ranking
                FROM crates c1
                JOIN crate_keywords k1 on k1.crate_id = c1.id
                JOIN crate_keywords k2 on k1.keyword_id = k2.keyword_id
                JOIN crates c2 on k2.crate_id = c2.id
                WHERE c1.origin = ?1
                AND k2.crate_id != c1.id
                AND c2.recent_downloads > ?2
                GROUP by k2.crate_id
                HAVING w > 200
                ORDER by 1 desc
                LIMIT 15
            "#)?;
            let args: &[&dyn ToSql] = &[&origin, &min_recent_downloads];
            let res = query.query_map(args, |row| {
                Ok((Origin::from_str(row.get_ref_unwrap(1).as_str()?), row.get_unwrap(2)))
            })?;
            Ok(res.collect::<std::result::Result<_,_>>()?)
        }).await
    }

    /// Find keywords that may be most relevant to the crate
    #[inline]
    pub async fn keywords(&self, origin: &Origin) -> FResult<Vec<String>> {
        let origin = origin.clone();
        self.with_read_spawn("keywords", move |conn| {
            Self::keywords_tx(conn, &origin)
        }).await
    }

    pub fn keywords_tx(conn: &Connection, origin: &Origin) -> FResult<Vec<String>> {
        let mut query = conn.prepare_cached(r#"
            select avg(ck.weight) * srck.weight, k.keyword
            -- find the crate to categorize
            from crates
            -- find its keywords
            join crate_keywords srck on crates.id = srck.crate_id
            -- find other crates using these keywords
            -- ck.weight * srck.weight gives strenght of the connection
            -- and divided by count(*) for tf-idf for relevance
            join crate_keywords ck on ck.keyword_id = srck.keyword_id
            join keywords k on k.id = ck.keyword_id
            -- ignore keywords equal categories
            left join categories c on c.slug = k.keyword
            where crates.origin = ?1
            and k.visible
            and c.slug is null
            group by ck.keyword_id
            order by 1 desc
            limit 10
            "#)?;
        let res: Vec<(f64, String)> = query.query_map(&[&origin.to_str()], |row| Ok((row.get_unwrap(0), row.get_unwrap(1))))?
            .collect::<std::result::Result<_,_>>()?;
        let min_score = res.get(0).map_or(0., |(rel,_)|rel/20.);
        let crate_name = origin.short_crate_name();
        Ok(res.into_iter().filter_map(move |(rel,k)|{
            if rel >= min_score && k != crate_name {
                Some(k)
            } else {
                None
            }
        }).collect())
    }

    /// Find most relevant/popular keywords in the category
    pub async fn top_keywords_in_category(&self, slug: &str) -> FResult<Vec<String>> {
        self.with_read("top_keywords_in_category", |conn| {
            let mut query = conn.prepare_cached(r#"
                select sum(k.weight * c.relevance_weight), kk.keyword from categories c
                    join crate_keywords k using(crate_id)
                    join keywords kk on kk.id = k.keyword_id
                    where explicit and c.slug = ?1
                    group by k.keyword_id
                    having sum(k.weight) > 7 and count(*) >= 4
                    order by 1 desc
                    limit 12
            "#)?;
            let q = query.query_map(&[&slug], |row| row.get(1))?;
            let q = q.filter_map(|r| r.map_err(|e| error!("kw: {}", e)).ok());
            Ok(q.collect())
        }).await
    }

    /// Most popular crates in the category
    /// Returns weight/importance as well
    pub async fn top_crates_in_category_partially_ranked(&self, slug: &str, limit: u32) -> FResult<Vec<(Origin, f64)>> {
        self.with_read("top_crates_in_category_partially_ranked", |conn| {
            // sort by relevance to the category, downrank for being crappy (later also downranked for being removed from crates)
            // low number of downloads is mostly by rank, rather than downloads
            let mut query = conn.prepare_cached(
            "SELECT k.origin, (k.ranking * c.rank_weight) as w
                FROM categories c
                JOIN crates k on c.crate_id = k.id
                WHERE c.slug = ?1
                ORDER by w desc
                LIMIT ?2",
            )?;
            let args: &[&dyn ToSql] = &[&slug, &limit];
            let q = query.query_map(args, |row| {
                Ok((Origin::from_str(row.get_ref_unwrap(0).as_str()?), row.get_unwrap(1)))
            })?;
            let q = q.filter_map(|r| r.map_err(|e| error!("top: {}", e)).ok());
            Ok(q.collect())
        }).await
    }

    pub async fn top_crates_uncategorized(&self, limit: u32) -> FResult<Vec<(Origin, f64)>> {
        self.with_read("top_crates_uncategorized", |conn| {
            // sort by relevance to the category, downrank for being crappy (later also downranked for being removed from crates)
            // low number of downloads is mostly by rank, rather than downloads
            let mut query = conn.prepare_cached(
            "SELECT k.origin, k.ranking as w
                FROM crates k
                LEFT JOIN categories c on c.crate_id = k.id
                WHERE c.slug IS NULL
                ORDER by w desc
                LIMIT ?1",
            )?;
            let args: &[&dyn ToSql] = &[&limit];
            let q = query.query_map(args, |row| {
                Ok((Origin::from_str(row.get_ref_unwrap(0).as_str()?), row.get_unwrap(1)))
            })?;
            let q = q.filter_map(|r| r.map_err(|e| error!("top: {}", e)).ok());
            Ok(q.collect())
        }).await
    }

    /// Newly added or updated crates in the category, filtered to include only not-terrible crates
    ///
    /// Returns `origin` strings
    pub async fn recently_updated_crates_in_category(&self, slug: &str) -> FResult<Vec<Origin>> {
        let slug = slug.to_owned();
        self.with_read_spawn("recently_updated_crates_in_category", move |conn| {
            let mut query = conn.prepare_cached(r#"
                select max(created) + 3600*24*7 * c.rank_weight * k.ranking, -- week*rank ~= best this week
                    k.origin
                    from categories c
                    join crate_versions v using (crate_id)
                    join crates k on v.crate_id = k.id
                    where c.slug = ?1
                        and k.ranking > 0.33 -- skip spam
                    group by v.crate_id
                    having count(*) > 1 -- so these are updates, not new releases
                    order by 1 desc
                    limit 20
            "#)?;
            let q = query.query_map(&[&slug], |row| {
                Ok(Origin::from_str(row.get_ref_unwrap(1).as_str()?))
            })?;
            let q = q.filter_map(|r| r.map_err(|e| error!("upd: {}", e)).ok());
            Ok(q.collect())
        }).await
    }

    /// Newly added or updated crates in any category
    ///
    /// Returns `origin` strings
    pub async fn recently_updated_crates(&self, limit: u32) -> FResult<Vec<(Origin, f64)>> {
        self.with_read("recently_updated_crates", |conn| {
            let mut query = conn.prepare_cached(r#"
                select max(created) + 3600*24*7 * k.ranking, -- week*rank ~= best this week
                    k.ranking,
                    k.origin
                    from crate_versions v
                    join crates k on v.crate_id = k.id
                    group by v.crate_id
                    having count(*) > 1 -- so these are updates, not new releases
                    order by 1 desc
                    limit ?1
            "#)?;
            let q = query.query_map(&[&limit], |row| {
                let origin = Origin::from_str(row.get_ref_unwrap(2).as_str()?);
                Ok((origin, row.get(1)?))
            })?;
            let q = q.filter_map(|r| r.map_err(|e| error!("ruc: {}", e)).ok());
            Ok(q.collect())
        }).await
    }

    /// Newly added or updated crates in any category
    ///
    /// Returns `origin` strings
    pub async fn most_downloaded_crates(&self, limit: u32) -> FResult<Vec<(Origin, u32)>> {
        self.with_read("recently_updated_crates", |conn| {
            let mut query = conn.prepare_cached(r#"
                select recent_downloads, origin from crates order by 1 desc limit ?1
            "#)?;
            let q = query.query_map(&[&limit], |row| {
                let origin = Origin::from_str(row.get_ref_unwrap(1).as_str()?);
                Ok((origin, row.get(0)?))
            })?;
            let q = q.filter_map(|r| r.map_err(|e| error!("mdc: {}", e)).ok());
            Ok(q.collect())
        }).await
    }

    #[inline]
    pub async fn crate_rank(&self, origin: &Origin) -> FResult<f64> {
        self.with_read("crate_rank", |conn| {
            let mut query = conn.prepare_cached("SELECT ranking FROM crates WHERE origin = ?1)")?;
            Ok(none_rows(query.query_row(&[&origin.to_str()], |row| row.get(0)))?.unwrap_or(0.))
        }).await
    }

    /// List of all notable crates
    /// Returns origin, rank, last updated unix timestamp
    pub async fn sitemap_crates(&self) -> FResult<Vec<(Origin, f64, i64)>> {
        self.with_read("sitemap_crates", |conn| {
            let mut q = conn.prepare(r#"
                SELECT origin, ranking, max(created) as last_update
                FROM crates c
                JOIN crate_versions v ON c.id = v.crate_id
                WHERE ranking > 0.25
                GROUP BY c.id
            "#)?;
            let q = q.query_map([], |row| -> Result<(Origin, f64, i64)> {
                Ok((Origin::from_str(row.get_ref_unwrap(0).as_str()?), row.get_unwrap(1), row.get_unwrap(2)))
            })?.filter_map(|r| r.map_err(|e| error!("sitemap: {}", e)).ok());
            Ok(q.collect())
        }).await
    }

    /// Number of crates in every category
    pub async fn category_crate_counts(&self) -> FResult<HashMap<String, (u32, f64)>> {
        self.with_read("category_crate_counts", |conn| {
            let mut q = conn.prepare(r#"
                select c.slug, count(*), sum(rank_weight) from categories c group by c.slug
            "#)?;
            let q = q.query_map([], |row| -> Result<(String, (u32, f64))> {
                Ok((row.get_unwrap(0), (row.get_unwrap(1), row.get_unwrap(2))))
            })?.filter_map(|r| r.map_err(|e| error!("counts: {}", e)).ok());
            Ok(q.collect())
        }).await
    }

    /// Crates overdue for an update
    pub async fn crates_to_reindex(&self) -> FResult<Vec<Origin>> {
        self.with_read("crates_to_reindex", |conn| {
            let mut q = conn.prepare("SELECT origin FROM crates WHERE next_update < ?1 LIMIT 1000")?;
            let timestamp = Utc::now().timestamp() as u32;
            let q = q.query_map(&[&timestamp], |r| {
                let s = r.get_ref_unwrap(0).as_str()?;
                Ok(Origin::from_str(s))
            })?.filter_map(|r| r.map_err(|e| error!("reindx: {}", e)).ok());
            Ok(q.collect())
        }).await
    }

    pub async fn delete_crate(&self, origin: &Origin) -> FResult<()> {
        self.with_write("delete", |conn| {
            let origin_str = origin.to_str();
            let mut q = conn.prepare("DELETE from categories WHERE crate_id in (SELECT id FROM crates WHERE origin = ?1 LIMIT 1)")?;
            q.execute([&origin_str])?;
            let mut q = conn.prepare("DELETE from crate_keywords WHERE crate_id in (SELECT id FROM crates WHERE origin = ?1 LIMIT 1)")?;
            q.execute([&origin_str])?;
            let mut q = conn.prepare("DELETE from crate_repos WHERE crate_id in (SELECT id FROM crates WHERE origin = ?1 LIMIT 1)")?;
            q.execute([&origin_str])?;
            let mut q = conn.prepare("DELETE from crate_versions WHERE crate_id in (SELECT id FROM crates WHERE origin = ?1 LIMIT 1)")?;
            q.execute([&origin_str])?;
            let mut q = conn.prepare("DELETE from crates WHERE origin = ?1")?;
            q.execute([&origin_str])?;
            Ok(())
        }).await
    }
}

pub enum RepoChange {
    Removed { crate_name: String, weight: f64 },
    Replaced { crate_name: String, replacement: String, weight: f64 },
}

pub struct KeywordInsert {
    /// k => (weight, explicit)
    keywords: HashMap<String, (f64, bool)>,
    ready: bool,
}

impl KeywordInsert {
    pub fn new() -> FResult<Self> {
        Ok(Self {
            keywords: HashMap::new(),
            ready: false,
        })
    }

    pub fn add(&mut self, word: &str, mut weight: f64, visible: bool) {
        let word = word
            .trim_matches(|c: char| !c.is_alphanumeric())
            .trim_end_matches("-rs")
            .trim_start_matches("rust-");
        if word.is_empty() || weight <= 0.000001 {
            return;
        }
        let word = normalize_keyword(word);
        if word == "rust" || word == "rs" {
            return;
        }
        if STOPWORDS.contains(word.as_str()) {
            weight *= 0.1;
        }
        self.add_raw(word, weight, visible);
    }

    pub fn add_raw(&mut self, word: String, weight: f64, visible: bool) {
        if word.is_empty() {
            return;
        }
        let k = self.keywords.entry(word).or_insert((weight, visible));
        if k.0 < weight {k.0 = weight}
        if visible {k.1 = visible}
    }

    pub fn add_synonyms(&mut self, tag_synonyms: &Synonyms) {
        let mut to_add = Vec::new();
        for (k, &(v, _)) in self.keywords.iter() {
            if let Some((k, v)) = self.get_synonym(tag_synonyms, k, v) {
                if let Some((k2, v2)) = self.get_synonym(tag_synonyms, &k, v) {
                    to_add.push((k2, v2));
                }
                to_add.push((k, v));
            }
        }
        for (s, v) in to_add {
            self.keywords.entry(s).or_insert((v, false));
        }
    }

    fn get_synonym(&self, tag_synonyms: &Synonyms, k: &str, v: f64) -> Option<(String, f64)> {
        let (synonym, relevance) = tag_synonyms.get(k)?;
        if self.keywords.get(synonym).is_some() {
            return None;
        }
        Some((normalize_keyword(synonym), v * relevance.min(0.8) as f64))
    }

    /// Clears old keywords from the db
    pub fn pre_commit(&mut self, conn: &Connection, crate_id: u32) -> FResult<()> {
        let mut clear_keywords = conn.prepare_cached("DELETE FROM crate_keywords WHERE crate_id = ?1")?;
        clear_keywords.execute(&[&crate_id])?;
        self.ready = true;
        Ok(())
    }

    /// Call pre_commit first
    pub fn commit(mut self, conn: &Connection, crate_id: u32, overall_weight: f64) -> FResult<()> {
        assert!(self.ready);

        let mut select_id = conn.prepare_cached("SELECT id, visible FROM keywords WHERE keyword = ?1")?;
        let mut insert_name = conn.prepare_cached("INSERT OR IGNORE INTO keywords (keyword, visible) VALUES (?1, ?2)")?;
        let mut insert_value = conn.prepare_cached("INSERT OR IGNORE INTO crate_keywords(keyword_id, crate_id, weight, explicit)
            VALUES (?1, ?2, ?3, ?4)")?;
        let mut make_visible = conn.prepare_cached("UPDATE keywords SET visible = 1 WHERE id = ?1")?;

        for (cond, stopwords) in COND_STOPWORDS.iter() {
            if self.keywords.get(*cond).is_some() {
                match stopwords {
                    Some(stopwords) => for stop in stopwords.iter() {
                        if let Some(k) = self.keywords.get_mut(*stop) {
                            k.0 /= 3.0;
                        }
                    },
                    None => for (_, (ref mut w, _)) in self.keywords.iter_mut().filter(|(k, _)| k != cond) {
                        *w /= 2.0;
                    }
                }
            }
        }

        for (word, (weight, visible)) in self.keywords {
            let args: &[&dyn ToSql] = &[&word, if visible { &1i32 } else { &0i32 }];
            insert_name.execute(args)?;
            let (keyword_id, old_vis): (u32, u32) = select_id.query_row(&[&word], |r| Ok((r.get_unwrap(0), r.get_unwrap(1))))?;
            if visible && old_vis == 0 {
                make_visible.execute(&[&keyword_id])?;
            }
            let weight = weight * overall_weight;
            let args: &[&dyn ToSql] = &[&keyword_id, &crate_id, &weight, if visible { &1i32 } else { &0i32 }];
            insert_value.execute(args)?;
        }
        Ok(())
    }
}

fn crates_io_name(name: &str) -> std::result::Result<Origin, rusqlite::Error> {
    Origin::try_from_crates_io_name(name)
                    .ok_or_else(|| rusqlite::Error::ToSqlConversionFailure(format!("bad name {}", name).into()))
}

fn hex_hash(s: &str) -> String {
    format!("*{}", blake3::hash(s.as_bytes()).to_hex())
}

struct CategoryCandidate {
    rank_weight: f64,
    category_relevance: f64,
    slug: Box<str>,
    /// false if guessed
    explicit: bool,
}

#[derive(Debug)]
pub struct CrateOwnerRow {
    pub origin: Origin,
    pub crate_ranking: f32,
    pub invited_by_github_id: Option<u32>,
    pub invited_at: Option<DateTime<Utc>>,
    pub latest_release: DateTime<Utc>,
}

#[inline]
fn none_rows<T>(res: std::result::Result<T, rusqlite::Error>) -> std::result::Result<Option<T>, rusqlite::Error> {
    match res {
        Ok(dat) => Ok(Some(dat)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(err) => Err(err),
    }
}

#[test]
fn try_indexing() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let f = rt.spawn(async move {
    let t = tempfile::NamedTempFile::new().unwrap();

    let sy = categories::Synonyms::new(Path::new("../data/")).unwrap();
    let db = CrateDb::new_with_synonyms(t.as_ref(), sy).unwrap();
    let origin = Origin::from_crates_io_name("cratedbtest");
    let source_data = CrateVersionSourceData {
        capitalized_name: "captname".into(),
        ..Default::default()
    };
    let manifest = cargo_toml::Manifest::from_str(r#"[package]
name="crates-indexing-unit-test-hi"
version="1.2.3"
keywords = ["test-CRATE"]
categories = ["1", "two", "GAMES", "science", "::science::math::"]
"#).unwrap();
    let new_derived = db.index_latest(CrateVersionData {
        source_data: &source_data,
        manifest: &manifest,
        origin: &origin,
        deps_stats: &[],
        is_build: false,
        is_dev: false,
        authors: &[],
        category_slugs: &["science".into()],
        bad_categories: &["face".into(), "book".into()],
        repository: None,
        cache_key: 1,
        extracted_auto_keywords: Vec::new(),
    }).await.unwrap();
    assert_eq!(1, db.crates_with_keyword("test-crate").await.unwrap());
    assert_eq!(new_derived.categories.len(), 1); // uses slugs, not manifest
    assert_eq!(&*new_derived.categories[0], "science");
    assert_eq!(new_derived.keywords.len(), 1); // only keywords from manifest
    });
    rt.block_on(f).unwrap();
}

