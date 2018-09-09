extern crate categories;
extern crate chrono;
extern crate rusqlite;
#[macro_use]
extern crate failure;
extern crate thread_local;
#[macro_use]
extern crate lazy_static;
extern crate rich_crate;
use std::borrow::Cow;
use chrono::prelude::*;
use failure::ResultExt;
use rich_crate::Include;
use rich_crate::Markup;
use rich_crate::Origin;
use rich_crate::Repo;
use rich_crate::RichCrate;
use rich_crate::RichCrateVersion;
use rusqlite::*;
use std::sync::Mutex;
use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;
use thread_local::ThreadLocal;
type FResult<T> = std::result::Result<T, failure::Error>;

mod schema;
mod stopwords;
use stopwords::{COND_STOPWORDS, STOPWORDS};

pub struct CrateDb {
    url: String,
    conn: ThreadLocal<std::result::Result<RefCell<Connection>, rusqlite::Error>>,
    exclusive_conn: Mutex<Option<Connection>>,
}

impl CrateDb {
    /// Path to sqlite db file to create/update
    pub fn new(path: impl AsRef<Path>) -> FResult<Self> {
        let path = path.as_ref();
        Ok(Self {
            url: format!("file:{}?cache=shared", path.display()),
            conn: ThreadLocal::new(),
            exclusive_conn: Mutex::new(None),
        })
    }

    #[inline]
    fn with_connection<F, T>(&self, cb: F) -> FResult<T> where F: FnOnce(&mut Connection) -> FResult<T> {
        let conn = self.conn.get_or(|| Box::new(self.connect().map(RefCell::new)));
        match conn {
            Ok(conn) => cb(&mut *conn.borrow_mut()),
            Err(err) => bail!("{}", err),
        }
    }

    #[inline]
    fn with_tx<F, T>(&self, cb: F) -> FResult<T> where F: FnOnce(&Connection) -> FResult<T> {
        let mut conn = self.exclusive_conn.lock().unwrap();
        let conn = conn.get_or_insert_with(|| self.connect().unwrap());

        let tx = conn.transaction()?;
        let res = cb(&tx)?;
        tx.commit()?;
        Ok(res)
    }

    fn connect(&self) -> std::result::Result<Connection, rusqlite::Error> {
        let db = Self::db(&self.url)?;
        db.execute_batch("
            PRAGMA cache_size = 500000;
            PRAGMA threads = 4;
            PRAGMA synchronous = 0;
            PRAGMA journal_mode = TRUNCATE;")?;
        Ok(db)
    }

    pub fn latest_crate_update_timestamp(&self) -> FResult<Option<u32>> {
        self.with_connection(|conn| {
            Ok(none_rows(conn.query_row("SELECT max(created) FROM crate_versions", &[], |row| row.get(0)))?)
        })
    }

    /// Add data of the latest version of a crate to the index
    pub fn index_latest(&self, c: &RichCrateVersion) -> FResult<()> {
        let origin = c.origin().to_str();

        let mut insert_keyword = KeywordInsert::new()?;
        for (i, k) in c.keywords(Include::AuthoritativeOnly).map(|k| k.trim().to_lowercase()).enumerate() {
            print!("#{}, ", k);
            let mut w: f64 = 100./(6+i*2) as f64;
            if STOPWORDS.get(k.as_str()).is_some() {
                w *= 0.6;
            }
            insert_keyword.add(&k, w, true);
        }
        for (i, k) in c.short_name().split(|c: char| !c.is_alphanumeric()).enumerate() {
            print!("'{}, ", k);
            let mut w: f64 = 100./(8+i*2) as f64;
            insert_keyword.add(k, w, false);
        }
        if let Some((w2, d)) = Self::extract_text(&c) {
            let d: &str = &d;
            for (i, k) in d.split_whitespace()
                .map(|k| k.trim_right_matches("'s"))
                .filter(|k| k.len() >= 2)
                .map(|k| k.to_lowercase())
                .filter(|k| STOPWORDS.get(k.as_str()).is_none())
                .take(25)
                .enumerate() {
                let w: f64 = w2 * 150./(80+i) as f64;
                insert_keyword.add(&k, w, false);
            }
        }
        if let Some(l) = c.links() {
            insert_keyword.add(l.trim_left_matches("lib"), 0.54, false);
        }
        for feat in c.features().keys() {
            if feat != "default" {
                insert_keyword.add(&format!("feature:{}", feat), 0.55, false);
            }
        }
        if c.is_sys() {
            insert_keyword.add("has:is_sys", 0.01, false);
        }
        if c.has_bin() {
            insert_keyword.add("has:bin", 0.01, false);
        }

        print!("{}: ", origin);

        {
            let mut tmp = insert_keyword.keywords.iter().collect::<Vec<_>>();
            tmp.sort_by(|a,b| b.1.partial_cmp(a.1).unwrap());
            print!("#{} ", tmp.into_iter().map(|(k,_)| k.to_string()).collect::<Vec<_>>().join(" #"));
        }

        self.with_tx(|tx| {
            let mut insert_crate = tx.prepare_cached("INSERT OR IGNORE INTO crates (origin, recent_downloads) VALUES (?1, ?2)")?;
            let mut insert_repo = tx.prepare_cached("INSERT OR REPLACE INTO crate_repos (crate_id, repo) VALUES (?1, ?2)")?;
            let mut delete_repo = tx.prepare_cached("DELETE FROM crate_repos WHERE crate_id = ?1")?;
            let mut clear_categories = tx.prepare_cached("DELETE FROM categories WHERE crate_id = ?1")?;
            let mut insert_category = tx.prepare_cached("INSERT OR IGNORE INTO categories (crate_id, slug, rank_weight, relevance_weight) VALUES (?1, ?2, ?3, ?4)")?;
            let mut get_crate_id = tx.prepare_cached("SELECT id, recent_downloads FROM crates WHERE origin = ?1")?;

            insert_crate.execute(&[&origin, &0]).context("insert crate")?;
            let (crate_id, downloads): (u32, u32) = get_crate_id.query_row(&[&origin], |row| (row.get(0), row.get(1))).context("crate_id")?;
            let is_important_ish = downloads > 2000;

            if let Some(repo) = c.repository() {
                let url = repo.canonical_git_url();
                insert_repo.execute(&[&crate_id, &url.as_ref()]).context("insert repo")?;
            } else {
                delete_repo.execute(&[&crate_id]).context("del repo")?;
            }

            clear_categories.execute(&[&crate_id]).context("clear cat")?;

            let (categories, had_explicit_categories) = {
                let keywords = insert_keyword.keywords.iter().map(|(k,_)| k.to_string());
                self.extract_crate_categories(&tx, c, keywords, is_important_ish)?
            };

            if !had_explicit_categories {
                print!(">??? ");
            }
            for (rank, rel, slug) in categories {
                print!(">{}, ", slug);
                insert_category.execute(&[&crate_id, &slug, &rank, &rel]).context("insert cat")?;
                if had_explicit_categories {
                    insert_keyword.add(&slug, rel/3., false);
                }
            }
            for (i, k) in c.authors().iter().filter_map(|a|a.email.as_ref().or(a.name.as_ref())).enumerate() {
                print!("by:{}, ", k);
                let mut w: f64 = 50./(100+i) as f64;
                insert_keyword.add(&k, w, false);
            }

            if let Some(repo) = c.repository() {
                let url = repo.canonical_git_url();
                insert_keyword.add(&format!("repo:{}", url), 1., false); // crates in monorepo probably belong together
            }
            insert_keyword.commit(&tx, crate_id)?;
            println!();
            Ok(())
        })
    }

    /// (rank-relevance, relevance, slug)
    ///
    /// Rank relevance is normalized and biased towards one top category
    fn extract_crate_categories(&self, conn: &Connection, c: &RichCrateVersion, keywords: impl Iterator<Item=String>, is_important_ish: bool) -> FResult<(Vec<(f64, f64, String)>, bool)> {
        let (explicit_categories, invalid_categories): (Vec<_>, Vec<_>) = c.category_slugs(Include::AuthoritativeOnly)
            .map(|k| k.to_string())
            .partition(|slug| {
                categories::CATEGORIES.from_slug(&slug).next().is_some() // FIXME: that checks top level only
            });
        let had_explicit_categories = !explicit_categories.is_empty();

        let keywords_collected = keywords.chain(invalid_categories).collect();

        let categories: Vec<_> = if had_explicit_categories {
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

            categories::adjusted_relevance(candidates, keywords_collected, 0.01, 15)
        } else {
            let cat_w = 0.2 + 0.2 * c.keywords(Include::AuthoritativeOnly).count() as f64;
            self.crate_categories_tx(conn, &c.origin(), keywords_collected, if is_important_ish {0.1} else {0.3})?.into_iter()
            .map(|(w, slug)| {
                ((w * cat_w).min(0.99), slug)
            }).collect()
        };

        let max_weight = categories.iter().map(|(w, _)| *w)
            .max_by(|a, b| a.partial_cmp(&b).unwrap())
            .unwrap_or(0.)
            .max(0.3); // prevents div/0, ensures odd choices stay low

        let is_sys = c.is_sys();
        let categories = categories
            .into_iter()
            .map(|(relevance_weight, slug)| {
                let rank_weight = relevance_weight/max_weight
                * if relevance_weight >= max_weight*0.99 {1.} else {0.4} // a crate is only in 1 category
                * if is_sys {0.92} else {1.}; // rank sys crates below their high-level wrappers // TODO do same for derive helpers
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
    pub fn index_repo_crates(&self, repo: &Repo, paths_and_names: impl Iterator<Item = (impl AsRef<str>, impl AsRef<str>)>) -> FResult<()> {
        let repo = repo.canonical_git_url();
        self.with_tx(|tx| {
            let mut insert_repo = tx.prepare_cached("INSERT OR IGNORE INTO repo_crates (repo, path, crate_name) VALUES (?1, ?2, ?3)")?;
            for (path, name) in paths_and_names {
                let name = name.as_ref();
                let path = path.as_ref();
                insert_repo.execute(&[&repo.as_ref(), &path, &name]).context("repo rev insert")?;
            }
            Ok(())
        })
    }

    pub fn crates_in_repo(&self, repo: &Repo) -> FResult<Vec<String>> {
        self.with_connection(|conn| {
            let mut q = conn.prepare_cached("
                SELECT crate_name
                FROM repo_crates
                WHERE repo = ?1
                ORDER BY path, crate_name LIMIT 10
            ")?;
            let q = q.query_map(&[&repo.canonical_git_url()], |r| {
                let n: String = r.get(0); n
            })?.filter_map(|r| r.ok());
            Ok(q.collect())
        })
    }

    /// Returns crate name (not origin)
    pub fn parent_crate(&self, repo: &Repo, child_name: &str) -> FResult<Option<String>> {
        self.with_connection(|conn| {
            let mut paths = conn.prepare_cached("SELECT path, crate_name FROM repo_crates WHERE repo = ?1 LIMIT 100")?;
            let mut paths: HashMap<String, String> = paths
                .query_map(&[&repo.canonical_git_url()], |r| (r.get(0), r.get(1)))?
                .collect::<std::result::Result<_, _>>()?;

            if paths.len() < 2 {
                return Ok(None);
            }

            let child_path = if let Some(a) = paths.iter().find(|(_, child)| *child == child_name)
                .map(|(path, _)| path.to_owned()) {a} else {return Ok(None)};

            paths.remove(&child_path);
            let mut child_path = child_path.as_str();

            loop {
                 child_path = child_path.rsplitn(1, '/').nth(1).unwrap_or("");
                if let Some(child) = paths.get(child_path) {
                    return Ok(Some(child.to_owned()));
                }
                if child_path.is_empty() { // in these paths "" is the root
                    break;
                }
            }

            fn unprefix(s: &str) -> &str {
                if s.starts_with("rust-") || s.starts_with("rust_") {
                    return &s[5..];
                }
                if s.ends_with("-rs") || s.ends_with("_rs") {
                    return &s[..s.len()-3];
                }
                if s.starts_with("rust") {
                    return &s[4..];
                }
                s
            }

            Ok(if let Some(child) = repo.repo_name().and_then(|n| paths.get(n).or_else(|| paths.get(unprefix(n)))) {
                Some(child.to_owned())
            }
            else if let Some(child) = repo.owner_name().and_then(|n| paths.get(n).or_else(|| paths.get(unprefix(n)))) {
                Some(child.to_owned())
            } else {
                None
            })
        })
    }

    pub fn index_repo_changes(&self, repo: &Repo, changes: &[RepoChange]) -> FResult<()> {
        let repo = repo.canonical_git_url();
        self.with_tx(|tx| {
            let mut insert_change = tx.prepare_cached("INSERT OR IGNORE INTO repo_changes (repo, crate_name, replacement, weight) VALUES (?1, ?2, ?3, ?4)")?;
            for change in changes {
                match *change {
                    RepoChange::Replaced {ref crate_name, ref replacement, weight} => {
                        insert_change.execute(&[&repo, &crate_name.as_str(), &Some(replacement.as_str()), &weight])
                    },
                    RepoChange::Removed {ref crate_name, weight} => {
                        insert_change.execute(&[&repo, &crate_name.as_str(), &(None as Option<&str>), &weight])
                    },
                }?;
            }
            Ok(())
        })
    }

    pub fn path_in_repo(&self, repo: &Repo, crate_name: &str) -> FResult<Option<String>> {
        let repo = repo.canonical_git_url();
        self.with_connection(|conn| {
            let mut get_path = conn.prepare_cached("SELECT path FROM repo_crates WHERE repo = ?1 AND crate_name = ?2")?;
            Ok(none_rows(get_path.query_row(&[&repo, &crate_name], |row| row.get(0))).context("path_in_repo")?)
        })
    }

    /// Update download counts of the crate
    pub fn index_versions(&self, all: &RichCrate) -> FResult<()> {
        self.with_tx(|tx| {
            let mut update_recent = tx.prepare_cached("UPDATE crates SET recent_downloads = ?1 WHERE id = ?2")?;
            let mut get_crate_id = tx.prepare_cached("SELECT id FROM crates WHERE origin = ?1")?;
            let mut insert_version = tx.prepare_cached("INSERT OR IGNORE INTO crate_versions (crate_id, version, created) VALUES (?1, ?2, ?3)")?;
            let mut insert_dl = tx.prepare_cached("INSERT OR REPLACE INTO crate_downloads (crate_id, period, version, downloads) VALUES (?1, ?2, ?3, ?4)").context("cr dl")?;

            let origin = all.origin().to_str();
            let crate_id: u32 = get_crate_id.query_row(&[&origin], |row| row.get(0))
                .with_context(|_| format!("the crate {} hasn't been indexed yet", origin))?;
            let recent = all.downloads_recent() as u32;
            update_recent.execute(&[&recent, &crate_id]).context("update recent")?;

            for ver in all.versions() {
                let timestamp = DateTime::parse_from_rfc3339(&ver.created_at).context("version timestamp")?;
                insert_version.execute(&[&crate_id, &ver.num, &timestamp.timestamp()]).context("insert ver")?;
            }

            for dl in all.daily_downloads() {
                let downloads = dl.downloads as u32;
                if downloads > 0 {
                    let period = dl.date.and_hms(0,0,0).timestamp();
                    let ver = dl.version.map(|v| v.num.as_str()); // `NULL` means all versions together
                    insert_dl.execute(&[&crate_id, &period, &ver, &downloads]).context("insert dl")?; // FIXME: ignore 0s?
                }
            }
            Ok(())
        })
    }

    /// Guess categories for a crate
    ///
    /// Returns category slugs
    pub fn guess_crate_categories<'a>(&self, origin: &Origin, keywords: impl Iterator<Item = &'a str>) -> FResult<Vec<(f64, String)>> {
        self.with_connection(|conn| {
            self.crate_categories_tx(&conn, origin, keywords.map(|k| k.to_lowercase()).collect(), 0.1)
        })
    }

    /// Assigned categories with their weights
    pub fn crate_categories(&self, origin: &Origin) -> FResult<Vec<(f64, String)>> {
        self.with_connection(|conn| {
            let mut query = conn.prepare_cached(r#"
                SELECT c.relevance_weight, c.slug
                FROM crates k
                JOIN categories c on c.crate_id = k.id
                WHERE k.origin = ?1
                ORDER by relevance_weight desc
            "#)?;
            let res = query.query_map(&[&origin.to_str()], |row| (row.get(0), row.get(1))).context("crate_categories")?;
            Ok(res.collect::<std::result::Result<_,_>>()?)
        })
    }

    fn crate_categories_tx(&self, conn: &Connection, origin: &Origin, keywords: HashSet<String>, threshold: f64) -> FResult<Vec<(f64, String)>> {
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
        let candidates = query.query_map(&[&origin.to_str()], |row| (row.get(0), row.get(1))).context("categories q")?;
        let candidates = candidates.collect::<std::result::Result<_, _>>()?;

        Ok(categories::adjusted_relevance(candidates, keywords, threshold, 2))
    }

    /// Find most relevant keyword for the crate
    ///
    /// Returns (top n-th for the keyword, the keyword)
    pub fn top_keyword(&self, origin: &Origin) -> FResult<Option<(u32, String)>> {
        self.with_connection(|conn| {
            let mut query = conn.prepare_cached(r#"
            select top, keyword from (
                select count(*) as total, sum(case when oc.recent_downloads >= c.recent_downloads then 1 else 0 end) as top, k.keyword from crates c
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
            Ok(none_rows(query.query_row(&[&origin.to_str()], |row| (row.get(0), row.get(1))))?)
        })
    }

    /// Number of crates with a given keyword
    ///
    /// NB: must be lowercase
    pub fn crates_with_keyword(&self, keyword: &str) -> FResult<u32> {
        self.with_connection(|conn| {
            let mut query = conn.prepare_cached("SELECT count(*) FROM crate_keywords
                WHERE explicit AND keyword_id = (SELECT id FROM keywords WHERE keyword = ?1)")?;
            Ok(none_rows(query.query_row(&[&keyword], |row| row.get(0)))?.unwrap_or(0))
        })
    }

    /// Categories similar to the given category
    pub fn related_categories(&self, slug: &str) -> FResult<Vec<String>> {
        self.with_connection(|conn| {
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
        })
    }

    pub fn replacement_crates(&self, crate_name: &str) -> FResult<Vec<String>> {
        self.with_connection(|conn| {
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
        })
    }

    pub fn related_crates(&self, origin: &Origin) -> FResult<Vec<Origin>> {
        self.with_connection(|conn| {
            let mut query = conn.prepare_cached(r#"
                SELECT sum(k2.weight * k1.weight) as w, c2.origin
                FROM crates c1
                JOIN crate_keywords k1 on k1.crate_id = c1.id
                JOIN crate_keywords k2 on k1.keyword_id = k2.keyword_id
                JOIN crates c2 on k2.crate_id = c2.id
                WHERE c1.origin = ?1
                AND k2.crate_id != c1.id
                AND c2.recent_downloads > 150
                GROUP by k2.crate_id
                HAVING w > 200
                ORDER by 1 desc
                LIMIT 6
            "#)?;
            let res = query.query_map(&[&origin.to_str()], |row| {
                let s: String = row.get(1);
                Origin::from_string(s)
            }).context("related_crates")?;
            Ok(res.collect::<std::result::Result<_,_>>()?)
        })
    }

    /// Find keywords that may be most relevant to the crate
    pub fn keywords(&self, origin: &Origin) -> FResult<Vec<String>> {
        self.with_connection(|conn| {
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
            let res: Vec<(f64, String)> = query.query_map(&[&origin.to_str()], |row| (row.get(0), row.get(1)))?
                .collect::<std::result::Result<_,_>>()?;
            let min_score = res.get(0).map_or(0., |(rel,_)|rel/20.);
            Ok(res.into_iter().filter_map(|(rel,k)|{
                if rel >= min_score {
                    Some(k)
                } else {
                    None
                }
            }).collect())
        })
    }

    /// Find most relevant/popular keywords in the category
    pub fn top_keywords_in_category(&self, slug: &str) -> FResult<Vec<String>> {
        self.with_connection(|conn| {
            let mut query = conn.prepare_cached(r#"
                select sum(k.weight * c.relevance_weight), kk.keyword from categories c
                    join crate_keywords k using(crate_id)
                    join keywords kk on kk.id = k.keyword_id
                    where explicit and c.slug = ?1
                    group by k.keyword_id
                    having sum(k.weight) > 11 and count(*) >= 4
                    order by 1 desc
                    limit 10
            "#)?;
            let q = query.query_map(&[&slug], |row| row.get(1)).context("top keywords")?;
            let q = q.filter_map(|r| r.ok());
            Ok(q.collect())
        })
    }

    /// Crate & total weighed removals (when dependency was removed from a crate)
    pub fn removals(&self) -> FResult<HashMap<Origin, f64>> {
        self.with_connection(|conn| {
            let mut query = conn.prepare("SELECT crate_name, sum(weight) as w
                FROM repo_changes
                WHERE replacement IS NULL
                GROUP BY crate_name")?;
            let q = query.query_map(&[], |row| {
                let s: String = row.get(0);
                (Origin::from_crates_io_name(&s), row.get(1))
            })?;
            let q = q.filter_map(|r| r.ok());
            Ok(q.collect())
        })
    }

    /// Most popular crates in the category
    /// Returns recent_downloads and weight/importance as well
    pub fn top_crates_in_category_partially_ranked(&self, slug: &str, limit: u32) -> FResult<Vec<(Origin, u32, f64)>> {
        self.with_connection(|conn| {
            // sort by relevance to the category, downrank for being removed from crates
            let mut query = conn.prepare_cached(
            "SELECT k.origin, k.recent_downloads, (k.recent_downloads * c.rank_weight) as w
                FROM categories c
                JOIN crates k on c.crate_id = k.id
                WHERE c.slug = ?1
                ORDER by w desc
                LIMIT ?2"
            )?;

            let q = query.query_map(&[&slug, &limit], |row| {
                let s: String = row.get(0);
                (Origin::from_string(s), row.get(1), row.get(2))
            })?;
            let q = q.filter_map(|r| r.ok());
            Ok(q.collect())
        })
    }

    /// Newly added or updated crates in the category
    ///
    /// Returns `origin` strings
    pub fn recently_updated_crates_in_category(&self, slug: &str) -> FResult<Vec<Origin>> {
        self.with_connection(|conn| {
            let mut query = conn.prepare_cached(r#"
                select max(created), k.origin
                    from categories c
                    join crate_versions v using (crate_id)
                    join crates k on v.crate_id = k.id
                    where c.slug = ?1
                    group by v.crate_id
                    order by 1 desc
                    limit 20
            "#)?;
            let q = query.query_map(&[&slug], |row| {
                let s: String = row.get(1);
                Origin::from_string(s)
            })?;
            let q = q.filter_map(|r| r.ok());
            Ok(q.collect())
        })
    }

    /// Number of crates in every category
    pub fn category_crate_counts(&self) -> FResult<HashMap<String, u32>> {
        self.with_connection(|conn| {
            let mut q = conn.prepare(r#"
                select c.slug, count(*) as cnt from categories c group by c.slug
            "#)?;
            let q = q.query_map(&[], |row| -> (String, u32) {
                (row.get(0), row.get(1))
            }).context("counts")?.filter_map(|r| r.ok());
            Ok(q.collect())
        })
    }

    fn extract_text(c: &RichCrateVersion) -> Option<(f64, Cow<str>)> {
        if let Some(s) = c.description() {
            if let Some(more) = c.alternative_description() {
                return Some((1., format!("{}{}", s, more).into()));
            }
            return Some((1., s.into()));
        }
        if let Ok(Some(r)) = c.readme() {
            let sub = match r.markup {
                Markup::Markdown(ref s) | Markup::Rst(ref s) => s,
            };
            let end = sub.char_indices().skip(200).map(|(i,_)|i).next().unwrap_or(sub.len());
            let sub = sub[0..end].trim_right_matches(|c:char| c.is_alphanumeric());//half-word
            return Some((0.5, sub.into()));
        }
        None
    }
}

pub enum RepoChange {
    Removed {crate_name: String, weight: f64},
    Replaced {crate_name: String, replacement: String, weight: f64},
}

pub struct KeywordInsert {
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
            .trim_right_matches("-rs")
            .trim_left_matches("rust-");
        if word.is_empty() || weight <= 0.000001 {
            return;
        }
        let word = word.to_lowercase();
        if word == "rust" || word == "rs" {
            return;
        }
        let k = self.keywords.entry(word).or_insert((weight, visible));
        if k.0 < weight {k.0 = weight}
        if visible {k.1 = visible}
    }

    pub fn commit(mut self, conn: &Connection, crate_id: u32) -> FResult<()> {
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
            insert_name.execute(&[&word, if visible {&1} else {&0}])?;
            let (keyword_id, old_vis): (u32, u32) = select_id.query_row(&[&word],|r| (r.get(0), r.get(1))).context("get keyword")?;
            if visible && old_vis == 0 {
                make_visible.execute(&[&keyword_id]).context("keyword vis")?;
            }
            insert_value.execute(&[&keyword_id, &crate_id, &weight, if visible {&1} else {&0}]).context("keyword")?;
        }
        Ok(())
    }
}

#[inline]
fn none_rows<T>(res: std::result::Result<T, rusqlite::Error>) -> std::result::Result<Option<T>, rusqlite::Error> {
    match res {
        Ok(dat) => Ok(Some(dat)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(err) => Err(err),
    }
}
