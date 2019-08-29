use kitchen_sink::Origin;

use parking_lot::Mutex;
use rusqlite::*;
use std::path::Path;

pub struct BuildDb {
    pub(crate) conn: Mutex<Connection>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Compat {
    VerifiedWorks,
    ProbablyWorks,
    BrokenDeps,
    Incompatible,
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

    pub fn get_compat(&self, origin: &Origin, ver: &str) -> Result<Vec<(String, Compat)>> {
        let conn = self.conn.lock();
        let mut get = conn.prepare_cached(r"SELECT rustc_version, compat FROM build_results WHERE origin = ?1 AND version = ?2")?;
        let origin_str = origin.to_str();
        let res = get.query_map(&[origin_str.as_str(), ver], |row| {
            let compat = match row.get_raw(1).as_str().expect("strtype") {
                "Y" => Compat::VerifiedWorks,
                "y" => Compat::ProbablyWorks,
                "n" => Compat::BrokenDeps,
                "N" => Compat::Incompatible,
                _ => panic!("wat?"),
            };
            Ok((row.get(0)?, compat))
        })?;
        res.collect()
    }

    pub fn set_compat(&self, origin: &Origin, ver: &str, rustc_version: &str, compat: Compat) -> Result<()> {
        let conn = self.conn.lock();
        let mut ins = conn.prepare_cached(r"INSERT OR REPLACE INTO build_results(origin, version, rustc_version, compat) VALUES(?1, ?2, ?3, ?4)")?;
        let origin_str = origin.to_str();
        let result_str = match compat {
            Compat::VerifiedWorks => "Y",
            Compat::ProbablyWorks => "y",
            Compat::BrokenDeps => "n",
            Compat::Incompatible => "N",
        };
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
