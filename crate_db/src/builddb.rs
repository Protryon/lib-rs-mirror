use parking_lot::Mutex;
use rich_crate::Origin;
use rusqlite::*;
use semver::Version as SemVer;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::path::Path;
use log::{info, error};

pub struct BuildDb {
    pub(crate) conn: Mutex<Connection>,
}

pub type RustcMinorVersion = u16;

#[derive(Debug, Clone)]
pub struct CompatibilityInfo {
    pub rustc_version: RustcMinorVersion, // 1.x.0
    pub crate_version: SemVer,
    pub compat: Compat,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CompatRanges {
    has_ever_built: bool,
    ok: BTreeMap<RustcMinorVersion, (Compat, Option<String>)>,
    bad: BTreeMap<RustcMinorVersion, (Compat, Option<String>)>,
}

impl CompatRanges {
    pub fn has_ever_built(&self) -> bool {
        self.has_ever_built
    }

    /// note that it compiled fine with this version
    pub fn add_compat(&mut self, rustc_minor_ver: RustcMinorVersion, new_compat: Compat, reason: Option<String>) {
        if new_compat.successful() {
            if new_compat == Compat::VerifiedWorks {
                self.has_ever_built = true;
            }
            &mut self.ok
        } else {
            &mut self.bad
        }
        .entry(rustc_minor_ver)
            .and_modify(|existing_compat| {
                if new_compat.is_better(&existing_compat.0) {
                    *existing_compat = (new_compat, reason.clone());
                }
            })
            .or_insert((new_compat, reason));
    }

    pub fn compat_data_for_rustc(&self, rustc_version: RustcMinorVersion) -> Option<(Compat, Option<&str>)> {
        match (self.ok.get(&rustc_version), self.bad.get(&rustc_version)) {
            (Some((ok, ok_reason)), Some((bad, bad_reason))) => {
                Some(if ok.is_better(bad) {
                    (*ok, ok_reason.as_deref())
                } else {
                    (*bad, bad_reason.as_deref())
                })
            }
            (Some(any), _) | (_, Some(any)) => Some((any.0, any.1.as_deref())),
            _ => None,
        }
    }

    pub fn oldest_ok(&self) -> Option<RustcMinorVersion> {
        self.ok.keys().copied().next()
    }

    pub fn newest_bad(&self) -> Option<RustcMinorVersion> {
        self.bad.keys().rev().copied().next()
    }

    pub fn newest_bad_compat(&self) -> Option<(RustcMinorVersion, Compat)> {
        self.bad.iter().rev().map(|(&c, v)| (c, v.0)).next()
    }

    pub fn remove_uncertain_self_failures(&mut self) {
        self.bad.retain(|_, c| c.0 == Compat::DefinitelyIncompatible || c.0 == Compat::BrokenDeps);
    }

    pub fn normalize(&mut self) {
        // First delete false positives. Cargo has this problem that it fails open when
        // it's too old to even recognize new unstable features. Non-contiguous MSRV ranges would be confusing,
        // so we simplify it to first stable version that does support the feature.
        if let Some(newest_bad) = self.newest_bad_certain() {
            self.ok.retain(|&ok_ver, _| {
                ok_ver > newest_bad
            });
        }
        // Then remove all spurious failures
        if let Some(oldest_ok) = self.oldest_ok() {
            self.bad.retain(|&bad_ver, _| {
                bad_ver < oldest_ok
            });
        }
    }

    pub fn newest_bad_certain(&self) -> Option<RustcMinorVersion> {
        self.bad.iter().rev()
            .filter(|(_, c)| c.0 == Compat::DefinitelyIncompatible)
            .map(|(&v, _)| v)
            .next()
    }

    pub fn newest_bad_likely(&self) -> Option<RustcMinorVersion> {
        self.bad.iter().rev()
        // TODO: remove SuspectedIncompatible once we have data
            .filter(|(_, c)| c.0 == Compat::DefinitelyIncompatible || c.0 == Compat::LikelyIncompatible|| c.0 == Compat::SuspectedIncompatible)
            .map(|(&v, _)| v)
            .next()
    }

    pub fn oldest_ok_certain(&self) -> Option<RustcMinorVersion> {
        self.ok.iter()
            .filter(|(_, c)| c.0 == Compat::DefinitelyIncompatible)
            .map(|(&v, _)| v)
            .next()
    }

    pub fn all_rustc_versions(&self) -> impl Iterator<Item=RustcMinorVersion> + '_ {
        self.ok.keys().copied().chain(self.bad.keys().copied())
    }
}

pub type CompatByCrateVersion = BTreeMap<SemVer, CompatRanges>;

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
#[repr(u8)]
pub enum Compat {
    VerifiedWorks = b'Y',
    ProbablyWorks = b'y',
    BrokenDeps = b'n',
    SuspectedIncompatible = b'N',
    LikelyIncompatible = b'x',
    DefinitelyIncompatible = b'X',
}

impl Compat {
    #[track_caller]
    pub fn from_str(s: &str) -> Self {
        match s {
            "Y" => Compat::VerifiedWorks,
            "y" => Compat::ProbablyWorks,
            "n" => Compat::BrokenDeps,
            "N" => Compat::SuspectedIncompatible,
            "x" => Compat::LikelyIncompatible,
            "X" => Compat::DefinitelyIncompatible,
            _ => panic!("bad compat str {}", s),
        }
    }

    pub fn successful(&self) -> bool {
        matches!(self, Compat::VerifiedWorks | Compat::ProbablyWorks)
    }

    pub fn certainity(&self) -> u8 {
        match self {
            Compat::VerifiedWorks => 3,
            Compat::ProbablyWorks => 1,
            Compat::BrokenDeps => 0,
            Compat::SuspectedIncompatible => 1,
            Compat::LikelyIncompatible => 2,
            Compat::DefinitelyIncompatible => 3,
        }
    }

    pub fn is_better(&self, other: &Compat) -> bool {
        (self.successful() && !other.successful()) ||
            (self.successful() == other.successful() && self.certainity() > other.certainity())
    }

    pub fn as_char(self) -> char {
        self as u8 as char
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
                compat TEXT NOT NULL,
                reason TEXT
            );
            CREATE UNIQUE INDEX IF NOT EXISTS build_results_ver on build_results(origin, version, rustc_version);
            ")?;
        Ok(Self {
            conn: Mutex::new(db),
        })
    }

    pub fn get_compat_raw(&self, origin: &Origin) -> Result<Vec<CompatibilityInfo>> {
        let conn = self.conn.lock();
        let mut get = conn.prepare_cached(r"SELECT rustc_version, version, compat, reason FROM build_results WHERE origin = ?1")?;
        let origin_str = origin.to_str();
        let res = get.query_map(&[origin_str.as_str()], Self::compat_row)?;
        res.collect()
    }

    pub fn get_compat(&self, origin: &Origin) -> Result<CompatByCrateVersion> {
        let conn = self.conn.lock();
        let mut get = conn.prepare_cached(r"SELECT rustc_version, version, compat, reason FROM build_results WHERE origin = ?1")?;
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
            if v.has_ever_built() {
                any_version_has_built = true;
            }
            v.normalize();
        }

        if !any_version_has_built {
            // if it never built, it may be garbage data
            for c in compat.values_mut() {
                c.remove_uncertain_self_failures();
            }
        }
        Ok(compat)
    }

    pub fn postprocess_compat(compat: &mut CompatByCrateVersion) {
        let mut prev_oldest_ok = None;

        for c in compat.values_mut().rev() {
            if let (Some(prev_ok), Some(newest_bad)) = (prev_oldest_ok, c.newest_bad()) {
                // bad data or unexpected change in compatibility?
                if prev_ok < newest_bad {
                    prev_oldest_ok = None;
                }
            }

            // assume that if the new version built with old rust, then older version will too
            if let Some(prev_oldest_ok) = prev_oldest_ok {
                c.add_compat(prev_oldest_ok, Compat::ProbablyWorks, Some("assumed from newer version".into()));
            }
            prev_oldest_ok = c.oldest_ok();
        }

        // assume that once support for old Rust is dropped, it's not restored
        let mut prev_newest_bad = None;
        for (ver, c) in compat.iter_mut() {
            // if it never built, it may be garbage data
            if !c.has_ever_built() {
                c.remove_uncertain_self_failures();
            }

            if let (Some((prev_bad, _)), Some(oldest_ok)) = (prev_newest_bad, c.oldest_ok()) {
                // bad data or unexpected change in compatibility?
                if prev_bad >= oldest_ok {
                    prev_newest_bad = None;
                }
            }

            if let Some((prev_newest_bad, compat)) = prev_newest_bad {
                c.add_compat(prev_newest_bad, match compat {
                    Compat::VerifiedWorks => Compat::ProbablyWorks,
                    Compat::DefinitelyIncompatible | Compat::LikelyIncompatible => Compat::SuspectedIncompatible,
                    other => other,
                }, Some("assumed from older version".into()));
            }

            // skip over prerelease versions, because breakage during beta may not be indicative of stable versions
            if ver.pre.is_empty() {
                prev_newest_bad = c.newest_bad_compat();
            }

            // fix data after all the changes
            c.normalize();
        }
    }

    pub fn get_all_compat_by_crate(&self) -> Result<HashMap<Origin, CompatByCrateVersion>> {
        let conn = self.conn.lock();
        let mut get = conn.prepare_cached(r"SELECT rustc_version, version, compat, reason, origin FROM build_results")?;

        let mut by_crate = HashMap::with_capacity(20000);

        let mut rows = get.query([])?;
        while let Some(row) = rows.next()? {
            let compat = Self::compat_row(row)?;
            let origin = Origin::from_str(row.get_ref(4)?.as_str()?);

            let mut by_ver = by_crate.entry(origin).or_insert_with(BTreeMap::default);

            Self::append_compat(&mut by_ver, compat);
        }
        Ok(by_crate)
    }

    fn append_compat(by_ver: &mut CompatByCrateVersion, c: CompatibilityInfo) {
        by_ver.entry(c.crate_version)
            .or_insert_with(CompatRanges::default)
            .add_compat(c.rustc_version, c.compat, c.reason);
    }

    fn compat_row(row: &Row) -> Result<CompatibilityInfo> {
        let rustc_version = row.get_ref_unwrap(0).as_str()?;
        let crate_version = row.get_ref_unwrap(1).as_str()?;
        let compat = Compat::from_str(row.get_ref_unwrap(2).as_str()?);
        let reason = row.get_unwrap(3);
        Ok(CompatibilityInfo {
            rustc_version: SemVer::parse(rustc_version).map_err(|e| Error::ToSqlConversionFailure(e.into()))?.minor as RustcMinorVersion,
            crate_version: garbage_parse(crate_version),
            compat,
            reason,
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
                    if c.has_ever_built() {
                        seen_any_ok_build = true;
                    } else if c.newest_bad().is_some() {
                        old_crates_broken_up_to = Some(crate_ver);
                    }
                }
                for v in c.all_rustc_versions() {
                    rustc_versions.insert(v);
                }
            }
            // if everything is broken, we just don't know if it's a pre-1.0 problem or bad crate.
            // assume only 0.x crates are rotten. If someone released 1.x then it should have been usable!
            if !seen_any_ok_build || old_crates_broken_up_to.map_or(false, |v| v.major > 0) {
                old_crates_broken_up_to = None;
            }
            let rustc_versions = rustc_versions.into_iter().map(|rv| {
                let crate_oldest_bad = compat.iter()
                    .filter(|(crate_ver, _)| old_crates_broken_up_to.map_or(true, |b| *crate_ver > b))
                    .filter(|(_, rustc)| rustc.newest_bad().map_or(true, |v| rv <= v))
                    .map(|(crate_ver, _)| crate_ver).min();
                let crate_newest_bad = compat.iter()
                    .filter(|(_, rustc)| rustc.newest_bad().map_or(true, |v| rv <= v))
                    .map(|(crate_ver, _)| crate_ver).max();
                let crate_newest_ok = compat.iter()
                    .filter(|(crate_ver, _)| old_crates_broken_up_to.map_or(true, |b| *crate_ver > b))
                    .filter(|(_, rustc)| rustc.oldest_ok().map_or(true, |v| rv >= v))
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


    pub fn set_compat_multi(&self, rows: &[SetCompatMulti]) -> Result<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        {
            let mut get = tx.prepare_cached(r"SELECT compat FROM build_results WHERE origin = ?1 AND version = ?2 AND rustc_version = ?3")?;
            let mut insert = tx.prepare_cached(r"INSERT OR REPLACE INTO build_results(origin, version, rustc_version, compat, reason) VALUES(?1, ?2, ?3, ?4, ?5)")?;

            let mut to_insert = Vec::with_capacity(rows.len());

            for &SetCompatMulti { origin, ver, rustc_version, compat: new_compat, reason } in rows {
                if rustc_version < 18 {
                    error!("Bogus compat data: {:?}", (origin, ver, rustc_version, new_compat));
                    continue;
                }
                let ver = ver.to_string();
                let rustc_version = format!("1.{}.0", rustc_version);
                let origin_str = origin.to_str();

                let existing = get.query_row(&[origin_str.as_str(), &ver, &rustc_version], |row| {
                    Ok(Compat::from_str(row.get_ref_unwrap(0).as_str()?))
                });
                match existing {
                    Ok(existing) => {
                        if existing.is_better(&new_compat) {
                            continue;
                        }
                    },
                    Err(Error::QueryReturnedNoRows) => {},
                    Err(e) => return Err(e),
                }

                to_insert.push((origin, origin_str, ver, rustc_version, new_compat, reason));
            }

            for (origin, origin_str, ver, rustc_version, new_compat, reason) in to_insert {
                info!("https://lib.rs/compat/{}#{} R.{}={:?} ({})", origin.short_crate_name(), ver, rustc_version, new_compat, reason);
                let result_str = new_compat.as_char().to_string();
                insert.execute(&[origin_str.as_str(), &ver, &rustc_version, &result_str, reason])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn set_compat(&self, origin: &Origin, ver: &SemVer, rustc_version: RustcMinorVersion, compat: Compat, reason: &str) -> Result<()> {
        self.set_compat_multi(&[SetCompatMulti {origin, ver, rustc_version, compat, reason}])
    }
}

pub struct SetCompatMulti<'a> {
    pub origin: &'a Origin,
    pub ver: &'a SemVer,
    pub rustc_version: RustcMinorVersion,
    pub compat: Compat,
    pub reason: &'a str,
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
