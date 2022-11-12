use github_info::User;
use github_info::UserType;
use parking_lot::Mutex;
use rusqlite::types::ToSql;
use rusqlite::{Connection, Error, Row, Transaction};
use std::path::Path;
use std::time::SystemTime;
use log::info;

type Result<T, E = rusqlite::Error> = std::result::Result<T, E>;

mod schema;

pub struct UserDb {
    pub(crate) conn: Mutex<Connection>,
}

impl UserDb {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let db = Self::db(path.as_ref())?;
        db.execute_batch("
            PRAGMA journal_mode = WAL;")?;
        Ok(Self {
            conn: Mutex::new(db),
        })
    }

    pub fn email_has_github(&self, email: &str) -> Result<bool> {
        let conn = self.conn.lock();
        let mut get_email = conn.prepare_cached("SELECT 1 FROM github_emails WHERE email = ?1 LIMIT 1")?;
        get_email.exists([&email.to_ascii_lowercase()])
    }

    pub fn user_by_github_login(&self, login: &str) -> Result<Option<User>> {
        self.user_by_github_login_opt(login, false)
    }

    pub fn user_by_github_login_opt(&self, login: &str, allow_stale: bool) -> Result<Option<User>> {
        let res = self.user_by_query(r"SELECT
                u.id,
                u.login,
                u.name,
                u.avatar_url,
                u.gravatar_id,
                u.html_url,
                u.type,
                u.two_factor_authentication,
                u.created_at,
                u.blog,
                u.login_case,
                u.fetched_timestamp
            FROM github_users u
            WHERE login = ?1
            ORDER BY u.fetched_timestamp DESC, u.created_at DESC
            LIMIT 1", &login.to_ascii_lowercase());
        Ok(if let Some((fetched_at, user)) = res? {
            if allow_stale || fetched_at + 3600*24*31 > SystemTime::UNIX_EPOCH.elapsed().unwrap().as_secs() {
                Some(user)
            } else {
                info!("ignoring stale gh user data for {}", user.login);
                None
            }
        } else { None })
    }

    fn user_by_query(&self, query: &str, arg: &dyn ToSql) -> Result<Option<(u64, User)>> {
        let conn = self.conn.lock();
        let mut get_user = conn.prepare_cached(query)?;
        let mut res = get_user.query_map([arg], Self::read_user_row)?;
        res.next().transpose()
    }

    /// Timestamp + user
    fn read_user_row(row: &Row) -> Result<(u64, User), Error> {
        let fetched_at = row.get_ref_unwrap(11)
            .as_i64_or_null()?.unwrap_or(0) as u64;
        let mut login = row.get_ref_unwrap(10).as_str_or_null()?.map(Ok)
            .unwrap_or_else(|| row.get_ref_unwrap(1).as_str())?;
        let name = row.get_ref_unwrap(2).as_str_or_null()?
            .filter(|n| !n.is_empty());
        if let Some(name) = name {
            if name.eq_ignore_ascii_case(login) {
                login = name;
            }
        }
        Ok((fetched_at, User {
            id: row.get_unwrap(0),
            login: login.into(),
            name: name.map(From::from),
            avatar_url: row.get_unwrap::<_, Option<Box<str>>>(3).filter(|n| !n.is_empty()),
            gravatar_id: row.get_unwrap::<_, Option<Box<str>>>(4).filter(|n| !n.is_empty()),
            html_url: row.get_unwrap(5),
            user_type: match row.get_ref_unwrap(6).as_str().unwrap() {
                "org" => UserType::Org,
                "bot" => UserType::Bot,
                _ => UserType::User,
            },
            two_factor_authentication: row.get_unwrap(7),
            created_at: row.get_unwrap::<_, Option<Box<str>>>(8).filter(|n| !n.is_empty()),
            blog: row.get_unwrap::<_, Option<Box<str>>>(9).filter(|n| !n.is_empty()),
        }))
    }

    /// Not possible via GitHub API any more
    pub fn login_by_github_id(&self, id: u64) -> Result<String> {
        let conn = self.conn.lock();
        let mut get_user = conn.prepare_cached(r"SELECT login FROM github_users WHERE id = ?1
            ORDER BY fetched_timestamp DESC, created_at DESC
            LIMIT 1")?;
        get_user.query_row([&id], |row| row.get(0))
    }

    pub fn user_by_email(&self, email: &str) -> Result<Option<User>> {
        let std_suffix = "@users.noreply.github.com";
        if let Some(rest) = email.strip_suffix(std_suffix) {
            if let Some(id) = rest.split('+').next().and_then(|id| id.parse().ok()) {
                if let Ok(login) = self.login_by_github_id(id) {
                    // don't lose reliable association only because other metadata is stale
                    return self.user_by_github_login_opt(&login, true);
                }
            }
        }
        if let Some(u) = self.user_by_email_inner(email)? {
            return Ok(Some(u));
        }
        if let Some(fallback) = unplussed(email) {
            return self.user_by_email_inner(&fallback);
        }
        Ok(None)
    }

    fn user_by_email_inner(&self, email: &str) -> Result<Option<User>> {
        let res = self.user_by_query(r"SELECT
                u.id,
                u.login,
                COALESCE(u.name, e.name) as name,
                u.avatar_url,
                u.gravatar_id,
                u.html_url,
                u.type,
                u.two_factor_authentication,
                u.created_at,
                u.blog,
                u.login_case,
                u.fetched_timestamp
            FROM github_emails e
            JOIN github_users u ON e.github_id = u.id
            WHERE email = ?1
            ORDER BY u.fetched_timestamp DESC, u.created_at DESC
            LIMIT 1", &email.to_ascii_lowercase());
        // must allow stale data, beacuse the caller won't know which user to refresh!
        Ok(res?.map(|(_, u)| u))
    }

    pub fn index_users(&self, users: &[User], fetched_at: Option<SystemTime>) -> Result<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        Self::insert_users_inner(&tx, users, fetched_at)?;
        tx.commit()?;
        Ok(())
    }

    fn insert_users_inner(tx: &Transaction, users: &[User], fetched_at: Option<SystemTime>) -> Result<(), Error> {
        // timestamp is missing on bulk updates from crates-io datadump, which may be stale
        let mut insert_user = tx.prepare_cached(if fetched_at.is_some() {
            "INSERT INTO github_users (
                id, login, name, avatar_url, gravatar_id, html_url, type, two_factor_authentication, created_at, blog, login_case, fetched_timestamp)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                ON CONFLICT(id, login) DO UPDATE SET
                login = excluded.login,
                login_case = excluded.login_case,
                name = COALESCE(excluded.name, name),
                avatar_url = COALESCE(excluded.avatar_url, avatar_url),
                gravatar_id = COALESCE(excluded.gravatar_id, gravatar_id),
                html_url = excluded.html_url,
                type = excluded.type,
                two_factor_authentication = COALESCE(excluded.two_factor_authentication, two_factor_authentication),
                created_at = COALESCE(excluded.created_at, created_at),
                fetched_timestamp = excluded.fetched_timestamp,
                blog = COALESCE(excluded.blog, blog)
                "}
            else {
                "INSERT OR IGNORE INTO github_users (
                id, login, name, avatar_url, gravatar_id, html_url, type, two_factor_authentication, created_at, blog, login_case, fetched_timestamp)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            "})?;
        let timestamp = fetched_at.and_then(|f| f.duration_since(SystemTime::UNIX_EPOCH).ok()).unwrap_or_default().as_secs();
        for user in users {
            let login_lowercase = user.login.to_ascii_lowercase();
            let args: &[&dyn ToSql] = &[
                &user.id,
                &login_lowercase,
                &user.name.as_deref().filter(|n| !n.eq_ignore_ascii_case(&login_lowercase)),
                &user.avatar_url.as_deref().filter(|n| !n.is_empty()),
                &user.gravatar_id.as_deref().filter(|n| !n.is_empty()),
                &user.html_url,
                &match user.user_type {
                    UserType::User => "user",
                    UserType::Org => "org",
                    UserType::Bot => "bot",
                },
                &user.two_factor_authentication,
                &user.created_at.as_deref().filter(|n| !n.is_empty()),
                &user.blog.as_deref().filter(|n| !n.is_empty()),
                &if user.login != login_lowercase { Some(user.login.as_str()) } else { None },
                &timestamp,
            ];
            insert_user.execute(args)?;
        }
        Ok(())
    }

    pub fn index_user(&self, user: &User, email: Option<&str>, name: Option<&str>) -> Result<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        {
            Self::insert_users_inner(&tx, std::slice::from_ref(user), Some(SystemTime::now()))?;
            if let Some(e) = email {
                let mut insert_email = tx.prepare_cached("INSERT OR REPLACE INTO github_emails (
                    github_id, email, name) VALUES (?1, ?2, ?3)")?;
                let name = name.filter(|n| !n.trim_start().is_empty() && !n.eq_ignore_ascii_case(&user.login));
                let email = e.to_ascii_lowercase();
                let args: &[&dyn ToSql] = &[&user.id, &email, &name];
                insert_email.execute(args)?;

                if let Some(plain) = unplussed(&email) {
                    let args: &[&dyn ToSql] = &[&user.id, &plain, &name];
                    insert_email.execute(args)?;
                }
            }
        }
        tx.commit()?;
        Ok(())
    }
}

fn unplussed(email: &str) -> Option<String> {
    let mut parts = email.split('+');
    let u = parts.next()?;
    let rest = parts.next()?.split('@').nth(1)?;
    Some(format!("{u}@{rest}"))
}

#[test]
fn userdb() {
    let _ = std::fs::remove_dir("/tmp/userdbtest5.db");
    let u = UserDb::new("/tmp/userdbtest5.db").unwrap();
    u.index_users(&[User {
        id: 1,
        login: "HELLO".into(),
        name: None,
        avatar_url: None,
        gravatar_id: None,
        html_url: "bla".into(),
        blog: None,
        two_factor_authentication: Some(false),
        user_type: UserType::Org,
        created_at: None,
    }], Some(SystemTime::now())).unwrap();

    assert_eq!("hello", u.login_by_github_id(1).unwrap());
    let res = u.user_by_github_login("HellO").unwrap().unwrap();

    assert_eq!(1, res.id);
    assert_eq!("bla", &*res.html_url);
    assert_eq!(UserType::Org, res.user_type);
    assert_eq!(Some(false), res.two_factor_authentication);

    u.index_users(&[User {
        id: 1,
        login: "HELlO".into(),
        name: None,
        avatar_url: None,
        gravatar_id: None,
        html_url: "bla2".into(),
        blog: None,
        two_factor_authentication: None,
        user_type: UserType::User,
        created_at: Some("2020-02-20".into()),
    }], Some(SystemTime::now())).unwrap();
    let res = u.user_by_github_login("HellO").unwrap().unwrap();

    assert_eq!(1, res.id);
    assert_eq!("bla2", &*res.html_url);
    assert_eq!(UserType::User, res.user_type);
    assert_eq!(Some(false), res.two_factor_authentication);
    assert_eq!("2020-02-20", res.created_at.as_deref().unwrap());

    assert_eq!("hello", u.login_by_github_id(1).unwrap());

    u.index_users(&[User {
        id: 1,
        login: "changed_login".into(),
        name: None,
        avatar_url: None,
        gravatar_id: None,
        html_url: "bla2".into(),
        blog: None,
        two_factor_authentication: None,
        user_type: UserType::User,
        created_at: Some("2020-02-20".into()),
    }], Some(SystemTime::now())).unwrap();
    assert_eq!("changed_login", u.login_by_github_id(1).unwrap());
}
