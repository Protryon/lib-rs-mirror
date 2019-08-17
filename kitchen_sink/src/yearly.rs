use parking_lot::Mutex;
use simple_cache::TempCache;
use std::collections::hash_map::Entry::*;
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;

/// Downloads each day of the year
#[derive(Serialize, Deserialize, Clone)]
pub struct DailyDownloads(
    #[serde(with = "BigArray")]
    pub [u32; 366]
);

#[derive(Serialize, Deserialize, Clone)]
pub struct VersionMap {
    /// None means "other" in addition to versions explictly listed that day
    pub versions: HashMap<Option<Box<str>>, DailyDownloads>,
    #[serde(with = "BigArrayBool")]
    pub is_set: [bool; 366],
}

pub struct AllDownloads {
    by_year: Mutex<HashMap<u16, TempCache<VersionMap>>>,
    base_path: PathBuf,
}


impl AllDownloads {
    /// Dir where to store years
    pub fn new(base_path: impl Into<PathBuf>) -> Self {
        Self {
            by_year: Mutex::new(HashMap::new()),
            base_path: base_path.into(),
        }
    }

    /// Crates.io crate name
    pub fn get_crate_year(&self, crate_name: &str, year: u16) -> Result<Option<VersionMap>, simple_cache::Error> {
        let mut t = self.by_year.lock();
        let cache = match t.entry(year) {
            Occupied(e) => e.into_mut(),
            Vacant(e) => {
                e.insert(TempCache::new(self.base_path.join(format!("{}.rmpz", year)))?)
            },
        };
        Ok(cache.get(crate_name)?)
    }

    pub fn set_crate_year(&self, crate_name: &str, year: u16, v: &VersionMap) -> Result<(), simple_cache::Error> {
        for ver in v.versions.values() {
            assert!(ver.0.iter().cloned().zip(v.is_set.iter().cloned())
                .filter(|&(dl, _)| dl > 0)
                .all(|(_, is_set)| is_set));
        }
        let mut t = self.by_year.lock();
        let cache = match t.entry(year) {
            Occupied(e) => e.into_mut(),
            Vacant(e) => {
                e.insert(TempCache::new(self.base_path.join(format!("{}.rmpz", year)))?)
            },
        };
        cache.set(crate_name, v)?;
        Ok(())
    }
}

impl Default for VersionMap {
    fn default() -> Self {
        Self {
            is_set: [false; 366],
            versions: Default::default(),
        }
    }
}

// Serde workaround
use serde::de::{Deserializer, Error, SeqAccess, Visitor};
use serde::ser::{SerializeTuple, Serializer};

trait BigArray<'de>: Sized {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where S: Serializer;
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>;
}

impl<'de> BigArray<'de> for [u32; 366]
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where S: Serializer
    {
        let mut seq = serializer.serialize_tuple(self.len())?;
        for elem in &self[..] {
            seq.serialize_element(elem)?;
        }
        seq.end()
    }

    fn deserialize<D>(deserializer: D) -> Result<[u32; 366], D::Error>
        where D: Deserializer<'de>
    {
        struct ArrayVisitor;

        impl<'de> Visitor<'de> for ArrayVisitor {
            type Value = [u32; 366];

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(concat!("an array of length ", 366))
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<[u32; 366], A::Error>
                where A: SeqAccess<'de>
            {
                let mut arr = [u32::default(); 366];
                for i in 0..366 {
                    arr[i] = seq.next_element()?
                        .ok_or_else(|| Error::invalid_length(i, &self))?;
                }
                Ok(arr)
            }
        }

        let visitor = ArrayVisitor;
        deserializer.deserialize_tuple(366, visitor)
    }
}

trait BigArrayBool<'de>: Sized {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where S: Serializer;
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>;
}

impl<'de> BigArrayBool<'de> for [bool; 366]
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where S: Serializer
    {
        let mut seq = serializer.serialize_tuple(self.len())?;
        for elem in &self[..] {
            seq.serialize_element(elem)?;
        }
        seq.end()
    }

    fn deserialize<D>(deserializer: D) -> Result<[bool; 366], D::Error>
        where D: Deserializer<'de>
    {
        struct ArrayVisitor;

        impl<'de> Visitor<'de> for ArrayVisitor {
            type Value = [bool; 366];

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(concat!("an array of length ", 366))
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<[bool; 366], A::Error>
                where A: SeqAccess<'de>
            {
                let mut arr = [bool::default(); 366];
                for i in 0..366 {
                    arr[i] = seq.next_element()?
                        .ok_or_else(|| Error::invalid_length(i, &self))?;
                }
                Ok(arr)
            }
        }

        let visitor = ArrayVisitor;
        deserializer.deserialize_tuple(366, visitor)
    }
}

impl fmt::Debug for VersionMap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("VersionMap {…}")
    }
}
