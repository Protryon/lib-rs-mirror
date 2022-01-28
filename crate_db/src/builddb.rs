use parking_lot::Mutex;
use rich_crate::Origin;
use rusqlite::*;
use semver::Version as SemVer;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::path::Path;
use log::debug;

pub struct BuildDb {
    pub(crate) conn: Mutex<Connection>,
}

pub type RustcMinorVersion = u16;

#[derive(Debug, Clone)]
pub struct CompatibilityInfo {
    pub rustc_version: RustcMinorVersion, // 1.x.0
    pub crate_version: SemVer,
    pub compat: Compat,
}

#[derive(Debug, Clone, Default)]
pub struct CompatRange {
    // rustc version
    pub oldest_ok: Option<RustcMinorVersion>,
    pub newest_bad: Option<RustcMinorVersion>,
    pub newest_ok: Option<RustcMinorVersion>,
    pub oldest_ok_raw: Option<RustcMinorVersion>, // actual test data, no assumptions
    pub newest_bad_raw: Option<RustcMinorVersion>, // actual test data, no assumptions
}

pub type CompatByCrateVersion = BTreeMap<SemVer, CompatRange>;

#[derive(Debug)]
pub struct RustcCompatRange {
    // crate version
    pub crate_newest_ok: Option<SemVer>,
    pub crate_newest_bad: Option<SemVer>,
    pub crate_oldest_bad: Option<SemVer>,
}

#[derive(Debug)]
pub struct CrateCompatInfo {
    pub origin: Origin,
    pub rustc_versions: Vec<(RustcMinorVersion, RustcCompatRange)>,
    pub old_crates_broken_up_to: Option<SemVer>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
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
            CREATE UNIQUE INDEX IF NOT EXISTS build_results_ver on build_results(origin, version, rustc_version);
            ")?;
        Ok(Self {
            conn: Mutex::new(db),
        })
    }

    pub fn get_compat_raw(&self, origin: &Origin) -> Result<Vec<CompatibilityInfo>> {
        let conn = self.conn.lock();
        let mut get = conn.prepare_cached(r"SELECT rustc_version, version, compat FROM build_results WHERE origin = ?1")?;
        let origin_str = origin.to_str();
        let res = get.query_map(&[origin_str.as_str()], Self::compat_row)?;
        res.collect()
    }

    pub fn get_compat(&self, origin: &Origin) -> Result<CompatByCrateVersion> {
        let conn = self.conn.lock();
        let mut get = conn.prepare_cached(r"SELECT rustc_version, version, compat FROM build_results WHERE origin = ?1")?;
        let origin_str = origin.to_str();
        let mut rows = get.query(&[origin_str.as_str()])?;
        let mut compat = CompatByCrateVersion::new();
        while let Some(row) = rows.next()? {
            Self::append_compat(&mut compat, Self::compat_row(row)?);
        }

        let mut any_version_has_built = false;

        // some crates used 2018 edition without using 2018 features,
        // allowing very old edition-unaware Rust to compile it.
        for v in compat.values_mut() {
            if v.oldest_ok.is_some() {
                any_version_has_built = true;
            }

            if let (Some(oldest_ok), Some(newest_bad)) = (v.oldest_ok, v.newest_bad) {
                if oldest_ok <= newest_bad {
                    if oldest_ok < 29 {
                        v.oldest_ok = Some(newest_bad + 1);
                    } else {
                        v.newest_bad = Some(oldest_ok - 1);
                    }
                }
            }
        }

        if !any_version_has_built {
            // Remove broken data
            for c in compat.values_mut() {
                // if it never built, it may be garbage data
                // but keep deps broken info to help the builder narrow target ranges
                if c.oldest_ok.is_none() && c.newest_bad.is_some() {
                    debug!("{:?} never built; so ignoring failures ({:?})", origin, c.newest_bad);
                    c.newest_bad = None;
                }
            }
        }
        Ok(compat)
    }

    pub fn postprocess_compat(compat: &mut CompatByCrateVersion) {
        let mut prev_oldest_ok = None;
        for c in compat.values_mut().rev() {
            if let (Some(prev_ok), Some(newest_bad)) = (prev_oldest_ok, c.newest_bad) {
                // bad data or unexpected change in compatibility?
                if prev_ok < newest_bad {
                    prev_oldest_ok = None;
                }
            }

            // assume that if the new version built with old rust, then older version will too
            match (c.oldest_ok, prev_oldest_ok) {
                (None, _) => { c.oldest_ok = prev_oldest_ok; }
                (Some(curr), Some(prev)) if prev < curr => { c.oldest_ok = Some(prev); }
                (Some(curr), _) => { prev_oldest_ok = Some(curr); }
            }
        }

        // assume that once support for old Rust is dropped, it's not restored
        let mut prev_newest_bad = None;
        for c in compat.values_mut() {
            // if it never built, it may be garbage data
            if c.oldest_ok.is_none() {
                c.newest_bad = None;
            }

            if let (Some(prev_bad), Some(oldest_ok)) = (prev_newest_bad, c.oldest_ok) {
                // bad data or unexpected change in compatibility?
                if prev_bad >= oldest_ok {
                    prev_newest_bad = None;
                }
            }

            match (c.newest_bad, prev_newest_bad) {
                (None, _) => { c.newest_bad = prev_newest_bad; }
                (Some(curr), Some(prev)) if prev > curr => { c.newest_bad = Some(prev); }
                (Some(curr), _) => { prev_newest_bad = Some(curr); }
            }

            // fix data after all the changes
            if let (Some(oldest_ok), Some(newest_ok)) = (c.oldest_ok, c.newest_ok) {
                if newest_ok < oldest_ok {
                    c.newest_ok = Some(oldest_ok);
                }
            }
        }
    }

    pub fn get_all_compat_by_crate(&self) -> Result<HashMap<Origin, CompatByCrateVersion>> {
        let conn = self.conn.lock();
        let mut get = conn.prepare_cached(r"SELECT rustc_version, version, compat, origin FROM build_results")?;

        let mut by_crate = HashMap::with_capacity(20000);

        let mut rows = get.query([])?;
        while let Some(row) = rows.next()? {
            let compat = Self::compat_row(row)?;
            let origin = Origin::from_str(row.get_ref(3)?.as_str()?);

            let mut by_ver = by_crate.entry(origin).or_insert_with(BTreeMap::default);

            Self::append_compat(&mut by_ver, compat);
        }
        Ok(by_crate)
    }

    fn append_compat(by_ver: &mut CompatByCrateVersion, c: CompatibilityInfo) {
        let t = by_ver.entry(c.crate_version).or_insert_with(CompatRange::default);

        let rustc_version = c.rustc_version;
        match c.compat {
            Compat::VerifiedWorks | Compat::ProbablyWorks => {
                if t.oldest_ok.map_or(true, |v| rustc_version < v) {
                    t.oldest_ok = Some(rustc_version);
                    if c.compat != Compat::ProbablyWorks {
                        t.oldest_ok_raw = Some(rustc_version);
                    }
                }
                else if t.newest_ok.map_or(true, |v| rustc_version > v) {
                    t.newest_ok = Some(rustc_version);
                }
            },
            Compat::Incompatible => {
                if t.newest_bad_raw.map_or(true, |v| rustc_version > v) {
                    t.newest_bad_raw = Some(rustc_version);
                }
            },
            Compat::BrokenDeps => {
                if t.newest_bad.map_or(true, |v| rustc_version > v) {
                    t.newest_bad = Some(rustc_version);
                }
            },
        }

        if t.newest_bad.map_or(true, |v| v < t.newest_bad_raw.unwrap_or(0)) {
            t.newest_bad = t.newest_bad_raw;
        }
    }

    fn compat_row(row: &Row) -> Result<CompatibilityInfo> {
        let rustc_version = row.get_ref_unwrap(0).as_str()?;
        let crate_version = row.get_ref_unwrap(1).as_str()?;
        Ok(CompatibilityInfo {
            rustc_version: SemVer::parse(rustc_version).map_err(|e| Error::ToSqlConversionFailure(e.into()))?.minor as RustcMinorVersion,
            crate_version: garbage_parse(crate_version),
            compat: Compat::from_str(row.get_ref_unwrap(2).as_str()?),
        })
    }

    pub fn get_all_compat(&self) -> Result<Vec<CrateCompatInfo>> {
        let by_crate = self.get_all_compat_by_crate()?;

        Ok(by_crate.into_iter().map(|(origin, compat)| {
            let mut rustc_versions = BTreeSet::new();
            let mut old_crates_broken_up_to = None;
            let mut seen_any_ok_build = false;
            // versions are iterated from oldest thanks to btree
            for (crate_ver, c) in &compat {
                // there are pre-rust-1.0 crates that are broken in their 1.0 versions
                if !seen_any_ok_build {
                    if c.oldest_ok.is_some() || c.newest_ok.is_some() {
                        seen_any_ok_build = true;
                    } else if c.newest_bad.is_some() {
                        old_crates_broken_up_to = Some(crate_ver);
                    }
                }
                if let Some(v) = c.oldest_ok {rustc_versions.insert(v);}
                if let Some(v) = c.newest_bad {rustc_versions.insert(v);}
                if let Some(v) = c.newest_ok {rustc_versions.insert(v);}
            }
            // if everything is broken, we just don't know if it's a pre-1.0 problem or bad crate.
            // assume only 0.x crates are rotten. If someone released 1.x then it should have been usable!
            if !seen_any_ok_build || old_crates_broken_up_to.map_or(false, |v| v.major > 0) {
                old_crates_broken_up_to = None;
            }
            let rustc_versions = rustc_versions.into_iter().map(|rv| {
                let crate_oldest_bad = compat.iter()
                    .filter(|(crate_ver, _)| old_crates_broken_up_to.map_or(true, |b| *crate_ver > b))
                    .filter(|(_, rustc)| rustc.newest_bad.map_or(true, |v| rv <= v))
                    .map(|(crate_ver, _)| crate_ver).min();
                let crate_newest_bad = compat.iter()
                    .filter(|(_, rustc)| rustc.newest_bad.map_or(true, |v| rv <= v))
                    .map(|(crate_ver, _)| crate_ver).max();
                let crate_newest_ok = compat.iter()
                    .filter(|(crate_ver, _)| old_crates_broken_up_to.map_or(true, |b| *crate_ver > b))
                    .filter(|(_, rustc)| rustc.oldest_ok.map_or(true, |v| rv >= v))
                    .map(|(crate_ver, _)| crate_ver).max();
                (rv, RustcCompatRange {
                    crate_newest_bad: crate_newest_bad.cloned(),
                    crate_oldest_bad: crate_oldest_bad.cloned(),
                    crate_newest_ok: crate_newest_ok.cloned(),
                })
            }).collect();
            CrateCompatInfo {
                origin,
                rustc_versions,
                old_crates_broken_up_to: old_crates_broken_up_to.cloned(),
            }
        }).collect())
    }


    pub fn set_compat_multi(&self, rows: &[(&Origin, &str, &str, Compat)]) -> Result<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        {
            let mut clear_speculation = tx.prepare_cached(r"DELETE from build_results WHERE origin = ? AND version = ?2 AND rustc_version = ?3 AND (compat = 'n' OR compat = 'y')")?;
            let mut ins_ignore = tx.prepare_cached(r"INSERT OR IGNORE INTO build_results(origin, version, rustc_version, compat) VALUES(?1, ?2, ?3, ?4)")?;
            let mut ins_replace = tx.prepare_cached("INSERT OR REPLACE INTO build_results(origin, version, rustc_version, compat) VALUES(?1, ?2, ?3, ?4)")?;

            for (origin, ver, rustc_version, compat) in rows {
                let origin_str = origin.to_str();
                let result_str = compat.as_str();
                clear_speculation.execute(&[origin_str.as_str(), ver, rustc_version])?;
                // these are weak signals, so don't replace good info with them
                let ins = if *compat != Compat::VerifiedWorks { &mut ins_ignore } else { &mut ins_replace };
                ins.execute(&[origin_str.as_str(), ver, rustc_version, result_str])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn set_compat(&self, origin: &Origin, ver: &str, rustc_version: &str, compat: Compat) -> Result<()> {
        self.set_compat_multi(&[(origin, ver, rustc_version, compat)])
    }
}

fn garbage_parse(v: &str) -> SemVer {
    SemVer::parse(v).or_else(|_| SemVer::parse(v.split('-').next().unwrap())).unwrap_or_else(|_| SemVer {
        major: 0,
        minor: 0,
        patch: 0,
        pre: semver::Prerelease::EMPTY,
        build: semver::BuildMetadata::new("parse_error").expect("et tu"),
    })
}
