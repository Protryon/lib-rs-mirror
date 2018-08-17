extern crate reqwest;
extern crate thread_local;
extern crate rusqlite;
extern crate serde;
extern crate serde_json;
#[macro_use] extern crate quick_error;
use rusqlite::Connection;
use std::path::Path;
use thread_local::ThreadLocal;
mod error;
pub use error::Error;

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

    pub fn get_json<B>(&self, cache_name: &str, url: impl AsRef<str>) -> Result<B, Error>
        where B: for<'a> serde::Deserialize<'a>
    {
        let data = self.get_cached(cache_name, url)?;
        match serde_json::from_slice(&data) {
            Ok(res) => Ok(res),
            Err(parse) => Err(Error::Parse(parse, data)),
        }
    }

    pub fn get(&self, cache_name: &str) -> Result<Vec<u8>, Error> {
        self.with_connection(|conn| {
            let mut q = conn.prepare_cached("SELECT value FROM cache WHERE key = ?1")?;
            if let Ok(res) = q.query_row(&[&cache_name], |r| r.get(0)) {
                Ok(res)
            } else {
                Err(Error::NotCached)
            }
        })
    }

    pub fn get_cached(&self, cache_name: &str, url: impl AsRef<str>) -> Result<Vec<u8>, Error> {
        Ok(if let Ok(data) = self.get(cache_name) {
            data
        } else {
            if self.cache_only {
                return Err(Error::NotCached);
            }
            let data = self.fetch(url.as_ref())?;
            self.set(cache_name, &data)?;
            data
        })
    }

    pub fn delete(&self, cache_name: &str) -> Result<(), Error> {
        let conn = self.conn.lock().unwrap();
        let mut q = conn.prepare_cached("DELETE FROM cache WHERE key = ?1")?;
        q.execute(&[&cache_name])?;
        Ok(())
    }

    pub fn set(&self, cache_name: &str, data: &[u8]) -> Result<(), Error> {
        self.with_connection(|conn| {
            let mut q = conn.prepare_cached("INSERT OR REPLACE INTO cache(key, value) VALUES(?1, ?2)")?;
            q.execute(&[&cache_name, &data])?;
            Ok(())
        })
    }

    fn fetch(&self, url: &str) -> Result<Vec<u8>, Error> {
        eprintln!("cache miss {}", url);
        let client = reqwest::Client::new();
        if url.contains("crates.io") {
            // Please don't remove this.
            std::thread::sleep(std::time::Duration::from_secs(1));
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

