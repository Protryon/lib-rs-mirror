use github_info::User;
use github_info::UserType;
use parking_lot::Mutex;
use rusqlite::types::ToSql;
use rusqlite::*;
use std::path::Path;
type Result<T, E = rusqlite::Error> = std::result::Result<T, E>;

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
        get_email.exists(&[&email.to_ascii_lowercase()])
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
                u.type,
                u.two_factor_authentication,
                u.created_at,
                u.blog
            FROM github_users u
            WHERE login = ?1 LIMIT 1")?;
        let mut res = get_user.query_map(&[&login.to_lowercase()], Self::read_user_row)?;
        Ok(if let Some(res) = res.next() {
            Some(res?)
        } else {
            None
        })
    }

    fn read_user_row(row: &Row) -> Result<User, Error> {
        Ok(User {
            id: row.get_unwrap(0),
            login: row.get_unwrap(1),
            name: row.get_unwrap(2),
            avatar_url: row.get_unwrap(3),
            gravatar_id: row.get_unwrap(4),
            html_url: row.get_unwrap(5),
            user_type: match row.get_ref_unwrap(6).as_str().unwrap() {
                "org" => UserType::Org,
                "bot" => UserType::Bot,
                _ => UserType::User,
            },
            two_factor_authentication: row.get_unwrap(7),
            created_at: row.get_unwrap(8),
            blog: row.get_unwrap(9),
        })
    }

    /// Not possible via GitHub API any more
    pub fn login_by_github_id(&self, id: u64) -> Result<String> {
        let conn = self.conn.lock();
        let mut get_user = conn.prepare_cached(r"SELECT login FROM github_users WHERE id = ?1 LIMIT 1")?;
        get_user.query_row(&[&id], |row| row.get(0))
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
                u.type,
                u.two_factor_authentication,
                u.created_at,
                u.blog
            FROM github_emails e
            JOIN github_users u ON e.github_id = u.id
            WHERE email = ?1 LIMIT 1")?;
        let mut res = get_user.query_map(&[&email.to_ascii_lowercase()], Self::read_user_row)?;
        Ok(if let Some(res) = res.next() {
            Some(res?)
        } else {
            None
        })
    }

    pub fn index_users(&self, users: &[User]) -> Result<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        Self::insert_users_inner(&tx, users)?;
        tx.commit()?;
        Ok(())
    }

    fn insert_users_inner(tx: &Transaction, users: &[User]) -> Result<(), Error> {
        let mut insert_user = tx.prepare_cached("INSERT INTO github_users (
            id, login, name, avatar_url, gravatar_id, html_url, type, two_factor_authentication, created_at, blog)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ON CONFLICT(id, login) DO UpDATE SET
            login = excluded.login,
            name = COALESCE(excluded.name, name),
            avatar_url = COALESCE(excluded.avatar_url, avatar_url),
            gravatar_id = COALESCE(excluded.gravatar_id, gravatar_id),
            html_url = excluded.html_url,
            type = excluded.type,
            two_factor_authentication = COALESCE(excluded.two_factor_authentication, two_factor_authentication),
            created_at = COALESCE(excluded.created_at, created_at),
            blog = COALESCE(excluded.blog, blog)
            ")?;
        for user in users {
            let args: &[&dyn ToSql] = &[
                &user.id,
                &user.login.to_ascii_lowercase(),
                &user.name,
                &user.avatar_url,
                &user.gravatar_id,
                &user.html_url,
                &match user.user_type {
                    UserType::User => "user",
                    UserType::Org => "org",
                    UserType::Bot => "bot",
                },
                &user.two_factor_authentication,
                &user.created_at,
                &user.blog,
            ];
            insert_user.execute(args)?;
        }
        Ok(())
    }

    pub fn index_user(&self, user: &User, email: Option<&str>, name: Option<&str>) -> Result<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        {
            Self::insert_users_inner(&tx, std::slice::from_ref(user))?;
            let mut insert_email = tx.prepare_cached("INSERT OR REPLACE INTO github_emails (
                github_id, email, name) VALUES (?1, ?2, ?3)")?;
            if let Some(e) = email {
                let args: &[&dyn ToSql] = &[&user.id, &e.to_ascii_lowercase(), &name];
                insert_email.execute(args)?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}

#[test]
fn userdb() {
    let _ = std::fs::remove_dir("/tmp/userdbtest3.db");
    let u = UserDb::new("/tmp/userdbtest3.db").unwrap();
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
    }]).unwrap();

    assert_eq!("hello", u.login_by_github_id(1).unwrap());
    let res = u.user_by_github_login("HellO").unwrap().unwrap();

    assert_eq!(1, res.id);
    assert_eq!("bla", res.html_url);
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
    }]).unwrap();
    let res = u.user_by_github_login("HellO").unwrap().unwrap();

    assert_eq!(1, res.id);
    assert_eq!("bla2", res.html_url);
    assert_eq!(UserType::User, res.user_type);
    assert_eq!(Some(false), res.two_factor_authentication);
    assert_eq!("2020-02-20", res.created_at.as_deref().unwrap());
}
