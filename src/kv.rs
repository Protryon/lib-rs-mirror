use crate::error::Error;
use crate::SimpleCache;
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
use std::fs::File;
use std::io::BufReader;
use std::io::BufWriter;
use std::marker::PhantomData;
use std::path::PathBuf;
use tempfile::NamedTempFile;

type FxHashMap<K, V> = std::collections::HashMap<K, V, ahash::RandomState>;

struct Inner {
    data: Option<FxHashMap<Box<str>, Box<[u8]>>>,
    writes: usize,
    next_autosave: usize,
}

pub struct TempCache<T: Serialize + DeserializeOwned + Clone + Send> {
    path: PathBuf,
    data: RwLock<Inner>,
    _ty: PhantomData<T>,
    pub cache_only: bool,
    sem: tokio::sync::Semaphore,
}

impl<T: Serialize + DeserializeOwned + Clone + Send> TempCache<T> {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, Error> {
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
            sem: tokio::sync::Semaphore::new(32),
        })
    }

    #[inline]
    pub fn set(&self, key: impl Into<String>, value: impl Borrow<T>) -> Result<(), Error> {
        self.set_(key.into().into_boxed_str(), value.borrow())
    }

    fn lock_for_write(&self) -> Result<RwLockWriteGuard<'_, Inner>, Error> {
        let mut inner = self.data.write();
        if inner.data.is_none() {
            inner.data = Some(self.load_data()?);
        }
        Ok(inner)
    }

    fn lock_for_read(&self) -> Result<RwLockReadGuard<'_, Inner>, Error> {
        loop {
            let inner = self.data.read();
            if inner.data.is_some() {
                return Ok(inner);
            }
            drop(inner);
            let _ = self.lock_for_write()?;
        }
    }

    fn load_data(&self) -> Result<FxHashMap<Box<str>, Box<[u8]>>, Error> {
        let mut f = BufReader::new(File::open(&self.path)?);
        Ok(rmp_serde::from_read(&mut f).map_err(|e| {
            eprintln!("File {} is broken: {}", self.path.display(), e);
            e
        })?)
    }

    pub fn set_(&self, key: Box<str>, value: &T) -> Result<(), Error> {
        let mut e = DeflateEncoder::new(Vec::new(), Compression::best());
        rmp_serde::encode::write_named(&mut e, value)?;
        let compr = e.finish()?;

        let _ = Self::ungz(&compr)?; // sanity check

        let mut w = self.lock_for_write()?;
        w.writes += 1;
        w.data.as_mut().unwrap().insert(key, compr.into_boxed_slice());
        if w.writes >= w.next_autosave {
            w.writes = 0;
            w.next_autosave *= 2;
            drop(w); // unlock writes
            self.save()?;
        }
        Ok(())
    }

    pub fn delete(&self, key: &str) -> Result<(), Error> {
        let mut d = self.lock_for_write()?;
        if d.data.as_mut().unwrap().remove(key).is_some() {
            d.writes += 1;
        }
        Ok(())
    }

    // pub fn get_all<F: FnOnce(&HashMap<Box<str>, T>)>(&self, cb: F) -> Result<(), Error> {
    //     cb(&self.lock_for_read()?.data);
    //     Ok(())
    // }

    pub fn get(&self, key: &str) -> Result<Option<T>, Error> {
        let kw = self.lock_for_read()?;
        Ok(match kw.data.as_ref().unwrap().get(key) {
            Some(gz) => Some(Self::ungz(gz).map_err(|e| {
                eprintln!("ungz of {} failed in {}", key, self.path.display());
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
        let tmp_path = NamedTempFile::new_in(self.path.parent().expect("tmp"))?;
        let mut file = BufWriter::new(File::create(&tmp_path)?);
        let d = self.lock_for_read()?;
        rmp_serde::encode::write(&mut file, d.data.as_ref().unwrap())?;
        drop(d);
        tmp_path.persist(&self.path).map_err(|e| e.error)?;
        Ok(())
    }

    #[inline]
    pub async fn get_json<B>(&self, key: &str, url: impl AsRef<str>, on_miss: impl FnOnce(B) -> Option<T>) -> Result<Option<T>, Error>
    where B: for<'a> Deserialize<'a> {
        if let Some(res) = self.get(key)? {
            return Ok(Some(res));
        }

        if self.cache_only {
            return Ok(None);
        }

        let _s = self.sem.acquire().await;
        let data = SimpleCache::fetch(url.as_ref()).await?;
        match serde_json::from_slice(&data) {
            Ok(res) => {
                let res = on_miss(res);
                if let Some(ref res) = res {
                    self.set(key, res)?
                }
                Ok(res)
            },
            Err(parse) => Err(Error::Parse(parse, data)),
        }
    }
}

impl<T: Serialize + DeserializeOwned + Clone + Send> Drop for TempCache<T> {
    fn drop(&mut self) {
        if self.data.read().writes > 0 {
            if let Err(err) = self.save() {
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
