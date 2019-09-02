use parking_lot::Mutex;
use rich_crate::Origin;
use rusqlite::*;
use semver::Version as SemVer;
use std::path::Path;
use std::collections::HashMap;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

pub struct BuildDb {
    pub(crate) conn: Mutex<Connection>,
}

#[derive(Debug, Clone)]
pub struct CompatibilityInfo {
    pub rustc_version: SemVer,
    pub crate_version: SemVer,
    pub compat: Compat,
}

#[derive(Debug)]
pub struct CompatRange {
    pub oldest_ok: SemVer,
    pub newest_bad: SemVer,
    pub newest_ok: SemVer,
}

#[derive(Debug)]
pub struct RustcCompatRange {
    pub newest_ok: Option<SemVer>,
    pub oldest_bad: Option<SemVer>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Compat {
    VerifiedWorks,
    ProbablyWorks,
    BrokenDeps,
    Incompatible,
}

impl Compat {
    pub fn from_str(s: &str) -> Self {
        match s {
            "Y" => Compat::VerifiedWorks,
            "y" => Compat::ProbablyWorks,
            "n" => Compat::BrokenDeps,
            "N" => Compat::Incompatible,
            _ => panic!("bad compat str {}", s),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Compat::VerifiedWorks => "Y",
            Compat::ProbablyWorks => "y",
            Compat::BrokenDeps => "n",
            Compat::Incompatible => "N",
        }
    }
}

impl BuildDb {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let db = Connection::open(path.as_ref())?;
        db.execute_batch("
            CREATE TABLE IF NOT EXISTS build_results (
                origin TEXT NOT NULL,
                version TEXT NOT NULL,
                rustc_version TEXT NOT NULL,
                compat TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS raw_builds (
                origin TEXT NOT NULL,
                version TEXT NOT NULL,
                stdout TEXT NOT NULL,
                stderr TEXT NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS builds_ver on raw_builds(origin, version);
            CREATE UNIQUE INDEX IF NOT EXISTS build_results_ver on build_results(origin, version, rustc_version);
            ")?;
        Ok(Self {
            conn: Mutex::new(db),
        })
    }

    pub fn get_compat(&self, origin: &Origin) -> Result<Vec<CompatibilityInfo>> {
        let conn = self.conn.lock();
        let mut get = conn.prepare_cached(r"SELECT rustc_version, version, compat FROM build_results WHERE origin = ?1")?;
        let origin_str = origin.to_str();
        let res = get.query_map(&[origin_str.as_str()], Self::compat_row)?;
        res.collect()
    }

    pub fn get_all_compat(&self) -> Result<Vec<(Origin, Vec<(SemVer, RustcCompatRange)>)>> {
        let conn = self.conn.lock();
        let mut get = conn.prepare_cached(r"SELECT rustc_version, version, compat, origin FROM build_results")?;

        let mut by_crate = HashMap::with_capacity(10000);

        let max_ver: SemVer = "999.999.999".parse().unwrap();
        let min_ver: SemVer = "0.0.0".parse().unwrap();

        for row in get.query_map(NO_PARAMS, |row| Ok((Origin::from_str(row.get_raw(3).as_str().unwrap()), Self::compat_row(row)?)))? {
            let (origin, compat) = row?;
            let by_ver = by_crate.entry(origin).or_insert_with(BTreeMap::default);
            let t = by_ver.entry(compat.crate_version).or_insert_with(|| CompatRange {
                oldest_ok: max_ver.clone(),
                newest_ok: min_ver.clone(),
                newest_bad: min_ver.clone(),
            });
            match compat.compat {
                Compat::VerifiedWorks | Compat::ProbablyWorks => {
                    if compat.rustc_version < t.oldest_ok {
                        t.oldest_ok = compat.rustc_version.clone();
                    }
                    if compat.rustc_version > t.newest_ok {
                        t.newest_ok = compat.rustc_version;
                    }
                },
                Compat::Incompatible | Compat::BrokenDeps => {
                    if compat.rustc_version > t.newest_bad {
                        t.newest_bad = compat.rustc_version;
                    }
                },
            }
        }
        Ok(by_crate.into_iter().map(|(origin, compat)| {
            let mut rustc_versions = BTreeSet::new();
            for (_, c) in &compat {
                if c.oldest_ok != max_ver {rustc_versions.insert(&c.oldest_ok);}
                if c.newest_bad != min_ver {rustc_versions.insert(&c.newest_bad);}
                if c.newest_ok != min_ver {rustc_versions.insert(&c.newest_ok);}
            }
            let rver = rustc_versions.into_iter().map(|rv| {
                let oldest_bad = compat.iter().filter(|(_, rustc)| rv <= &rustc.newest_bad).map(|(crate_ver, _)| crate_ver).min();
                let newest_ok = compat.iter().filter(|(_, rustc)| rv >= &rustc.oldest_ok).map(|(crate_ver, _)| crate_ver).max();
                (rv.clone(), RustcCompatRange {
                    oldest_bad: oldest_bad.cloned(),
                    newest_ok: newest_ok.cloned(),
                })
            }).collect();
            (origin, rver)
        }).collect())
    }

    fn compat_row(row: &Row) -> Result<CompatibilityInfo> {
        let compat = Compat::from_str(row.get_raw(2).as_str().expect("strtype"));
        Ok(CompatibilityInfo {
            rustc_version: SemVer::parse(row.get_raw(0).as_str().unwrap()).expect("semver"),
            crate_version: SemVer::parse(row.get_raw(1).as_str().unwrap()).expect("semver"),
            compat
        })
    }

    pub fn set_compat(&self, origin: &Origin, ver: &str, rustc_version: &str, compat: Compat) -> Result<()> {
        let conn = self.conn.lock();
        // these are weak info, so don't replace good info with them
        let mut ins = conn.prepare_cached(if compat != Compat::VerifiedWorks {
            r"INSERT OR IGNORE INTO build_results(origin, version, rustc_version, compat) VALUES(?1, ?2, ?3, ?4)"
        } else {
            "INSERT OR REPLACE INTO build_results(origin, version, rustc_version, compat) VALUES(?1, ?2, ?3, ?4)"
        })?;
        let origin_str = origin.to_str();
        let result_str = compat.as_str();
        ins.execute(&[origin_str.as_str(), ver, rustc_version, result_str])?;
        Ok(())
    }

    pub fn set_raw_build_info(&self, origin: &Origin, ver: &str, stdout: &str, stderr: &str) -> Result<()> {
        let conn = self.conn.lock();
        let mut ins = conn.prepare_cached(r"INSERT OR REPLACE INTO raw_builds(origin, version, stdout, stderr) VALUES(?1, ?2, ?3, ?4)")?;
        let origin_str = origin.to_str();
        ins.execute(&[origin_str.as_str(), ver, stdout, stderr])?;
        Ok(())
    }

    pub fn get_raw_build_info(&self, origin: &Origin, ver: &str) -> Result<Option<(String, String)>> {
        let conn = self.conn.lock();
        let mut get = conn.prepare_cached(r"SELECT stdout, stderr FROM raw_builds WHERE origin = ?1 AND version = ?2")?;
        let origin_str = origin.to_str();
        let res = get.query_map(&[origin_str.as_str(), ver], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;
        let mut res = res.collect::<Result<Vec<(String, String)>>>()?;
        Ok(res.pop())
    }
}
