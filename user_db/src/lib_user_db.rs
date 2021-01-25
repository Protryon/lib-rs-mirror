use github_info::User;
use github_info::UserType;
use parking_lot::Mutex;
use rusqlite::types::ToSql;
use rusqlite::*;
use std::path::Path;
type Result<T, E = failure::Error> = std::result::Result<T, E>;

mod schema;

pub struct UserDb {
    pub(crate) conn: Mutex<Connection>,
}

impl UserDb {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let db = Self::db(path.as_ref())?;
        db.execute_batch("
            PRAGMA synchronous = 0;
            PRAGMA journal_mode = WAL;")?;
        Ok(Self {
            conn: Mutex::new(db),
        })
    }

    pub fn email_has_github(&self, email: &str) -> Result<bool> {
        let conn = self.conn.lock();
        let mut get_email = conn.prepare_cached("SELECT 1 FROM github_emails WHERE email = ?1 LIMIT 1")?;
        Ok(get_email.exists(&[&email.to_ascii_lowercase()])?)
    }

    pub fn user_by_github_login(&self, login: &str) -> Result<Option<User>> {
        let conn = self.conn.lock();
        let mut get_user = conn.prepare_cached(r"SELECT
                u.id,
                u.login,
                u.name,
                u.avatar_url,
                u.gravatar_id,
                u.html_url,
                u.type
            FROM github_users u
            WHERE login = ?1 LIMIT 1")?;
        let mut res = get_user.query_map(&[&login.to_lowercase()], |row| {
            Ok(User {
                id: row.get_unwrap(0),
                login: row.get_unwrap(1),
                name: row.get_unwrap(2),
                avatar_url: row.get_unwrap(3),
                gravatar_id: row.get_unwrap(4),
                html_url: row.get_unwrap(5),
                blog: None,
                created_at: None,
                user_type: match row.get_raw(6).as_str().unwrap() {
                    "org" => UserType::Org,
                    "bot" => UserType::Bot,
                    _ => UserType::User,
                },
            })
        })?;
        Ok(if let Some(res) = res.next() {
            Some(res?)
        } else {
            None
        })
    }

    pub fn user_by_email(&self, email: &str) -> Result<Option<User>> {
        let conn = self.conn.lock();
        let mut get_user = conn.prepare_cached(r"SELECT
                u.id,
                u.login,
                u.name,
                u.avatar_url,
                u.gravatar_id,
                u.html_url,
                u.type
            FROM github_emails e
            JOIN github_users u ON e.github_id = u.id
            WHERE email = ?1 LIMIT 1")?;
        let mut res = get_user.query_map(&[&email.to_ascii_lowercase()], |row| {
            Ok(User {
                id: row.get_unwrap(0),
                login: row.get_unwrap(1),
                name: row.get_unwrap(2),
                avatar_url: row.get_unwrap(3),
                gravatar_id: row.get_unwrap(4),
                html_url: row.get_unwrap(5),
                blog: None,
                created_at: None,
                user_type: match row.get_raw(6).as_str().unwrap() {
                    "org" => UserType::Org,
                    "bot" => UserType::Bot,
                    _ => UserType::User,
                },
            })
        })?;
        Ok(if let Some(res) = res.next() {
            Some(res?)
        } else {
            None
        })
    }

    pub fn index_user(&self, user: &User, email: Option<&str>, name: Option<&str>) -> Result<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        {
            let mut insert_user = tx.prepare_cached("INSERT OR IGNORE INTO github_users (
                id, login, name, avatar_url, gravatar_id, html_url, type)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)")?;
            let mut insert_email = tx.prepare_cached("INSERT OR IGNORE INTO github_emails (
                github_id, email, name) VALUES (?1, ?2, ?3)")?;

            let t = match user.user_type {
                UserType::User => "user",
                UserType::Org => "org",
                UserType::Bot => "bot",
            };
            let args: &[&dyn ToSql] = &[&user.id, &user.login.to_ascii_lowercase(), &user.name, &user.avatar_url, &user.gravatar_id, &user.html_url, &t];
            insert_user.execute(args)?;

            if let Some(e) = email {
                let args: &[&dyn ToSql] = &[&user.id, &e.to_ascii_lowercase(), &name];
                insert_email.execute(args)?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}
