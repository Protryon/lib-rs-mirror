extern crate rusqlite;
extern crate chrono;
extern crate categories;
#[macro_use] extern crate failure;
extern crate thread_local;
#[macro_use] extern crate lazy_static;
extern crate rich_crate;
use std::collections::HashMap;
use rich_crate::RichCrate;
use rich_crate::Origin;
use rich_crate::Repo;
use rich_crate::Markup;
use rich_crate::RichCrateVersion;
use rusqlite::*;
use thread_local::ThreadLocal;
use chrono::prelude::*;
use std::path::PathBuf;
use failure::ResultExt;
use std::cell::RefCell;
type Result<T> = std::result::Result<T, failure::Error>;

mod schema;
mod stopwords;
use stopwords::STOPWORDS;

pub struct CrateDb {
    path: PathBuf,
    pub(crate) conn: ThreadLocal<std::result::Result<RefCell<Connection>, rusqlite::Error>>,
}

impl CrateDb {
    /// Path to sqlite db file to create/update
    pub fn new(path: impl Into<PathBuf>) -> Result<Self> {
        Ok(Self {
            path: path.into(),
            conn: ThreadLocal::new(),
        })
    }

    #[inline]
    fn with_connection<F, T>(&self, cb: F) -> Result<T> where F: FnOnce(&mut Connection) -> Result<T> {
        let conn = self.conn.get_or(|| Box::new(self.connect().map(RefCell::new)));
        match conn {
            Ok(conn) => cb(&mut *conn.borrow_mut()),
            Err(err) => bail!("{}", err),
        }
    }

    #[inline]
    fn with_tx<F, T>(&self, cb: F) -> Result<T> where F: FnOnce(&Connection) -> Result<T> {
        self.with_connection(|conn| {
            let tx = conn.transaction()?;
            let res = cb(&tx)?;
            tx.commit()?;
            Ok(res)
        })
    }

    fn connect(&self) -> std::result::Result<Connection, rusqlite::Error> {
        let db = Self::db(&self.path)?;
        db.execute("PRAGMA synchronous = 0", &[])?;
        Ok(db)
    }

    /// Add data of the latest version of a crate to the index
    pub fn index_latest(&self, c: &RichCrateVersion) -> Result<()> {
        self.with_tx(|tx| {
            let mut insert_crate = tx.prepare_cached("INSERT OR IGNORE INTO crates (origin, recent_downloads) VALUES (?1, ?2)")?;
            let mut insert_repo = tx.prepare_cached("INSERT OR REPLACE INTO crate_repos (crate_id, repo) VALUES (?1, ?2)")?;
            let mut delete_repo = tx.prepare_cached("DELETE FROM crate_repos WHERE crate_id = ?1")?;
            let mut clear_categories = tx.prepare_cached("DELETE FROM categories WHERE crate_id = ?1")?;
            let mut clear_keywords = tx.prepare_cached("DELETE FROM crate_keywords WHERE crate_id = ?1")?;
            let mut insert_category = tx.prepare_cached("INSERT OR IGNORE INTO categories (crate_id, slug, weight) VALUES (?1, ?2, ?3)")?;
            let mut get_crate_id = tx.prepare_cached("SELECT id FROM crates WHERE origin = ?1")?;

            let origin = c.origin().to_str();

            insert_crate.execute(&[&origin, &0]).context("insert crate")?;
            let crate_id: u32 = get_crate_id.query_row(&[&origin], |row| row.get(0)).context("crate_id")?;
            let mut insert_keyword = KeywordInsert::new(&tx, crate_id)?;

            if let Some(repo) = c.repository() {
                let url = repo.canonical_git_url();
                insert_repo.execute(&[&crate_id, &url.as_ref()]).context("insert repo")?;
                insert_keyword.add(&format!(":repo:{}", url), 1., false)?; // crates in monorepo probably belong together
            } else {
                delete_repo.execute(&[&crate_id]).context("del repo")?;
            }

            clear_categories.execute(&[&crate_id]).context("clear cat")?;
            clear_keywords.execute(&[&crate_id]).context("clear cat")?;

            print!("{} = {}: ", origin, crate_id);

            let explicit_categories = c.raw_category_slugs().count();
            let cat_w = if explicit_categories > 0 {
                10.0 / (9.0 + explicit_categories as f64)
            } else {
                print!(">??? ");
                (0.001 + c.raw_keywords().count() as f64 * 0.01).min(0.05)
            };
            for (i, slug) in c.category_slugs().enumerate() {
                print!(">{}, ", slug);
                let mut w: f64 = 95./(5.+i as f64*3.) * cat_w;
                if slug.contains("::") {
                    w *= 1.3; // more specific
                }
                let is_real = categories::CATEGORIES.from_slug(&slug).next().is_some(); // FIXME: that checks top level only
                if is_real {
                    insert_category.execute(&[&crate_id, &&*slug, &w]).context("insert cat")?;
                }
                if explicit_categories > 0 {
                    insert_keyword.add(&*slug, w, false)?;
                }
            }
            for (i, k) in c.raw_keywords().map(|k| k.to_lowercase().trim().to_string()).enumerate() {
                print!("#{}, ", k);
                let mut w: f64 = 100./(6.+i as f64*2.);
                if STOPWORDS.get(&k).is_some() {
                    w *= 0.6;
                }
                insert_keyword.add(&k, w, true)?;
            }
            for (i, k) in c.short_name().split(|c: char| !c.is_alphanumeric()).enumerate() {
                print!("'{}, ", k);
                let mut w: f64 = 100./(8.+i as f64*2.);
                insert_keyword.add(k, w, false)?;
            }
            if let Some((w2,d)) = Self::extract_text(&c) {
                for (i, k) in d.split_whitespace().map(|k| k.to_lowercase().trim_matches(|c: char| !c.is_alphanumeric()).to_string()).enumerate() {
                    if k.len() < 2 || STOPWORDS.get(&k).is_some() {
                        continue;
                    }
                    let w: f64 = w2 * 150./(80.+i as f64);
                    insert_keyword.add(&k, w, false)?;
                }
            }
            for (i, k) in c.authors().iter().filter_map(|a|a.email.as_ref().or(a.name.as_ref()).map(|a| a.to_lowercase())).enumerate() {
                print!("@{}, ", k);
                let mut w: f64 = 50./(100.+i as f64);
                insert_keyword.add(&k, w, false)?;
            }
            if c.has_buildrs() {
                insert_keyword.add(":has_buildrs", 0.13, false)?;
            }
            if let Some(l) = c.links() {
                insert_keyword.add(l, 0.54, false)?;
            }
            for feat in c.features().keys() {
                if feat != "default" {
                    insert_keyword.add(&format!(":{}", feat), 0.55, false)?;
                }
            }
            println!("");
            Ok(())
        })
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
    pub fn index_repo_crates(&self, repo: &Repo, paths_and_names: impl Iterator<Item = (impl AsRef<str>, impl AsRef<str>)>) -> Result<()> {
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

    /// Returns crate name (not origin)
    pub fn parent_crate(&self, repo: &Repo, child_name: &str) -> Result<Option<String>> {
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

    pub fn index_repo_changes(&self, repo: &Repo, changes: &[RepoChange]) -> Result<()> {
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

    pub fn path_in_repo(&self, repo: &Repo, crate_name: &str) -> Result<String> {
        let repo = repo.canonical_git_url();
        self.with_connection(|conn| {
        let mut get_path = conn.prepare_cached("SELECT path FROM repo_crates WHERE repo = ?1 AND crate_name = ?2")?;
        Ok(get_path.query_row(&[&repo, &crate_name], |row| row.get(0)).context("path_in_repo")?)
        })
    }

    /// Update download counts of the crate
    pub fn index_versions(&self, all: &RichCrate) -> Result<()> {
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
                let period = dl.date.and_hms(0,0,0).timestamp();
                let ver = dl.version.map(|v| v.num.as_str()); // `NULL` means all versions together
                let downloads = dl.downloads as u32;
                insert_dl.execute(&[&crate_id, &period, &ver, &downloads]).context("insert dl")?;
            }
            Ok(())
        })
    }

    /// Guess categories for a crate
    ///
    /// Returns category slugs
    pub fn categories(&self, origin: &Origin) -> Result<Vec<String>> {
        self.with_connection(|conn| {
        let mut query = conn.prepare_cached(r#"
        select sum(cc.weight * ck.weight * relk.relevance)/(8+count(*)) as w, cc.slug
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
        having w > 0.3
        order by 1 desc
        limit 1"#).context("categories")?;
        let all = query.query_map(&[&origin.to_str()], |row| row.get(1)).context("categories q")?;
        Ok(all.collect::<std::result::Result<_,_>>()?)
        })
    }

    /// Find most relevant category for a crate, and how popular the crate is
    ///
    /// Returns (top n-th, category slug)
    pub fn top_category(&self, origin: &Origin) -> Result<(u32, String)> {
        self.with_connection(|conn| {
        let mut query = conn.prepare_cached(r#"
            select count(*) as top, cc.slug from crates c
            join categories cc on cc.crate_id = c.id
            join categories occ on occ.slug = cc.slug
            join crates oc on occ.crate_id = oc.id
            where c.origin = ?1
            and oc.recent_downloads >= c.recent_downloads
            group by cc.slug
            order by 1
            limit 1
        "#)?;
        Ok(query.query_row(&[&origin.to_str()], |row| (row.get(0), row.get(1)))?)
        })
    }

    /// Find most relevant keyword for the crate
    ///
    /// Returns (top n-th for the keyword, the keyword)
    pub fn top_keyword(&self, origin: &Origin) -> Result<(u32, String)> {
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
        Ok(query.query_row(&[&origin.to_str()], |row| (row.get(0), row.get(1)))?)
        })
    }

    /// Categories similar to the given category
    pub fn related_categories(&self, slug: &str) -> Result<Vec<String>> {
        self.with_connection(|conn| {
        let mut query = conn.prepare_cached(r#"
            select sum(c2.weight * c1.weight) as w, c2.slug
            from categories c1
            join categories c2 on c1.crate_id = c2.crate_id
            where c1.slug = ?1
            and c2.slug != c1.slug
            group by c2.slug
            having w > 500
            order by 1 desc
            limit 6
        "#)?;
        let res = query.query_map(&[&slug], |row| row.get(1)).context("related_categories")?;
        Ok(res.collect::<std::result::Result<_,_>>()?)
        })
    }

    pub fn replacement_crates(&self, crate_name: &str) -> Result<Vec<String>> {
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

    pub fn related_crates(&self, origin: &Origin) -> Result<Vec<Origin>> {
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
            HAVING w > 500
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
    pub fn keywords(&self, origin: &Origin) -> Result<Vec<String>> {
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
    pub fn top_keywords_in_category(&self, slug: &str) -> Result<Vec<String>> {
        self.with_connection(|conn| {
        let mut query = conn.prepare_cached(r#"
            select sum(k.weight), kk.keyword from categories c
                join crate_keywords k using(crate_id)
                join keywords kk on kk.id = k.keyword_id
                where explicit and c.slug = ?1
                group by k.keyword_id
                having sum(k.weight) > 50 and count(*) > 4
                order by 1 desc
                limit 10
        "#)?;
        let q = query.query_map(&[&slug], |row| row.get(1)).context("top keywords")?;
        let q = q.filter_map(|r| r.ok());
        Ok(q.collect())
        })
    }

    /// Most popular crates in the category
    pub fn top_crates_in_category(&self, slug: &str, limit: u32, simple_sort: bool) -> Result<Vec<(Origin, u32)>> {
        self.with_connection(|conn| {

        let mut query = conn.prepare_cached(if simple_sort {
            // sort by popularity
            "select k.origin, k.recent_downloads from categories c
                join crates k on c.crate_id = k.id
                where c.slug = ?1
                order by recent_downloads desc
                limit ?2
        "} else {
            // sort by relevance to the category, downrank for being removed from crates
            "select k.origin, k.recent_downloads from categories c
                join crates k on c.crate_id = k.id
                left join (select sum(weight) as w, crate_name from repo_changes where replacement is null group by crate_name) as removals
                on k.origin = 'crates.io:' || removals.crate_name
                where c.slug = ?1
                order by (recent_downloads * c.weight) / (300 + coalesce(removals.w, 1)) desc
                limit ?2"
        })?;

        let q = query.query_map(&[&slug, &limit], |row| {
            let s: String = row.get(0);
            (Origin::from_string(s), row.get(1))
        })?;
        let q = q.filter_map(|r| r.ok());
        Ok(q.collect())
        })
    }

    /// Newly added or updated crates in the category
    ///
    /// Returns `origin` strings
    pub fn recently_updated_crates_in_category(&self, slug: &str) -> Result<Vec<Origin>> {
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
    pub fn category_crate_counts(&self) -> Result<HashMap<String, u32>> {
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

    fn extract_text(c: &RichCrateVersion) -> Option<(f64,&str)> {
        if let Some(s) = c.description() {
            return Some((1.,s));
        }
        if let Ok(Some(r)) = c.readme() {
            let sub = match r.markup {
                Markup::Markdown(ref s) | Markup::Rst(ref s) => s,
            };
            let end = sub.char_indices().skip(200).map(|(i,_)|i).next().unwrap_or(sub.len());
            let sub = &sub[0..end].trim_right_matches(|c:char| c.is_alphanumeric());//half-word
            return Some((0.5,sub));
        }
        None
    }
}

pub enum RepoChange {
    Removed {crate_name: String, weight: f64},
    Replaced {crate_name: String, replacement: String, weight: f64},
}

pub struct KeywordInsert<'a> {
    crate_id: u32,
    select_id: CachedStatement<'a>,
    insert_name: CachedStatement<'a>,
    insert_value: CachedStatement<'a>,
    make_visible: CachedStatement<'a>,
}

impl<'a> KeywordInsert<'a> {
    pub fn new(conn: &'a Connection, crate_id: u32) -> Result<Self> {
        Ok(Self {
            crate_id,
            select_id: conn.prepare_cached("SELECT id, visible FROM keywords WHERE keyword = ?1")?,
            insert_name: conn.prepare_cached("INSERT OR IGNORE INTO keywords (keyword, visible) VALUES (?1, ?2)")?,
            insert_value: conn.prepare_cached("INSERT OR IGNORE INTO crate_keywords(keyword_id, crate_id, weight, explicit)
                VALUES (?1, ?2, ?3, ?4)")?,
            make_visible: conn.prepare_cached("UPDATE keywords SET visible = 1 WHERE id = ?1")?,
        })
    }

    pub fn add(&mut self, word: &str, weight: f64, visible: bool) -> Result<()> {
        if word.is_empty() {
            return Ok(());
        }
        self.insert_name.execute(&[&word, if visible {&1} else {&0}])?;
        let (keyword_id, old_vis): (u32, u32) = self.select_id.query_row(&[&word],|r| (r.get(0), r.get(1))).context("get keyword")?;
        if visible && old_vis == 0 {
            self.make_visible.execute(&[&keyword_id]).context("keyword vis")?;
        }
        self.insert_value.execute(&[&keyword_id, &self.crate_id, &weight, if visible {&1} else {&0}]).context("keyword")?;
        Ok(())
    }
}

