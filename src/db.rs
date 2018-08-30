use reqwest;
use rusqlite;
use serde;
use serde_json;
use rusqlite::Connection;
use std::path::Path;
use thread_local::ThreadLocal;
use error::Error;
use std::thread;
use std::time::Duration;

#[derive(Debug)]
pub struct SimpleCache {
    url: String,
    conn: ThreadLocal<Result<Connection, rusqlite::Error>>,
    pub cache_only: bool,
}

impl SimpleCache {
    pub fn new(db_path: impl AsRef<Path>) -> Result<Self, Error> {
        Ok(Self {
            url: format!("file:{}?cache=shared", db_path.as_ref().display()),
            conn: ThreadLocal::new(),
            cache_only: false,
        })
    }

    fn connect(&self) -> Result<Connection, rusqlite::Error> {
        let conn = Connection::open(&self.url)?;
        conn.execute_batch("
            CREATE TABLE IF NOT EXISTS cache2 (key TEXT NOT NULL PRIMARY KEY, ver TEXT NOT NULL, data BLOB NOT NULL);
            PRAGMA synchronous = 0;
            PRAGMA JOURNAL_MODE = OFF;
            PRAGMA read_uncommitted;")?;
        Ok(conn)
    }

    #[inline]
    fn with_connection<F, T>(&self, cb: F) -> Result<T, Error> where F: FnOnce(&Connection) -> Result<T, Error> {
        let conn = self.conn.get_or(|| Box::new(self.connect()));
        match conn {
            Ok(conn) => cb(conn),
            Err(err) => Err(Error::Other(err.to_string())),
        }
    }

    pub fn get_json<B>(&self, key: (&str, &str), url: impl AsRef<str>) -> Result<Option<B>, Error>
        where B: for<'a> serde::Deserialize<'a>
    {
        if let Some(data) = self.get_cached(key, url)? {
            match serde_json::from_slice(&data) {
                Ok(res) => Ok(Some(res)),
                Err(parse) => Err(Error::Parse(parse, data)),
            }
        } else {
            Ok(None)
        }
    }

    pub fn get(&self, key: (&str, &str)) -> Result<Option<Vec<u8>>, Error> {
        self.with_connection(|conn| {
            let mut q = conn.prepare_cached("SELECT data FROM cache2 WHERE key = ?1 AND ver = ?2")?;
            let row: Result<Vec<u8>, _> = q.query_row(&[&key.0, &key.1], |r| r.get(0));
            match row {
                Ok(row) => Ok(Some(row)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(err) => Err(err)?,
            }
        })
    }

    pub fn get_cached(&self, key: (&str, &str), url: impl AsRef<str>) -> Result<Option<Vec<u8>>, Error> {
        Ok(if let Some(data) = self.get(key)? {
            Some(data)
        } else {
            if self.cache_only {
                None
            } else {
                eprintln!("cache miss {}@{}", key.0, key.1);
                let data = self.fetch(url.as_ref())?;
                self.set(key, &data)?;
                Some(data)
            }
        })
    }

    pub fn delete(&self, key: (&str, &str)) -> Result<(), Error> {
        self.with_connection(|conn| {
            let mut q = conn.prepare_cached("DELETE FROM cache2 WHERE key = ?1")?;
            q.execute(&[&key.0])?;
            Ok(())
        })
    }

    pub fn set(&self, key: (&str, &str), data: &[u8]) -> Result<(), Error> {
        self.with_connection(|conn| {
            let mut q = conn.prepare_cached("INSERT OR REPLACE INTO cache2(key, ver, data) VALUES(?1, ?2, ?3)")?;
            q.execute(&[&key.0, &key.1, &data])?;
            Ok(())
        })
    }

    fn fetch(&self, url: &str) -> Result<Vec<u8>, Error> {
        let client = reqwest::Client::new();
        if url.contains("crates.io") {
            // Please don't remove this.
            thread::sleep(Duration::from_secs(1));
        }
        let mut res = client.get(url)
            .header(reqwest::header::UserAgent::new("crates.rs/1.0"))
            .send()?;
        if res.status() != reqwest::StatusCode::Ok {
            Err(res.status())?;
        }
        let mut buf = Vec::new();
        res.copy_to(&mut buf)?;
        Ok(buf)
    }
}

