use crate::MiniVer;
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

type PerVersionDownloads = HashMap<MiniVer, DailyDownloads>;

pub struct AllDownloads {
    by_year: Mutex<HashMap<u16, TempCache<PerVersionDownloads>>>,
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
    pub fn get_crate_year(&self, crate_name: &str, year: u16) -> Result<Option<PerVersionDownloads>, simple_cache::Error> {
        let mut t = self.by_year.lock();
        let cache = match t.entry(year) {
            Occupied(e) => e.into_mut(),
            Vacant(e) => {
                e.insert(TempCache::new(self.base_path.join(format!("{}-big.rmpz", year)))?)
            },
        };
        Ok(cache.get(crate_name)?)
    }

    pub fn set_crate_year(&self, crate_name: &str, year: u16, v: &PerVersionDownloads) -> Result<(), simple_cache::Error> {
        let mut t = self.by_year.lock();
        let cache = match t.entry(year) {
            Occupied(e) => e.into_mut(),
            Vacant(e) => {
                e.insert(TempCache::new(self.base_path.join(format!("{}-big.rmpz", year)))?)
            },
        };
        cache.set(crate_name, v)?;
        Ok(())
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

impl<'de> BigArray<'de> for [u32; 366] {
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
                for (i, a) in arr.iter_mut().enumerate() {
                    *a = seq.next_element()?
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

impl Default for DailyDownloads {
    fn default() -> Self {
        Self([0; 366])
    }
}
