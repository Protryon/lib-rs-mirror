use std::borrow::Borrow;
use std::sync::RwLock;
use std::collections::HashMap;
use serde::de::DeserializeOwned;
use crate::error::Error;
use serde::*;
use rmp_serde;
use serde_json;
use std::path::PathBuf;
use std::fs::File;
use std::fs;
use tempfile::NamedTempFile;
use std::io::BufReader;
use crate::SimpleCache;

struct Inner<T> {
    data: T,
    writes: usize,
    next_autosave: usize,
}

pub struct TempCache<T: Serialize + DeserializeOwned + Clone + Send> {
    path: PathBuf,
    data: RwLock<Inner<HashMap<Box<str>, T>>>,
    pub cache_only: bool,
}

impl<T: Serialize + DeserializeOwned + Clone + Send> TempCache<T> {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, Error> {
        let path = path.into().with_extension("rmp");
        let data = if path.exists() {
            let mut f = BufReader::new(File::open(&path)?);
            rmp_serde::from_read(&mut f).map_err(|e| {
                eprintln!("File {} is broken: {}", path.display(), e);
                e
            })?
        } else {
            HashMap::new()
        };

        Ok(Self {
            path,
            data: RwLock::new(Inner {
                data,
                writes: 0,
                next_autosave: 10,
            }),
            cache_only: false,
        })
    }

    #[inline]
    pub fn set(&self, key: impl Into<String>, value: impl Borrow<T>) -> Result<(), Error> {
        self.set_(key.into().into_boxed_str(), value.borrow())
    }

    pub fn set_(&self, key: Box<str>, value: &T) -> Result<(), Error> {

        // sanity check
        let value = rmp_serde::to_vec(value).map_err(Error::from)
            .and_then(|dat| rmp_serde::from_slice(&dat).map_err(|e| {
                eprintln!("Setter for {} {} failed to serialize: {}", self.path.display(), key, e);
                Error::from(e)
            }))?;

        let mut w = self.data.write().map_err(|_| Error::KvPoison)?;
        w.writes += 1;
        w.data.insert(key, value);
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

    pub fn get_all<F: FnOnce(&HashMap<Box<str>, T>)>(&self, cb: F) -> Result<(), Error> {
        cb(&self.data.read().map_err(|_| Error::KvPoison)?.data);
        Ok(())
    }

    pub fn get(&self, key: &str) -> Result<Option<T>, Error> {
        Ok(self.data.read().map_err(|_| Error::KvPoison)?.data.get(key).cloned())
    }

    pub fn save(&self) -> Result<(), Error> {
        let ser = {
            let d = self.data.read().map_err(|_| Error::KvPoison)?;
            rmp_serde::to_vec_named(&d.data)?
        };
        let tmp_path = NamedTempFile::new_in(self.path.parent().unwrap())?;
        fs::write(&tmp_path, &ser)?;
        let _: HashMap<String, T> = rmp_serde::from_slice(&ser).map_err(|e| {
            eprintln!("{} can't be saved, because {}", self.path.display(), e);
            e
        })?;
        tmp_path.persist(&self.path).map_err(|e| e.error)?;
        Ok(())
    }


    #[inline]
    pub fn get_json<B>(&self, key: &str, url: impl AsRef<str>, cb: impl FnOnce(B) -> Option<T>) -> Result<Option<T>, Error>
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
                let res = cb(res);
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
    assert_eq!(res, ("world".to_string(), "etc".to_string()));
}
