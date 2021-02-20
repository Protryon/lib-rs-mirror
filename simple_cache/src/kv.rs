use std::sync::Arc;
use crate::error::Error;
use fetcher::Fetcher;
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use flate2::Compression;
use parking_lot::RwLock;
use parking_lot::RwLockReadGuard;
use parking_lot::RwLockWriteGuard;
use rmp_serde;
use serde::de::DeserializeOwned;
use serde::*;
use serde_json;
use std::borrow::Borrow;
use std::collections::hash_map::Entry;
use std::fs::File;
use std::hash::Hash;
use std::io::BufReader;
use std::io::BufWriter;
use std::marker::PhantomData;
use std::path::PathBuf;
use tempfile::NamedTempFile;

type FxHashMap<K, V> = std::collections::HashMap<K, V, ahash::RandomState>;

struct Inner<K> {
    data: Option<FxHashMap<K, Box<[u8]>>>,
    writes: usize,
    next_autosave: usize,
}

pub struct TempCacheJson<T: Serialize + DeserializeOwned + Clone + Send, K: Serialize + DeserializeOwned + Clone + Send + Eq + Hash = Box<str>> {
    cache: TempCache<T, K>,
    fetcher: Arc<Fetcher>,
}

pub struct TempCache<T: Serialize + DeserializeOwned + Clone + Send, K: Serialize + DeserializeOwned + Clone + Send + Eq + Hash = Box<str>> {
    path: PathBuf,
    data: RwLock<Inner<K>>,
    _ty: PhantomData<T>,
    pub cache_only: bool,
}

impl<T: Serialize + DeserializeOwned + Clone + Send, K: Serialize + DeserializeOwned + Clone + Send + Eq + Hash> TempCacheJson<T, K> {
    pub fn new(path: impl Into<PathBuf>, fetcher: Arc<Fetcher>) -> Result<Self, Error> {
        Ok(Self {
            fetcher,
            cache: TempCache::new(path)?,
        })
    }
}

impl<T: Serialize + DeserializeOwned + Clone + Send, K: Serialize + DeserializeOwned + Clone + Send + Eq + Hash> TempCache<T, K> {
    pub fn new(path: impl Into<PathBuf>,) -> Result<Self, Error> {
        let path = path.into().with_extension("rmpz");
        let data = if path.exists() {
            None
        } else {
            Some(FxHashMap::default())
        };

        Ok(Self {
            path,
            data: RwLock::new(Inner {
                data,
                writes: 0,
                next_autosave: 10,
            }),
            _ty: PhantomData,
            cache_only: false,
        })
    }

    #[inline]
    pub fn set(&self, key: impl Into<K>, value: impl Borrow<T>) -> Result<(), Error> {
        self.set_(key.into(), value.borrow())
    }

    fn lock_for_write(&self) -> Result<RwLockWriteGuard<'_, Inner<K>>, Error> {
        let mut inner = self.data.write();
        if inner.data.is_none() {
            inner.data = Some(self.load_data()?);
        }
        Ok(inner)
    }

    fn lock_for_read(&self) -> Result<RwLockReadGuard<'_, Inner<K>>, Error> {
        loop {
            let inner = self.data.read();
            if inner.data.is_some() {
                return Ok(inner);
            }
            drop(inner);
            let _ = self.lock_for_write()?;
        }
    }

    fn load_data(&self) -> Result<FxHashMap<K, Box<[u8]>>, Error> {
        let mut f = BufReader::new(File::open(&self.path)?);
        Ok(rmp_serde::from_read(&mut f).map_err(|e| {
            eprintln!("File {} is broken: {}", self.path.display(), e);
            e
        })?)
    }

    pub fn set_(&self, key: K, value: &T) -> Result<(), Error> {
        let mut e = DeflateEncoder::new(Vec::new(), Compression::best());
        rmp_serde::encode::write_named(&mut e, value)?;
        let compr = e.finish()?;

        debug_assert!(Self::ungz(&compr).is_ok()); // sanity check

        let mut w = self.lock_for_write()?;
        let compr = compr.into_boxed_slice();
        match w.data.as_mut().unwrap().entry(key) {
            Entry::Vacant(e) => { e.insert(compr); },
            Entry::Occupied(mut e) => {
                if e.get() == &compr {
                    return Ok(());
                }
                e.insert(compr);
            },
        }
        w.writes += 1;
        if w.writes >= w.next_autosave {
            w.writes = 0;
            w.next_autosave *= 2;
            drop(w); // unlock writes
            let d = self.lock_for_read()?;
            self.save_unlocked(&d)?;
        }
        Ok(())
    }

    pub fn delete<Q>(&self, key: &Q) -> Result<(), Error> where K: Borrow<Q>, Q: Eq + Hash + ?Sized {
        let mut d = self.lock_for_write()?;
        if d.data.as_mut().unwrap().remove(key).is_some() {
            d.writes += 1;
        }
        Ok(())
    }

    pub fn get<Q>(&self, key: &Q) -> Result<Option<T>, Error> where K: Borrow<Q>, Q: Eq + Hash + std::fmt::Debug + ?Sized {
        let kw = self.lock_for_read()?;
        Ok(match kw.data.as_ref().unwrap().get(key) {
            Some(gz) => Some(Self::ungz(gz).map_err(|e| {
                eprintln!("ungz of {:?} failed in {}", key, self.path.display());
                drop(kw);
                let _ = self.delete(key);
                e
            })?),
            None => None,
        })
    }

    fn ungz(data: &[u8]) -> Result<T, Error> {
        let ungz = DeflateDecoder::new(data);
        Ok(rmp_serde::decode::from_read(ungz)?)
    }

    pub fn save(&self) -> Result<(), Error> {
        let mut data = self.data.write();
        if data.writes > 0 {
            self.save_unlocked(&data)?;
            data.data = None; // Flush mem
        }
        Ok(())
    }

    fn save_unlocked(&self, d: &Inner<K>) -> Result<(), Error> {
        if let Some(data) = d.data.as_ref() {
            let tmp_path = NamedTempFile::new_in(self.path.parent().expect("tmp"))?;
            let mut file = BufWriter::new(File::create(&tmp_path)?);
            rmp_serde::encode::write(&mut file, data)?;
            tmp_path.persist(&self.path).map_err(|e| e.error)?;
        }
        Ok(())
    }
}

impl<T: Serialize + DeserializeOwned + Clone + Send, K: Serialize + DeserializeOwned + Clone + Send + Eq + Hash> TempCacheJson<T, K> {
    #[inline(always)]
    pub fn cache_only(&self) -> bool {
        self.cache.cache_only
    }

    #[inline(always)]
    pub fn set_cache_only(&mut self, b: bool) {
        self.cache.cache_only = b;
    }

    #[inline(always)]
    pub fn get<Q>(&self, key: &Q) -> Result<Option<T>, Error> where K: Borrow<Q>, Q: Eq + Hash + std::fmt::Debug + ?Sized {
        self.cache.get(key)
    }

    #[inline(always)]
    pub fn set(&self, key: impl Into<K>, value: impl Borrow<T>) -> Result<(), Error> {
        self.cache.set(key, value)
    }

    #[inline(always)]
    pub fn delete<Q>(&self, key: &Q) -> Result<(), Error> where K: Borrow<Q>, Q: Eq + Hash + ?Sized {
        self.cache.delete(key)
    }

    #[inline(always)]
    pub fn save(&self) -> Result<(), Error> {
        self.cache.save()
    }

    pub async fn get_json<Q, B>(&self, key: &Q, url: impl AsRef<str>, on_miss: impl FnOnce(B) -> Option<T>) -> Result<Option<T>, Error>
    where B: for<'a> Deserialize<'a>, K: Borrow<Q> + for<'a> From<&'a Q>, Q: Eq + Hash + std::fmt::Debug + ?Sized {
        if let Some(res) = self.cache.get(key)? {
            return Ok(Some(res));
        }

        if self.cache.cache_only {
            return Ok(None);
        }

        let data = Box::pin(self.fetcher.fetch(url.as_ref())).await?;
        match serde_json::from_slice(&data) {
            Ok(res) => {
                let res = on_miss(res);
                if let Some(ref res) = res {
                    self.cache.set(key, res)?
                }
                Ok(res)
            },
            Err(parse) => Err(Error::Parse(parse, data)),
        }
    }
}

impl<T: Serialize + DeserializeOwned + Clone + Send, K: Serialize + DeserializeOwned + Clone + Send + Eq + Hash> Drop for TempCache<T, K> {
    fn drop(&mut self) {
        let d = self.data.read();
        if d.writes > 0 {
            if let Err(err) = self.save_unlocked(&d) {
                eprintln!("Temp db save failed: {}", err);
            }
        }
    }
}

#[test]
fn kvtest() {
    let tmp: TempCache<(String, String)> = TempCache::new("/tmp/rmptest.bin").unwrap();
    tmp.set("hello", &("world".to_string(), "etc".to_string())).unwrap();
    let res = tmp.get("hello").unwrap().unwrap();
    drop(tmp);
    assert_eq!(res, ("world".to_string(), "etc".to_string()));

    let tmp2: TempCache<(String, String)> = TempCache::new("/tmp/rmptest.bin").unwrap();
    let res2 = tmp2.get("hello").unwrap().unwrap();
    assert_eq!(res2, ("world".to_string(), "etc".to_string()));
}
