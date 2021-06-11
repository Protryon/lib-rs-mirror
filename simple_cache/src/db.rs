use crate::error::Error;
use fetcher::Fetcher;
use std::sync::Arc;

use rusqlite::types::ToSql;
use rusqlite::Connection;
use rusqlite::Error::SqliteFailure;
use rusqlite::ErrorCode::DatabaseLocked;

use brotli::BrotliCompress;
use brotli::BrotliDecompress;

use std::path::Path;
use std::thread;
use std::time::Duration;
use thread_local::ThreadLocal;

#[derive(Debug)]
pub struct SimpleCache {
    url: String,
    conn: ThreadLocal<Result<Connection, rusqlite::Error>>,
    pub cache_only: bool,
    compress: bool,
}

#[derive(Debug)]
pub struct SimpleFetchCache {
    cache: SimpleCache,
    fetcher: Arc<Fetcher>,
}

impl SimpleFetchCache {
    pub fn new(db_path: impl AsRef<Path>, fetcher: Arc<Fetcher>, compress: bool) -> Result<Self, Error> {
        Ok(Self {
            cache: SimpleCache::new(db_path, compress)?,
            fetcher,
        })
    }

    pub async fn get_json<B>(&self, key: (&str, &str), url: impl AsRef<str>) -> Result<Option<B>, Error>
    where B: for<'a> serde::Deserialize<'a> {
        if let Some(data) = self.get_cached(key, url).await? {
            match serde_json::from_slice(&data) {
                Ok(res) => Ok(Some(res)),
                Err(parse) => Err(Error::Parse(parse, data)),
            }
        } else {
            Ok(None)
        }
    }

    pub async fn get_cached(&self, key: (&str, &str), url: impl AsRef<str>) -> Result<Option<Vec<u8>>, Error> {
        Ok(if let Some(data) = tokio::task::block_in_place(|| self.cache.get(key))? {
            Some(data)
        } else if self.cache.cache_only {
            None
        } else {
            let data = Box::pin(self.fetcher.fetch(url.as_ref())).await?;
            tokio::task::block_in_place(|| self.cache.set(key, &data))?;
            Some(data)
        })
    }


    pub fn set_serialize<B: serde::Serialize>(&self, key: (&str, &str), value: &B) -> Result<(), Error> {
        tokio::task::block_in_place(|| self.cache.set_serialize(key, value))
    }

    pub fn get_deserialized<B: serde::de::DeserializeOwned>(&self, key: (&str, &str)) -> Result<Option<B>, Error> {
        tokio::task::block_in_place(|| self.cache.get_deserialized(key))
    }

    pub async fn fetch_cached_deserialized<B: serde::de::DeserializeOwned + serde::Serialize>(&self, key: (&str, &str), url: impl AsRef<str>) -> Result<Option<B>, Error> {
        if let Some(data) = self.get_deserialized(key)? {
            Ok(Some(data))
        } else if self.cache.cache_only {
            Ok(None)
        } else {
            let data = Box::pin(self.fetcher.fetch(url.as_ref())).await?;
            tokio::task::block_in_place(|| {
                let res = serde_json::from_slice(&data).map_err(|e| Error::Parse(e, data))?;
                self.cache.set_serialize(key, &res)?;
                Ok(Some(res))
            })
        }
    }

    pub fn set_cache_only(&mut self, val: bool) {
        self.cache.cache_only = val;
    }
}

impl SimpleCache {
    pub fn new(db_path: impl AsRef<Path>, compress: bool) -> Result<Self, Error> {
        Ok(Self {
            url: format!("file:{}?cache=shared", db_path.as_ref().display()),
            conn: ThreadLocal::new(),
            cache_only: false,
            compress,
        })
    }

    fn connect(&self) -> Result<Connection, rusqlite::Error> {
        let conn = Connection::open(&self.url)?;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS cache2 (key TEXT NOT NULL PRIMARY KEY, ver TEXT NOT NULL, data BLOB NOT NULL);
            PRAGMA synchronous = 0;
            PRAGMA JOURNAL_MODE = WAL;
            PRAGMA read_uncommitted;",
        )?;
        Ok(conn)
    }

    #[inline]
    fn with_connection<F, T>(&self, cb: F) -> Result<T, Error> where F: FnOnce(&Connection) -> Result<T, Error> {
        let conn = self.conn.get_or(|| self.connect());
        match conn {
            Ok(conn) => cb(conn),
            Err(err) => Err(Error::Other(err.to_string())),
        }
    }

    pub fn get(&self, key: (&str, &str)) -> Result<Option<Vec<u8>>, Error> {
        Self::with_retries(|| self.get_inner(key))
    }

    fn with_retries<T>(mut cb: impl FnMut() -> Result<T, Error>) -> Result<T, Error> {
        let mut retries = 5;
        loop {
            match cb() {
                Err(Error::Db(SqliteFailure(ref e, _))) if retries > 0 && e.code == DatabaseLocked => {
                    eprintln!("Retrying: {}", e);
                    retries -= 1;
                    thread::sleep(Duration::from_secs(1));
                },
                err => return err,
            }
        }
    }

    fn get_inner(&self, key: (&str, &str)) -> Result<Option<Vec<u8>>, Error> {
        self.with_connection(|conn| {
            let mut q = conn.prepare_cached("SELECT data FROM cache2 WHERE key = ?1 AND ver = ?2")?;
            let row: Result<Vec<u8>, _> = q.query_row(&[&key.0, &key.1], |r| r.get(0));
            match row {
                Ok(row) => Ok(Some(row)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(err) => Err(err.into()),
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

    pub fn set_serialize<B: serde::Serialize>(&self, key: (&str, &str), value: &B) -> Result<(), Error> {
        let serialized = rmp_serde::encode::to_vec_named(value)?;
        self.set(key, &serialized)
    }

    pub fn get_deserialized<B: serde::de::DeserializeOwned>(&self, key: (&str, &str)) -> Result<Option<B>, Error> {
        Ok(match self.get(key)? {
            None => None,
            Some(data) => {
                match Self::deserialize_compressed(&data) {
                    Ok(res) => Some(res),
                    Err(e) => {
                        log::error!("Deserialization error: {} for {:?}", e, key);
                        self.delete(key)?;
                        None
                    },
                }
            },
        })
    }

    pub(crate) fn deserialize_compressed<B: serde::de::DeserializeOwned>(mut data: &[u8]) -> Result<B, Error> {
        let mut decomp = Vec::with_capacity(data.len() * 2);
        BrotliDecompress(&mut data, &mut decomp)?;
        Self::deserialize(&decomp)
    }

    pub(crate) fn deserialize<B: serde::de::DeserializeOwned>(data: &[u8]) -> Result<B, Error> {
        Ok(rmp_serde::decode::from_slice(data)?)
    }

    pub fn set(&self, key: (&str, &str), mut data_in: &[u8]) -> Result<(), Error> {
        let mut out;
        let data = if self.compress {
            out = Vec::with_capacity(data_in.len() / 2);
            BrotliCompress(&mut data_in, &mut out, &Default::default())?;
            &out
        } else {
            data_in
        };
        Self::with_retries(|| self.set_inner(key, data))
    }

    fn set_inner(&self, key: (&str, &str), data: &[u8]) -> Result<(), Error> {
        self.with_connection(|conn| {
            let mut q = conn.prepare_cached("INSERT OR REPLACE INTO cache2(key, ver, data) VALUES(?1, ?2, ?3)")?;
            let arr: &[&dyn ToSql] = &[&key.0, &key.1, &data];
            q.execute(arr)?;
            Ok(())
        })
    }
}
