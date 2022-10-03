use std::io::Seek;
use std::time::Duration;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::atomic::AtomicU64;
use std::path::Path;
use std::time::SystemTime;
use crate::error::Error;
use fetcher::Fetcher;
use parking_lot::RwLock;
use parking_lot::RwLockReadGuard;
use parking_lot::RwLockWriteGuard;
use std::sync::Arc;
use log::{debug, info, warn, error};

use serde::de::DeserializeOwned;
use serde::*;

use std::borrow::Borrow;
use std::collections::hash_map::Entry;
use std::fs::File;
use std::hash::Hash;
use std::io::BufReader;
use std::io::BufWriter;
use std::marker::PhantomData;
use std::path::PathBuf;
use tempfile::NamedTempFile;
#[cfg(target_os = "macos")]
use std::os::macos::fs::MetadataExt;
#[cfg(target_os = "linux")]
use std::os::linux::fs::MetadataExt;

type FxHashMap<K, V> = std::collections::HashMap<K, V, ahash::RandomState>;

#[derive(Serialize, Deserialize, Clone)]
struct RawEntry {
    compressed: Box<[u8]>,
    /// 0 = old data, add default expiry
    expiry_timestamp: u32,
}

struct Inner<K> {
    data: Option<FxHashMap<K, RawEntry>>,
    writes: usize,
    next_autosave: usize,
    expected_size: AtomicU64,
}

pub struct TempCacheJson<T: Serialize + DeserializeOwned + Clone + Send, K: Serialize + DeserializeOwned + Clone + Send + Eq + Hash = Box<str>> {
    cache: TempCache<T, K>,
    fetcher: Arc<Fetcher>,
}

pub struct TempCache<T: Serialize + DeserializeOwned + Clone + Send, K: Serialize + DeserializeOwned + Clone + Send + Eq + Hash = Box<str>> {
    path: PathBuf,
    inner: RwLock<Inner<K>>,
    _ty: PhantomData<T>,
    pub cache_only: bool,
    default_expiry: Duration,
}

impl<T: Serialize + DeserializeOwned + Clone + Send, K: Serialize + DeserializeOwned + Clone + Send + Eq + Hash> TempCacheJson<T, K> {
    pub fn new(path: impl AsRef<Path>, fetcher: Arc<Fetcher>, default_expiry: Duration) -> Result<Self, Error> {
        Ok(Self {
            fetcher,
            cache: TempCache::new(path, default_expiry)?,
        })
    }
}

impl<T: Serialize + DeserializeOwned + Clone + Send, K: Serialize + DeserializeOwned + Clone + Send + Eq + Hash> TempCache<T, K> {
    pub fn new(path: impl AsRef<Path>, default_expiry: Duration) -> Result<Self, Error> {
        let base_path = path.as_ref();
        let path = base_path.with_extension("mpbr");

        let data = if path.exists() {
            None
        } else {
            Some(FxHashMap::default())
        };

        Ok(Self {
            path,
            inner: RwLock::new(Inner {
                data,
                writes: 0,
                next_autosave: 10,
                expected_size: AtomicU64::new(0),
            }),
            _ty: PhantomData,
            cache_only: false,
            default_expiry,
        })
    }

    #[inline]
    pub fn set(&self, key: impl Into<K>, value: impl Borrow<T>) -> Result<(), Error> {
        self.set_(key.into(), value.borrow())
    }

    #[track_caller]
    fn lock_for_write(&self) -> Result<RwLockWriteGuard<'_, Inner<K>>, Error> {
        if let Some(inner) = self.inner.try_write() {
            if inner.data.is_some() {
                return Ok(inner);
            }
        }
        tokio::task::block_in_place(|| {
            let mut inner = self.inner.try_write_for(Duration::from_secs(4)).ok_or(Error::Timeout)?;
            if inner.data.is_none() {
                let (size, data) = self.load_data()?;
                inner.expected_size = AtomicU64::new(size);
                inner.data = Some(data);
                inner.writes = 0;
                inner.next_autosave = 10;
            }
            Ok(inner)
        })
    }

    #[track_caller]
    fn lock_for_read(&self) -> Result<RwLockReadGuard<'_, Inner<K>>, Error> {
        if let Some(inner) = self.inner.try_read() {
            if inner.data.is_some() {
                return Ok(inner);
            }
        }
        tokio::task::block_in_place(|| {
            for _ in 0..5 {
                let inner = self.inner.try_read_for(Duration::from_secs(6)).ok_or(Error::Timeout)?;
                if inner.data.is_some() {
                    return Ok(inner);
                }
                drop(inner);
                let _ = self.lock_for_write()?;
            }
            Err(Error::Timeout)
        })
    }

    fn load_data(&self) -> Result<(u64, FxHashMap<K, RawEntry>), Error> {
        let f = File::open(&self.path)?;
        let file_size = f.metadata()?.st_size();
        let mut f = BufReader::new(f);
        let data: FxHashMap<K, RawEntry> = match rmp_serde::from_read(&mut f) {
            Ok(data) => data,
            Err(e) => {
                warn!("Trying to upgrade the file {}: {e}", self.path.display());
                f.rewind()?;
                let data2: FxHashMap<K, Box<[u8]>> = rmp_serde::from_read(&mut f).map_err(|e| {
                    error!("File {} is broken: {}", self.path.display(), e);
                    e
                })?;
                let expiry_timestamp = if self.default_expiry == Duration::ZERO { 0 } else { Self::expiry(self.default_expiry) };
                let mut fuzzy_expiry = 0u32;
                data2.into_iter().map(|(k,v)| {
                    fuzzy_expiry = fuzzy_expiry.wrapping_add(17291);
                    (k, RawEntry { compressed: v, expiry_timestamp: expiry_timestamp % fuzzy_expiry })
                }).collect()
            },
        };
        Ok((file_size, data))
    }

    #[inline]
    fn expiry(when: Duration) -> u32 {
        let timestamp_base = SystemTime::UNIX_EPOCH.elapsed().unwrap().as_secs();
        ((timestamp_base + when.as_secs()) >> 2) as u32 // that's one way to solve timestamp overflow
    }

    fn serialize(value: &T) -> Result<Vec<u8>, Error> {
        let mut e = brotli::CompressorWriter::new(Vec::new(), 1<<16, 7, 18);
        rmp_serde::encode::write_named(&mut e, value)?;
        Ok(e.into_inner())
    }

    #[track_caller]
    pub fn set_(&self, key: K, value: &T) -> Result<(), Error> {
        let compr = Self::serialize(value)?;
        debug_assert!(Self::unbr(&compr).is_ok()); // sanity check

        let mut w = self.lock_for_write()?;
        match w.data.as_mut().unwrap().entry(key) {
                Entry::Vacant(e) => { e.insert(RawEntry {
                    compressed: compr.into_boxed_slice(),
                    expiry_timestamp: 0,
                });
            },
            Entry::Occupied(mut e) => {
                if &*e.get().compressed == compr.as_slice() {
                    return Ok(());
                }
                e.insert(RawEntry {
                    compressed: compr.into_boxed_slice(),
                    expiry_timestamp: 0,
                });
            },
        }
        w.writes += 1;
        if w.writes >= w.next_autosave {
            w.writes = 0;
            w.next_autosave *= 2;
            drop(w); // unlock writes
            tokio::task::block_in_place(|| {
                let d = self.lock_for_read()?;
                if !self.save_unlocked(&d)? {
                    warn!("Data write race; discarding {}", self.path.display());
                    let mut w = self.lock_for_write()?;
                    w.data = None;
                }
                Ok::<_, Error>(())
            })?;
        }
        Ok(())
    }

    #[track_caller]
    pub fn for_each(&self, mut cb: impl FnMut(&K, T)) -> Result<(), Error> {
        tokio::task::block_in_place(|| {
            let kw = self.lock_for_read()?;
            kw.data.as_ref().unwrap().iter().try_for_each(|(k, v)| {
                let v = Self::unbr(&v.compressed)?;
                cb(k, v);
                Ok(())
            })
        })
    }

    pub fn delete<Q>(&self, key: &Q) -> Result<(), Error> where K: Borrow<Q>, Q: Eq + Hash + ?Sized {
        let mut d = self.lock_for_write()?;
        if d.data.as_mut().unwrap().remove(key).is_some() {
            d.writes += 1;
        }
        Ok(())
    }

    #[track_caller]
    pub fn get<Q>(&self, key: &Q) -> Result<Option<T>, Error> where K: Borrow<Q>, Q: Eq + Hash + std::fmt::Debug + ?Sized {
        let kw = self.lock_for_read()?;
        Ok(match kw.data.as_ref().unwrap().get(key) {
            Some(br) => Some(Self::unbr(&br.compressed).map_err(|e| {
                error!("unbr of {:?} failed in {}", key, self.path.display());
                drop(kw);
                let _ = self.delete(key);
                e
            })?),
            None => None,
        })
    }

    fn unbr(data: &[u8]) -> Result<T, Error> {
        let unbr = brotli::Decompressor::new(data, 1<<16);
        Ok(rmp_serde::decode::from_read(unbr)?)
    }

    pub fn save(&self) -> Result<(), Error> {
        tokio::task::block_in_place(|| {
            let mut data = self.inner.write();
            if data.writes > 0 {
                self.expire_old(&mut data);
                self.save_unlocked(&data)?;
                data.data = None; // Flush mem
            }
            Ok(())
        })
    }

    fn expire_old(&self, d: &mut Inner<K>) {
        if self.default_expiry == Duration::ZERO {
            return;
        }
        if let Some(data) = d.data.as_mut() {
            let mut n = 0;
            let expiry_end = Self::expiry(Duration::ZERO);
            data.retain(|_, entry| {
                if entry.expiry_timestamp == 0 || entry.expiry_timestamp > expiry_end {
                    true
                } else {
                    n += 1;
                    false
                }
            });
            if n > 0 {
                info!("expired {n} entries from {}", self.path.display());
            }
        }
    }

    fn save_unlocked(&self, d: &Inner<K>) -> Result<bool, Error> {
        if let Some(data) = d.data.as_ref() {
            debug!("saving {}", self.path.display());
            let tmp_path = NamedTempFile::new_in(self.path.parent().expect("tmp"))?;
            let mut file = BufWriter::new(File::create(&tmp_path)?);

            rmp_serde::encode::write(&mut file, data)?;
            // checked after encode to minimize race condition time window
            let on_disk_size = std::fs::metadata(&self.path).ok().map(|m| m.st_size());
            let expected_size = d.expected_size.load(SeqCst);
            if expected_size > 0 && on_disk_size.map_or(false, |s| s != expected_size) {
                return Ok(false);
            }
            let new_size = file.into_inner()
                .map_err(|e| Error::Other(format!("{} @ {}", e.error(), self.path.display())))? // uuuuugh
                .metadata()?.st_size();
            d.expected_size.store(new_size, SeqCst);
            tmp_path.persist(self.path.with_extension("mpbr")).map_err(|e| e.error)?;
        }
        Ok(true)
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
        let mut d = self.inner.write();
        if d.writes > 0 {
            self.expire_old(&mut d);
            if let Err(err) = self.save_unlocked(&d) {
                error!("Temp db save failed: {}", err);
            }
        }
    }
}

#[test]
fn kvtest() {
    let tmp: TempCache<(String, String)> = TempCache::new("/tmp/rmptest.bin", Duration::from_secs(1234)).unwrap();
    tmp.set("hello", &("world".to_string(), "etc".to_string())).unwrap();
    let res = tmp.get("hello").unwrap().unwrap();
    drop(tmp);
    assert_eq!(res, ("world".to_string(), "etc".to_string()));

    let tmp2: TempCache<(String, String)> = TempCache::new("/tmp/rmptest.bin", Duration::from_secs(1234)).unwrap();
    let res2 = tmp2.get("hello").unwrap().unwrap();
    assert_eq!(res2, ("world".to_string(), "etc".to_string()));
}
