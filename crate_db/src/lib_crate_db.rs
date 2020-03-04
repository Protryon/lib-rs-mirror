use categories;
use chrono::prelude::*;
use failure::*;
use heck::KebabCase;
use rich_crate::CrateVersionSourceData;
use rich_crate::CrateOwner;
use rich_crate::Derived;
use rich_crate::Manifest;
use rich_crate::ManifestExt;
use rich_crate::Markup;
use rich_crate::Origin;
use rich_crate::Readme;
use rich_crate::Repo;
use rich_crate::RichCrate;
use rusqlite::types::ToSql;
use rusqlite::NO_PARAMS;
use rusqlite::*;
use std::borrow::Cow;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt::Write;
use std::fs;
use std::path::Path;
use thread_local::ThreadLocal;
use tokio::sync::{Mutex, RwLock};
type FResult<T> = std::result::Result<T, failure::Error>;

pub mod builddb;

mod schema;
mod stopwords;
use crate::stopwords::{COND_STOPWORDS, STOPWORDS};

pub struct CrateDb {
    url: String,
    // Sqlite is awful with "database table is locked"
    concurrency_control: RwLock<()>,
    conn: ThreadLocal<std::result::Result<RefCell<Connection>, rusqlite::Error>>,
    exclusive_conn: Mutex<Option<Connection>>,
    tag_synonyms: HashMap<Box<str>, (Box<str>, u8)>,
}

pub struct CrateVersionData<'a> {
    pub origin: &'a Origin,
    pub derived: &'a CrateVersionSourceData,
    pub manifest: &'a Manifest,
    pub deps_stats: &'a [(&'a str, f32)],
    pub is_build: bool,
    pub is_dev: bool,
    pub authors: &'a [rich_crate::Author],
    pub category_slugs: &'a [Cow<'a, str>],
    pub repository: Option<&'a Repo>,
    pub extracted_auto_keywords: Vec<(f32, String)>,
}

impl CrateDb {
    /// Path to sqlite db file to create/update
    pub fn new(path: impl AsRef<Path>) -> FResult<Self> {
        let path = path.as_ref();
        Self::new_with_synonyms(path, &path.with_file_name("tag-synonyms.csv"))
    }

    pub fn new_with_synonyms(path: &Path, synonyms: &Path) -> FResult<Self> {
        let tag_synonyms = fs::read_to_string(synonyms)?;
        let tag_synonyms = tag_synonyms.lines()
            .filter(|l| !l.starts_with('#'))
            .map(|l| {
                let mut cols = l.splitn(3, ',');
                let score: u8 = cols.next().unwrap().parse().unwrap();
                let find = cols.next().unwrap();
                let replace = cols.next().unwrap();
                (find.into(), (replace.into(), score))
            })
            .collect();
        Ok(Self {
            tag_synonyms,
            url: format!("file:{}?cache=shared", path.display()),
            conn: ThreadLocal::new(),
            concurrency_control: RwLock::new(()),
            exclusive_conn: Mutex::new(None),
        })
    }

    #[inline]
    async fn with_read<F, T>(&self, context: &'static str, cb: F) -> FResult<T> where F: FnOnce(&mut Connection) -> FResult<T> {
        let mut _sqlite_sucks = self.concurrency_control.read().await;

        tokio::task::block_in_place(|| {
            let conn = self.conn.get_or(|| self.connect().map(|conn| {
                let _ = conn.busy_timeout(std::time::Duration::from_secs(3));
                RefCell::new(conn)
            }));
            match conn {
                Ok(conn) => Ok(cb(&mut *conn.borrow_mut()).context(context)?),
                Err(err) => bail!("{} (in {})", err, context),
            }
        })
    }

    #[inline]
    async fn with_write<F, T>(&self, context: &'static str, cb: F) -> FResult<T> where F: FnOnce(&Connection) -> FResult<T> {
        let mut _sqlite_sucks = self.concurrency_control.write().await;

        let mut conn = self.exclusive_conn.lock().await;
        tokio::task::block_in_place(|| {
            let conn = conn.get_or_insert_with(|| self.connect().expect("db setup"));

            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let res = cb(&tx).context(context)?;
            tx.commit().context(context)?;
            Ok(res)
        })
    }

    fn connect(&self) -> std::result::Result<Connection, rusqlite::Error> {
        let db = Self::db(&self.url)?;
        db.execute_batch("
            PRAGMA cache_size = 500000;
            PRAGMA threads = 4;
            PRAGMA synchronous = 0;")?;
        Ok(db)
    }

    pub async fn rich_crate_version_data(&self, origin: &Origin) -> FResult<(Manifest, Derived)> {
        struct Row {
            capitalized_name: String,
            crate_compressed_size: u32,
            crate_decompressed_size: u32,
            lib_file: Option<String>,
            has_buildrs: bool,
            is_nightly: bool,
            is_yanked: bool,
            has_code_of_conduct: bool,
        }
        self.with_read("rich_crate_version_data", |conn| {
            let args: &[&dyn ToSql] = &[&origin.to_str()];
            let (manifest, readme, row, language_stats): (Manifest, _, Row, _) = conn.query_row("SELECT * FROM crates c JOIN crate_derived d ON (c.id = d.crate_id)
                WHERE origin = ?1", args, |row| {
                    let readme = match row.get_raw("readme_format").as_str() {
                        Err(_) => None,
                        Ok(ty) => {
                            let txt: String = row.get("readme")?;
                            Some(Readme {
                                markup: match ty {
                                    "html" => rich_crate::Markup::Html(txt),
                                    "md" => rich_crate::Markup::Markdown(txt),
                                    "rst" => rich_crate::Markup::Rst(txt),
                                    _ => unimplemented!(),
                                },
                                base_url: row.get("readme_base_url")?,
                                base_image_url: row.get("readme_base_image_url")?,
                            })
                        },
                    };

                    let manifest = row.get_raw("manifest").as_blob().expect("manifest col");
                    let manifest = rmp_serde::from_slice(manifest).expect("manifest parse");
                    let language_stats = row.get_raw("language_stats").as_blob().expect("language_stats col");
                    let language_stats = rmp_serde::from_slice(language_stats).expect("language_stats parse");
                    Ok((manifest, readme, Row {
                        lib_file: row.get("lib_file")?,
                        capitalized_name: row.get("capitalized_name")?,
                        crate_compressed_size: row.get("crate_compressed_size")?,
                        crate_decompressed_size: row.get("crate_decompressed_size")?,
                        has_buildrs: row.get("has_buildrs")?,
                        is_nightly: row.get("is_nightly")?,
                        is_yanked: row.get("is_yanked")?,
                        has_code_of_conduct: row.get("has_code_of_conduct")?,
                    }, language_stats))
                })?;

            let package = manifest.package.as_ref().expect("package in manifest");
            let name = &package.name;
            let maybe_repo = package.repository.as_ref().and_then(|r| Repo::new(r).ok());
            let path_in_repo = match maybe_repo.as_ref() {
                Some(repo) => self.path_in_repo_tx(conn, repo, name)?,
                None => None,
            };

            let keywords: HashSet<_> = package.keywords.iter().filter(|k| !k.is_empty()).map(|s| s.to_kebab_case()).collect();
            let keywords_derived = if keywords.is_empty() {
                Some(self.keywords_tx(conn, &origin).context("keywordsdb2")?)
            } else {
                None
            };
            let categories = if categories::Categories::fixed_category_slugs(&package.categories).is_empty() {
                Some(self.crate_categories_tx(conn, &origin, &keywords, 0.1).context("catdb")?
                    .into_iter().map(|(_, c)| c).collect())
            } else {
                None
            };

            Ok((manifest, Derived {
                path_in_repo,
                readme,
                categories,
                capitalized_name: row.capitalized_name,
                crate_compressed_size: row.crate_compressed_size,
                crate_decompressed_size: row.crate_decompressed_size,
                keywords: keywords_derived,
                lib_file: row.lib_file,
                has_buildrs: row.has_buildrs,
                is_nightly: row.is_nightly,
                is_yanked: row.is_yanked,
                has_code_of_conduct: row.has_code_of_conduct,
                language_stats,
            }))
        }).await
    }

    #[inline]
    pub async fn latest_crate_update_timestamp(&self) -> FResult<Option<u32>> {
        self.with_read("latest_crate_update_timestamp", |conn| {
            let nope: [u8; 0] = [];
            Ok(none_rows(conn.query_row("SELECT max(created) FROM crate_versions", nope.iter(), |row| row.get(0)))?)
        }).await
    }

    pub async fn crate_versions(&self, origin: &Origin) -> FResult<Vec<(String, u32)>> {
        self.with_read("crate_versions", |conn| {
            let mut q = conn.prepare("SELECT v.version, v.created FROM crates c JOIN crate_versions v ON v.crate_id = c.id WHERE c.origin = ?1")?;
            let res = q.query_map(&[&origin.to_str()][..], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?;
            Ok(res.collect::<Result<Vec<(String, u32)>>>()?)
        }).await
    }

    /// Add data of the latest version of a crate to the index
    /// Score is a ranking of a crate (0 = bad, 1 = great)
    pub async fn index_latest(&self, c: CrateVersionData<'_>) -> FResult<()> {
        let origin = c.origin.to_str();

        let manifest = &c.manifest;
        let package = manifest.package.as_ref().expect("package");
        let mut insert_keyword = KeywordInsert::new()?;
        let all_explicit_keywords = package.keywords.iter()
            .chain(c.derived.github_keywords.iter().flatten());
        for (i, k) in all_explicit_keywords.enumerate() {
            let mut w: f64 = 100. / (6 + i * 2) as f64;
            if STOPWORDS.get(k.as_str()).is_some() {
                w *= 0.6;
            }
            insert_keyword.add(&k, w, true);
        }

        for (i, k) in package.name.split(|c: char| !c.is_alphanumeric()).enumerate() {
            let w: f64 = 100. / (8 + i * 2) as f64;
            insert_keyword.add(k, w, false);
        }

        if let Some(l) = manifest.links() {
            insert_keyword.add(l.trim_start_matches("lib"), 0.54, false);
        }

        // order is important. SO's synonyms are very keyword-specific and would
        // add nonsense keywords if applied to freeform text
        insert_keyword.add_synonyms(&self.tag_synonyms);

        for (i, (w2, k)) in c.extracted_auto_keywords.iter().enumerate() {
            let w = *w2 as f64 * 150. / (80 + i) as f64;
            insert_keyword.add(&k, w, false);
        }

        for feat in manifest.features.keys() {
            if feat != "default" && feat != "std" && feat != "nightly" {
                insert_keyword.add(&format!("feature:{}", feat), 0.55, false);
            }
        }
        if manifest.is_sys(c.derived.has_buildrs || package.build.is_some()) {
            insert_keyword.add("has:is_sys", 0.01, false);
        }
        if manifest.is_proc_macro() {
            insert_keyword.add("has:proc_macro", 0.25, false);
        }
        if manifest.has_bin() {
            insert_keyword.add("has:bin", 0.01, false);
            if manifest.has_cargo_bin() {
                insert_keyword.add("has:cargo-bin", 0.2, false);
            }
        }
        if c.is_build {
            insert_keyword.add("has:is_build", 0.01, false);
        }
        if c.is_dev {
            insert_keyword.add("has:is_dev", 0.01, false);
        }

        for &(dep, weight) in c.deps_stats {
            insert_keyword.add(&format!("dep:{}", dep), (weight / 2.0).into(), false);
        }

        let mut out = String::with_capacity(200);
        write!(&mut out, "{}: ", origin)?;

        let next_timestamp = (Utc::now().timestamp() + 3600 * 24 * 31) as u32;

        self.with_write("insert_crate", |tx| {
            let mut insert_crate = tx.prepare_cached("INSERT OR IGNORE INTO crates (origin, recent_downloads, ranking) VALUES (?1, ?2, ?3)")?;
            let mut mark_updated = tx.prepare_cached("UPDATE crates SET next_update = ?2 WHERE id = ?1")?;
            let mut insert_repo = tx.prepare_cached("INSERT OR REPLACE INTO crate_repos (crate_id, repo) VALUES (?1, ?2)")?;
            let mut delete_repo = tx.prepare_cached("DELETE FROM crate_repos WHERE crate_id = ?1")?;
            let mut clear_categories = tx.prepare_cached("DELETE FROM categories WHERE crate_id = ?1")?;
            let mut insert_category = tx.prepare_cached("INSERT OR IGNORE INTO categories (crate_id, slug, rank_weight, relevance_weight) VALUES (?1, ?2, ?3, ?4)")?;
            let mut get_crate_id = tx.prepare_cached("SELECT id, recent_downloads FROM crates WHERE origin = ?1")?;
            let mut insert_derived = tx.prepare_cached("INSERT OR REPLACE INTO crate_derived (
                 crate_id, readme, readme_format, readme_base_url, readme_base_image_url, crate_compressed_size, crate_decompressed_size, capitalized_name, lib_file, has_buildrs, is_nightly, is_yanked, has_code_of_conduct, manifest, language_stats)
                VALUES (
                :crate_id,:readme,:readme_format,:readme_base_url,:readme_base_image_url,:crate_compressed_size,:crate_decompressed_size,:capitalized_name,:lib_file,:has_buildrs,:is_nightly,:is_yanked,:has_code_of_conduct,:manifest,:language_stats)
                ")?;

            let args: &[&dyn ToSql] = &[&origin, &0, &0];
            insert_crate.execute(args).context("insert crate")?;
            let (crate_id, downloads): (u32, u32) = get_crate_id.query_row(&[&origin], |row| Ok((row.get_unwrap(0), row.get_unwrap(1)))).context("crate_id")?;

            let (readme, readme_format, readme_base_url, readme_base_image_url) = match &c.derived.readme {
                Some(Readme {base_url, base_image_url, markup}) => {
                    let (markup, format) = match markup {
                        Markup::Html(s) => (s, "html"),
                        Markup::Markdown(s) => (s, "md"),
                        Markup::Rst(s) => (s, "rst"),
                    };
                    (Some(markup), Some(format), Some(base_url), Some(base_image_url))
                },
                None => (None, None, None, None),
            };

            let manifest = rmp_serde::encode::to_vec_named(c.manifest).context("manifest rmp")?;
            let language_stats = rmp_serde::encode::to_vec_named(&c.derived.language_stats).context("lang rmp")?;
            let named_args: &[(&str, &dyn ToSql)] = &[
                (":crate_id", &crate_id),
                (":readme", &readme),
                (":readme_format", &readme_format),
                (":readme_base_url", &readme_base_url),
                (":readme_base_image_url", &readme_base_image_url),
                (":crate_compressed_size", &c.derived.crate_compressed_size),
                (":crate_decompressed_size", &c.derived.crate_decompressed_size),
                (":capitalized_name", &c.derived.capitalized_name),
                (":lib_file", &c.derived.lib_file),
                (":has_buildrs", &c.derived.has_buildrs),
                (":is_nightly", &c.derived.is_nightly),
                (":is_yanked", &c.derived.is_yanked),
                (":has_code_of_conduct", &c.derived.has_code_of_conduct),
                (":manifest", &manifest),
                (":language_stats", &language_stats),
            ];
            insert_derived.execute_named(named_args).context("insert_derived")?;

            let is_important_ish = downloads > 2000;

            if let Some(repo) = c.repository {
                let url = repo.canonical_git_url();
                let args: &[&dyn ToSql] = &[&crate_id, &url.as_ref()];
                insert_repo.execute(args).context("insert repo")?;
            } else {
                delete_repo.execute(&[&crate_id]).context("del repo")?;
            }

            clear_categories.execute(&[&crate_id]).context("clear cat")?;

            let (categories, had_explicit_categories) = {
                let keywords = insert_keyword.keywords.iter().map(|(k,_)| k.to_string());
                self.extract_crate_categories(&tx, &c, keywords, is_important_ish)?
            };

            if !had_explicit_categories {
                let mut tmp = insert_keyword.keywords.iter().collect::<Vec<_>>();
                tmp.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap());
                write!(&mut out, "#{} ", tmp.into_iter().take(20).map(|(k, _)| k.to_string()).collect::<Vec<_>>().join(" #"))?;
            }

            if categories.is_empty() {
                write!(&mut out, "[no categories!] ")?;
            }
            if !had_explicit_categories && !categories.is_empty() {
                write!(&mut out, "[guessed categories]: ")?;
            }
            for (rank, rel, slug) in categories {
                write!(&mut out, ">{}, ", slug)?;
                let args: &[&dyn ToSql] = &[&crate_id, &slug, &rank, &rel];
                insert_category.execute(args).context("insert cat")?;
                if had_explicit_categories {
                    insert_keyword.add(&slug, rel/3., false);
                }
            }

            for (i, k) in c.authors.iter().filter_map(|a| a.email.as_ref().or(a.name.as_ref())).enumerate() {
                write!(&mut out, "by:{}, ", k)?;
                let w: f64 = 50. / (100 + i) as f64;
                insert_keyword.add(&k, w, false);
            }

            if let Some(repo) = c.repository {
                let url = repo.canonical_git_url();
                insert_keyword.add(&format!("repo:{}", url), 1., false); // crates in monorepo probably belong together
            }
            // yanked crates may contain garbage, or needlessly come up in similar crates
            // so knock all keywords' importance if it's yanked
            insert_keyword.commit(&tx, crate_id, if c.derived.is_yanked {0.1} else {1.})?;

            mark_updated.execute(&[&crate_id, &next_timestamp]).context("mark updated crate")?;
            println!("{}", out);
            Ok(())
        }).await
    }

    /// (rank-relevance, relevance, slug)
    ///
    /// Rank relevance is normalized and biased towards one top category
    fn extract_crate_categories(&self, conn: &Connection, c: &CrateVersionData<'_>, keywords: impl Iterator<Item=String>, is_important_ish: bool) -> FResult<(Vec<(f64, f64, String)>, bool)> {
        let (explicit_categories, invalid_categories): (Vec<_>, Vec<_>) = c.category_slugs.iter().map(|c| c.to_string())
            .partition(|slug| {
                categories::CATEGORIES.from_slug(&slug).1
            });
        let had_explicit_categories = !explicit_categories.is_empty();

        let keywords_collected = keywords.chain(invalid_categories).collect();

        let mut categories: Vec<_> = if had_explicit_categories {
            let cat_w = 10.0 / (9.0 + explicit_categories.len() as f64);
            let candidates = explicit_categories
                .into_iter()
                .enumerate()
                .map(|(i, slug)| {
                    let mut w = 100. / (5 + i.pow(2)) as f64 * cat_w;
                    if slug.contains("::") {
                        w *= 1.3; // more specific
                    }
                    (slug, w)
                })
                .collect();

            categories::adjusted_relevance(candidates, &keywords_collected, 0.01, 15)
        } else {
            let cat_w = 0.2 + 0.2 * c.manifest.package().keywords.len() as f64;
            self.guess_crate_categories_tx(conn, &c.origin, &keywords_collected, if is_important_ish {0.1} else {0.3})?.into_iter()
            .map(|(w, slug)| {
                ((w * cat_w).min(0.99), slug)
            }).collect()
        };

        // slightly nudge towards specific, leaf categories over root generic ones
        for (w, slug) in &mut categories {
            *w *= categories::CATEGORIES.from_slug(slug).0.last().map(|c| c.preference as f64).unwrap_or(1.);
        }

        let max_weight = categories.iter().map(|&(w, _)| w)
            .max_by(|a, b| a.partial_cmp(&b).unwrap_or(Ordering::Equal))
            .unwrap_or(0.3)
            .max(0.3); // prevents div/0, ensures odd choices stay low

        let categories = categories
            .into_iter()
            .map(|(relevance_weight, slug)| {
                let rank_weight = relevance_weight / max_weight * if relevance_weight >= max_weight * 0.99 { 1. } else { 0.4 }; // a crate is only in 1 category
                (rank_weight, relevance_weight, slug)
            })
            .collect();

        Ok((categories, had_explicit_categories))
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
    pub async fn index_repo_crates(&self, repo: &Repo, paths_and_names: impl Iterator<Item = (impl AsRef<str>, impl AsRef<str>)>) -> FResult<()> {
        let repo = repo.canonical_git_url();
        self.with_write("index_repo_crates", |tx| {
            let mut insert_repo = tx.prepare_cached("INSERT OR IGNORE INTO repo_crates (repo, path, crate_name) VALUES (?1, ?2, ?3)")?;
            for (path, name) in paths_and_names {
                let name = name.as_ref();
                let path = path.as_ref();
                insert_repo.execute(&[&repo.as_ref(), &path, &name]).context("repo rev insert")?;
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
                let s = r.get_raw(0).as_str().unwrap();
                Ok(Origin::from_crates_io_name(s))
            })?.filter_map(|r| r.ok());
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
                    return Ok(Some(Origin::from_crates_io_name(child)));
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
                if s.starts_with("rust") {
                    return &s[4..];
                }
                s
            }

            Ok(if let Some(child) = repo.repo_name().and_then(|n| paths.get(n).or_else(|| paths.get(unprefix(n)))) {
                Some(Origin::from_crates_io_name(child))
            } else if let Some(child) = repo.owner_name().and_then(|n| paths.get(n).or_else(|| paths.get(unprefix(n)))) {
                Some(Origin::from_crates_io_name(child))
            } else {
                None
            })
        }).await
    }

    pub async fn index_repo_changes(&self, repo: &Repo, changes: &[RepoChange]) -> FResult<()> {
        let repo = repo.canonical_git_url();
        self.with_write("index_repo_changes", |tx| {
            let mut insert_change = tx.prepare_cached("INSERT OR IGNORE INTO repo_changes (repo, crate_name, replacement, weight) VALUES (?1, ?2, ?3, ?4)")?;
            for change in changes {
                match *change {
                    RepoChange::Replaced { ref crate_name, ref replacement, weight } => {
                        let args: &[&dyn ToSql] = &[&repo, &crate_name.as_str(), &Some(replacement.as_str()), &weight];
                        insert_change.execute(args)
                    },
                    RepoChange::Removed { ref crate_name, weight } => {
                        let args: &[&dyn ToSql] = &[&repo, &crate_name.as_str(), &(None as Option<&str>), &weight];
                        insert_change.execute(args)
                    },
                }?;
            }
            Ok(())
        }).await
    }

    pub async fn path_in_repo(&self, repo: &Repo, crate_name: &str) -> FResult<Option<String>> {
        self.with_read("path_in_repo", |conn| self.path_in_repo_tx(conn, repo, crate_name)).await
    }

    pub fn path_in_repo_tx(&self, conn: &Connection, repo: &Repo, crate_name: &str) -> FResult<Option<String>> {
        let repo = repo.canonical_git_url();
        let mut get_path = conn.prepare_cached("SELECT path FROM repo_crates WHERE repo = ?1 AND crate_name = ?2")?;
        let args: &[&dyn ToSql] = &[&repo, &crate_name];
        Ok(none_rows(get_path.query_row(args, |row| row.get(0))).context("path_in_repo")?)
    }

    /// Update download counts of the crate
    pub async fn index_versions(&self, all: &RichCrate, score: f64, downloads_recent: Option<usize>) -> FResult<()> {
        self.with_write("index_versions", |tx| {
            let mut get_crate_id = tx.prepare_cached("SELECT id FROM crates WHERE origin = ?1")?;
            let mut insert_version = tx.prepare_cached("INSERT OR IGNORE INTO crate_versions (crate_id, version, created) VALUES (?1, ?2, ?3)")?;

            let origin = all.origin().to_str();
            let crate_id: u32 = get_crate_id.query_row(&[&origin], |row| row.get(0))
                .with_context(|_| format!("the crate {} hasn't been indexed yet", origin))?;

            let recent = downloads_recent.unwrap_or(0) as u32;
            let mut update_recent = tx.prepare_cached("UPDATE crates SET recent_downloads = ?1, ranking = ?2 WHERE id = ?3")?;
            let args: &[&dyn ToSql] = &[&recent, &score, &crate_id];
            update_recent.execute(args).context("update recent")?;

            for ver in all.versions() {
                let timestamp = DateTime::parse_from_rfc3339(&ver.created_at).context("version timestamp")?;
                let args: &[&dyn ToSql] = &[&crate_id, &ver.num, &timestamp.timestamp()];
                insert_version.execute(args).context("insert ver")?;
            }
            Ok(())
        }).await
    }

    pub async fn index_crate_owners(&self, origin: &Origin, owners: &[CrateOwner]) -> FResult<bool> {
        self.with_write("index_crate_owners", |tx| {
            let mut get_crate_id = tx.prepare_cached("SELECT id FROM crates WHERE origin = ?1")?;
            let mut insert = tx.prepare_cached("INSERT OR IGNORE INTO author_crates(github_id, crate_id, invited_by_github_id, invited_at) VALUES(?1, ?2, ?3, ?4)")?;
            let crate_id: u32 = match get_crate_id.query_row(&[&origin.to_str()], |row| row.get(0)) {
                Ok(id) => id,
                Err(_) => return Ok(false),
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
            Ok(true)
        }).await
    }

    pub async fn crates_by_author(&self, github_id: u32) -> FResult<Vec<CrateOwnerRow>> {
        self.with_read("crates_by_author", |conn| {
            let mut query = conn.prepare_cached(r#"SELECT ac.crate_id, ac.invited_by_github_id, ac.invited_at, max(cv.created)
                FROM author_crates ac JOIN crate_versions cv USING(crate_id)
                WHERE ac.github_id = ?1
                GROUP BY ac.crate_id
            "#)?;
            let q = query.query_map(&[&github_id], |row| {
                let crate_id: u32 = row.get_unwrap(0);
                let invited_by_github_id: Option<u32> = row.get_unwrap(1);
                let invited_at = row.get_raw(2).as_str().ok().and_then(|d| DateTime::parse_from_rfc3339(d).ok());
                let latest_timestamp: u32 = row.get_unwrap(3);
                Ok(CrateOwnerRow {
                    crate_id,
                    invited_by_github_id,
                    invited_at,
                    latest_version: DateTime::from_utc(NaiveDateTime::from_timestamp(latest_timestamp as _, 0), FixedOffset::east(0)),
                })
            })?;
            Ok(q.filter_map(|x| x.ok()).collect())
        }).await
    }

    /// Fetch or guess categories for a crate
    ///
    /// Returns category slugs
    fn crate_categories_tx(&self, conn: &Connection, origin: &Origin, kebab_keywords: &HashSet<String>, threshold: f64) -> FResult<Vec<(f64, String)>> {
        let assigned = self.assigned_crate_categories_tx(conn, origin)?;
        if !assigned.is_empty() {
            Ok(assigned)
        } else {
            self.guess_crate_categories_tx(&conn, origin, kebab_keywords, threshold)
        }
    }

    /// Assigned categories with their weights
    fn assigned_crate_categories_tx(&self, conn: &Connection, origin: &Origin) -> FResult<Vec<(f64, String)>> {
            let mut query = conn.prepare_cached(r#"
                SELECT c.relevance_weight, c.slug
                FROM crates k
                JOIN categories c on c.crate_id = k.id
                WHERE k.origin = ?1
                ORDER by relevance_weight desc
            "#)?;
            let res = query.query_map(&[&origin.to_str()], |row| Ok((row.get_unwrap(0), row.get_unwrap(1)))).context("crate_categories")?;
            Ok(res.collect::<std::result::Result<_,_>>()?)
    }

    fn guess_crate_categories_tx(&self, conn: &Connection, origin: &Origin, kebab_keywords: &HashSet<String>, threshold: f64) -> FResult<Vec<(f64, String)>> {
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
        limit 10"#).context("categories")?;
        let candidates = query.query_map(&[&origin.to_str()], |row| Ok((row.get_unwrap(0), row.get_unwrap(1)))).context("categories q")?;
        let candidates = candidates.collect::<std::result::Result<_, _>>()?;

        Ok(categories::adjusted_relevance(candidates, kebab_keywords, threshold, 2))
    }

    /// Find most relevant keyword for the crate
    ///
    /// Returns (top n-th for the keyword, the keyword)
    pub async fn top_keyword(&self, origin: &Origin) -> FResult<Option<(u32, String)>> {
        self.with_read("top_keyword", |conn| {
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
            Ok(none_rows(query.query_row(&[&origin.to_str()], |row| Ok((row.get_unwrap(0), row.get_unwrap(1)))))?)
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
            let res = query.query_map(&[&slug], |row| row.get(1)).context("related_categories")?;
            Ok(res.collect::<std::result::Result<_,_>>()?)
        }).await
    }

    pub async fn replacement_crates(&self, crate_name: &str) -> FResult<Vec<String>> {
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
            let res = query.query_map(&[&crate_name], |row| row.get(1)).context("replacement_crates")?;
            Ok(res.collect::<std::result::Result<_,_>>()?)
        }).await
    }

    pub async fn related_crates(&self, origin: &Origin, min_recent_downloads: u32) -> FResult<Vec<Origin>> {
        self.with_read("related_crates", |conn| {
            let mut query = conn.prepare_cached(r#"
                SELECT sum(k2.weight * k1.weight) as w, c2.origin
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
                LIMIT 6
            "#)?;
            let args: &[&dyn ToSql] = &[&origin.to_str(), &min_recent_downloads];
            let res = query.query_map(args, |row| {
                Ok(Origin::from_str(row.get_raw(1).as_str().unwrap()))
            }).context("related_crates")?;
            Ok(res.collect::<std::result::Result<_,_>>()?)
        }).await
    }

    /// Find keywords that may be most relevant to the crate
    #[inline]
    pub async fn keywords(&self, origin: &Origin) -> FResult<Vec<String>> {
        self.with_read("keywords", |conn| {
            self.keywords_tx(conn, origin)
        }).await
    }

    pub fn keywords_tx(&self, conn: &Connection, origin: &Origin) -> FResult<Vec<String>> {
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
            Ok(res.into_iter().filter_map(|(rel,k)|{
                if rel >= min_score {
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
            let q = query.query_map(&[&slug], |row| row.get(1)).context("top keywords")?;
            let q = q.filter_map(|r| r.ok());
            Ok(q.collect())
        }).await
    }

    /// Crate & total weighed removals (when dependency was removed from a crate)
    /// Roughly weighed by ranking of crates that did the removing.
    ///
    /// TODO: there should be a time decay, otherwise old crates will get penalized for churn
    pub async fn removals(&self) -> FResult<HashMap<Origin, f64>> {
        self.with_read("removals", |conn| {
            let mut query = conn.prepare("
                SELECT crate_name, sum(weight * (0.5+r.ranking/2)) AS w
                FROM (
                    SELECT max(k.ranking) as ranking, repo
                    FROM crate_repos cr
                    JOIN crates k ON cr.crate_id = k.id
                    WHERE k.ranking IS NOT NULL
                    GROUP BY cr.repo
                ) AS r
                JOIN repo_changes USING(repo)
                WHERE replacement IS NULL
                GROUP BY crate_name")?;
            let q = query.query_map(NO_PARAMS, |row| {
                let s = row.get_raw(0).as_str().unwrap();
                Ok((Origin::from_crates_io_name(s), row.get_unwrap(1)))
            })?;
            let q = q.filter_map(|r| r.ok());
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
                Ok((Origin::from_str(row.get_raw(0).as_str().unwrap()), row.get_unwrap(1)))
            })?;
            let q = q.filter_map(|r| r.ok());
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
                Ok((Origin::from_str(row.get_raw(0).as_str().unwrap()), row.get_unwrap(1)))
            })?;
            let q = q.filter_map(|r| r.ok());
            Ok(q.collect())
        }).await
    }

    /// Newly added or updated crates in the category
    ///
    /// Returns `origin` strings
    pub async fn recently_updated_crates_in_category(&self, slug: &str) -> FResult<Vec<Origin>> {
        self.with_read("recently_updated_crates_in_category", |conn| {
            let mut query = conn.prepare_cached(r#"
                select max(created) + 3600*24*7 * c.rank_weight * k.ranking, -- week*rank ~= best this week
                    k.origin
                    from categories c
                    join crate_versions v using (crate_id)
                    join crates k on v.crate_id = k.id
                    where c.slug = ?1
                    group by v.crate_id
                    having count(*) > 1 -- so these are updates, not new releases
                    order by 1 desc
                    limit 20
            "#)?;
            let q = query.query_map(&[&slug], |row| {
                Ok(Origin::from_str(row.get_raw(1).as_str().unwrap()))
            })?;
            let q = q.filter_map(|r| r.ok());
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
                let origin = Origin::from_str(row.get_raw(2).as_str()?);
                Ok((origin, row.get(1)?))
            })?;
            let q = q.filter_map(|r| r.ok());
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
                WHERE ranking > 0.2
                GROUP BY c.id
            "#)?;
            let q = q.query_map(NO_PARAMS, |row| -> Result<(Origin, f64, i64)> {
                Ok((Origin::from_str(row.get_raw(0).as_str().unwrap()), row.get_unwrap(1), row.get_unwrap(2)))
            }).context("sitemap")?.filter_map(|r| r.ok());
            Ok(q.collect())
        }).await
    }

    /// Number of crates in every category
    pub async fn category_crate_counts(&self) -> FResult<HashMap<String, u32>> {
        self.with_read("category_crate_counts", |conn| {
            let mut q = conn.prepare(r#"
                select c.slug, count(*) as cnt from categories c group by c.slug
            "#)?;
            let q = q.query_map(NO_PARAMS, |row| -> Result<(String, u32)> {
                Ok((row.get_unwrap(0), row.get_unwrap(1)))
            }).context("counts")?.filter_map(|r| r.ok());
            Ok(q.collect())
        }).await
    }

    /// Crates overdue for an update
    pub async fn crates_to_reindex(&self) -> FResult<Vec<Origin>> {
        self.with_read("crates_to_reindex", |conn| {
            let mut q = conn.prepare("SELECT origin FROM crates WHERE next_update < ?1 LIMIT 1000")?;
            let timestamp = Utc::now().timestamp() as u32;
            let q = q.query_map(&[&timestamp], |r| {
                let s = r.get_raw(0).as_str().unwrap();
                Ok(Origin::from_str(s))
            })?.filter_map(|r| r.ok());
            Ok(q.collect())
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
}

impl KeywordInsert {
    pub fn new() -> FResult<Self> {
        Ok(Self {
            keywords: HashMap::new(),
        })
    }

    pub fn add(&mut self, word: &str, weight: f64, visible: bool) {
        let word = word
            .trim_matches(|c: char| !c.is_alphanumeric())
            .trim_end_matches("-rs")
            .trim_start_matches("rust-");
        if word.is_empty() || weight <= 0.000001 {
            return;
        }
        let word = word.to_kebab_case();
        if word == "rust" || word == "rs" {
            return;
        }
        let k = self.keywords.entry(word).or_insert((weight, visible));
        if k.0 < weight {k.0 = weight}
        if visible {k.1 = visible}
    }

    pub fn add_synonyms(&mut self, tag_synonyms: &HashMap<Box<str>, (Box<str>, u8)>) {
        let to_add: Vec<_> = self.keywords.iter().filter_map(|(k, &(v, _))| {
            tag_synonyms.get(k.as_str()).and_then(|&(ref synonym, votes)| {
                let synonym: &str = &synonym;
                if self.keywords.get(synonym).is_some() {
                    None
                } else {
                    let relevance = (votes as f64 / 5. + 0.1).min(0.8);
                    Some((synonym.to_string(), v * relevance))
                }
            })
        }).collect();
        for (s, v) in to_add {
            self.keywords.entry(s).or_insert((v, false));
        }
    }

    pub fn commit(mut self, conn: &Connection, crate_id: u32, overall_weight: f64) -> FResult<()> {
        let mut select_id = conn.prepare_cached("SELECT id, visible FROM keywords WHERE keyword = ?1")?;
        let mut insert_name = conn.prepare_cached("INSERT OR IGNORE INTO keywords (keyword, visible) VALUES (?1, ?2)")?;
        let mut insert_value = conn.prepare_cached("INSERT OR IGNORE INTO crate_keywords(keyword_id, crate_id, weight, explicit)
            VALUES (?1, ?2, ?3, ?4)")?;
        let mut make_visible = conn.prepare_cached("UPDATE keywords SET visible = 1 WHERE id = ?1")?;
        let mut clear_keywords = conn.prepare_cached("DELETE FROM crate_keywords WHERE crate_id = ?1")?;

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

        clear_keywords.execute(&[&crate_id]).context("clear cat")?;
        for (word, (weight, visible)) in self.keywords {
            let args: &[&dyn ToSql] = &[&word, if visible { &1 } else { &0 }];
            insert_name.execute(args)?;
            let (keyword_id, old_vis): (u32, u32) = select_id.query_row(&[&word], |r| Ok((r.get_unwrap(0), r.get_unwrap(1)))).context("get keyword")?;
            if visible && old_vis == 0 {
                make_visible.execute(&[&keyword_id]).context("keyword vis")?;
            }
            let weight = weight * overall_weight;
            let args: &[&dyn ToSql] = &[&keyword_id, &crate_id, &weight, if visible { &1 } else { &0 }];
            insert_value.execute(args).context("keyword")?;
        }
        Ok(())
    }
}

pub struct CrateOwnerRow {
    crate_id: u32,
    invited_by_github_id: Option<u32>,
    invited_at: Option<DateTime<FixedOffset>>,
    latest_version: DateTime<FixedOffset>,
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
    let mut rt = tokio::runtime::Runtime::new().unwrap();
    let f = rt.spawn(async move {
    let t = tempfile::NamedTempFile::new().unwrap();

    let db = CrateDb::new_with_synonyms(t.as_ref(), Path::new("../data/tag-synonyms.csv")).unwrap();
    let origin = Origin::from_crates_io_name("cratedbtest");
    let derived = CrateVersionSourceData {
        capitalized_name: "captname".into(),
        ..Default::default()
    };
    let manifest = cargo_toml::Manifest::from_str(r#"[package]
name="crates-indexing-unit-test-hi"
version="1.2.3"
keywords = ["test-CRATE"]
categories = ["1", "two", "GAMES", "science", "::science::math::"]
"#).unwrap();
    db.index_latest(CrateVersionData {
        derived: &derived,
        manifest: &manifest,
        origin: &origin,
        deps_stats: &[],
        is_build: false,
        is_dev: false,
        authors: &[],
        category_slugs: &[],
        repository: None,
        extracted_auto_keywords: Vec::new(),
    }).await.unwrap();
    assert_eq!(1, db.crates_with_keyword("test-crate").await.unwrap());
    let (new_manifest, new_derived) = db.rich_crate_version_data(&origin).await.unwrap();
    assert_eq!(manifest.package().name, new_manifest.package().name);
    assert_eq!(manifest.package().keywords, new_manifest.package().keywords);
    assert_eq!(manifest.package().categories, new_manifest.package().categories);

    assert_eq!(new_derived.language_stats, derived.language_stats);
    assert_eq!(new_derived.crate_compressed_size, derived.crate_compressed_size);
    assert_eq!(new_derived.crate_decompressed_size, derived.crate_decompressed_size);
    assert_eq!(new_derived.is_nightly, derived.is_nightly);
    assert_eq!(new_derived.capitalized_name, derived.capitalized_name);
    assert_eq!(new_derived.readme, derived.readme);
    assert_eq!(new_derived.lib_file, derived.lib_file);
    assert_eq!(new_derived.has_buildrs, derived.has_buildrs);
    assert_eq!(new_derived.has_code_of_conduct, derived.has_code_of_conduct);
    assert_eq!(new_derived.is_yanked, derived.is_yanked);
    });
    rt.block_on(f).unwrap();
}

