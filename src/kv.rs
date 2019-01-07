use std::borrow::Borrow;
use std::sync::RwLock;
use serde::de::DeserializeOwned;
use crate::error::Error;
use serde::*;
use rmp_serde;
use serde_json;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::fs::File;
use tempfile::NamedTempFile;
use std::io::BufWriter;
use std::io::BufReader;
use crate::SimpleCache;
use flate2::Compression;
use flate2::write::DeflateEncoder;
use flate2::read::DeflateDecoder;
use fxhash::FxHashMap;

struct Inner {
    data: FxHashMap<Box<str>, Box<[u8]>>,
    writes: usize,
    next_autosave: usize,
}

pub struct TempCache<T: Serialize + DeserializeOwned + Clone + Send> {
    path: PathBuf,
    data: RwLock<Inner>,
    _ty: PhantomData<T>,
    pub cache_only: bool,
}

impl<T: Serialize + DeserializeOwned + Clone + Send> TempCache<T> {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, Error> {
        let path = path.into().with_extension("rmpz");
        let data = if path.exists() {
            let mut f = BufReader::new(File::open(&path)?);
            rmp_serde::from_read(&mut f).map_err(|e| {
                eprintln!("File {} is broken: {}", path.display(), e);
                e
            })?
        } else {
            FxHashMap::default()
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
    pub fn set(&self, key: impl Into<String>, value: impl Borrow<T>) -> Result<(), Error> {
        self.set_(key.into().into_boxed_str(), value.borrow())
    }

    pub fn set_(&self, key: Box<str>, value: &T) -> Result<(), Error> {
        let mut e = DeflateEncoder::new(Vec::new(), Compression::best());
        rmp_serde::encode::write_named(&mut e, value)?;
        let compr = e.finish()?;

        let _ =Self::ungz(&compr)?; // sanity check

        let mut w = self.data.write().map_err(|_| Error::KvPoison)?;
        w.writes += 1;
        w.data.insert(key, compr.into_boxed_slice());
        if w.writes >= w.next_autosave {
            w.writes = 0;
            w.next_autosave *= 2;
            drop(w); // unlock writes
            self.save()?;
        }
        Ok(())
    }

    pub fn delete(&self, key: &str) -> Result<(), Error> {
        let mut d = self.data.write().map_err(|_| Error::KvPoison)?;
        if d.data.remove(key).is_some() {
            d.writes += 1;
        }
        Ok(())
    }

    // pub fn get_all<F: FnOnce(&HashMap<Box<str>, T>)>(&self, cb: F) -> Result<(), Error> {
    //     cb(&self.data.read().map_err(|_| Error::KvPoison)?.data);
    //     Ok(())
    // }

    pub fn get(&self, key: &str) -> Result<Option<T>, Error> {
        let kw = self.data.read().map_err(|_| Error::KvPoison)?;
        Ok(match kw.data.get(key) {
            Some(gz) => Some(Self::ungz(gz).map_err(|e| {
                eprintln!("ungz of {} failed", key);
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
        let tmp_path = NamedTempFile::new_in(self.path.parent().unwrap())?;
        let mut file = BufWriter::new(File::create(&tmp_path)?);
        let d = self.data.read().map_err(|_| Error::KvPoison)?;
        rmp_serde::encode::write(&mut file, &d.data)?;
        drop(d);
        tmp_path.persist(&self.path).map_err(|e| e.error)?;
        Ok(())
    }


    #[inline]
    pub fn get_json<B>(&self, key: &str, url: impl AsRef<str>, on_miss: impl FnOnce(B) -> Option<T>) -> Result<Option<T>, Error>
        where B: for<'a> Deserialize<'a>
    {
        if let Some(res) = self.get(key)? {
            return Ok(Some(res));
        }

        if self.cache_only {
            return Ok(None);
        }

        let data = SimpleCache::fetch(url.as_ref())?;
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
        if self.data.read().ok().map_or(true, |d| d.writes > 0) {
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
