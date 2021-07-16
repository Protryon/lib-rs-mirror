use crate::MiniVer;
use parking_lot::Mutex;
use simple_cache::TempCache;
use std::collections::hash_map::Entry::*;
use std::collections::HashMap;
use serde_big_array::BigArray;
use std::path::PathBuf;
use std::sync::Arc;

/// Downloads each day of the year
#[derive(Serialize, Deserialize, Clone)]
pub struct DailyDownloads(
    #[serde(with = "BigArray")]
    pub [u32; 366]
);

#[derive(Serialize, Deserialize, Clone)]
pub struct AnnualDownloads(
    #[serde(with = "BigArray")]
    pub [u64; 366]
);

type PerVersionDownloads = HashMap<MiniVer, DailyDownloads>;
type ByYearCache = HashMap<u16, Arc<TempCache<PerVersionDownloads>>>;

pub struct AllDownloads {
    by_year: Mutex<ByYearCache>,
    base_path: PathBuf,
    sum_cache: TempCache<AnnualDownloads, u16>,
}

impl AllDownloads {
    /// Dir where to store years
    pub fn new(base_path: impl Into<PathBuf>) -> Self {
        let base_path = base_path.into();
        Self {
            sum_cache: TempCache::new(base_path.join("yearlydownloads")).unwrap(),
            by_year: Mutex::new(HashMap::new()),
            base_path,
        }
    }

    fn get_cache<'t>(&self, t: &'t mut parking_lot::MutexGuard<ByYearCache>, year: u16) -> Result<&'t mut Arc<TempCache<HashMap<MiniVer, DailyDownloads>>>, crates_io_client::Error> {
        Ok(match t.entry(year) {
            Occupied(e) => e.into_mut(),
            Vacant(e) => {
                e.insert(Arc::new(TempCache::new(self.base_path.join(format!("{}-big.rmpz", year)))?))
            },
        })
    }

    /// Crates.io crate name
    pub fn get_crate_year(&self, crate_name: &str, year: u16) -> Result<Option<PerVersionDownloads>, simple_cache::Error> {
        let mut t = self.by_year.lock();
        let cache = self.get_cache(&mut t, year)?;
        cache.get(crate_name)
    }

    pub fn set_crate_year(&self, crate_name: &str, year: u16, v: &PerVersionDownloads) -> Result<(), simple_cache::Error> {
        self.sum_cache.delete(&year)?;
        let mut t = self.by_year.lock();
        let cache = self.get_cache(&mut t, year)?;
        cache.set(crate_name, v)?;
        Ok(())
    }

    fn get_full_year(&self, year: u16) -> Result<Arc<TempCache<PerVersionDownloads>>, simple_cache::Error> {
        let mut t = self.by_year.lock();
        let cache = self.get_cache(&mut t, year)?;
        Ok(Arc::clone(&cache))
    }

    pub fn total_year_downloads(&self, year: u16) -> Result<[u64; 366], simple_cache::Error> {
        if let Some(res) = self.sum_cache.get(&year)? {
            return Ok(res.0);
        }
        let mut summed_days = [0u64; 366];
        if let Ok(year) = self.get_full_year(year) {
            year.for_each(|_, crate_year| {
                for (_, days) in crate_year {
                    for (sd, vd) in summed_days.iter_mut().zip(days.0.iter().copied()) {
                        *sd += vd as u64;
                    }
                }
            })?;
        }
        self.sum_cache.set(year, AnnualDownloads(summed_days))?;
        Ok(summed_days)
    }

    pub fn save(&self) -> Result<(), simple_cache::Error> {
        self.sum_cache.save()?;
        let t = self.by_year.lock();
        for y in t.values() {
            y.save()?;
        }
        Ok(())
    }
}

impl Default for DailyDownloads {
    fn default() -> Self {
        Self([0; 366])
    }
}
