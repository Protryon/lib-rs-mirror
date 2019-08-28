use kitchen_sink::Origin;

use parking_lot::Mutex;
use rusqlite::*;
use std::path::Path;

pub struct BuildDb {
    pub(crate) conn: Mutex<Connection>,
}

impl BuildDb {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let db = Connection::open(path.as_ref())?;
        db.execute_batch("
            CREATE TABLE IF NOT EXISTS raw_builds (
                origin TEXT NOT NULL,
                version TEXT NOT NULL,
                stdout TEXT NOT NULL,
                stderr TEXT NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS builds_ver on raw_builds(origin, version);
            ")?;
        Ok(Self {
            conn: Mutex::new(db),
        })
    }

    pub fn set_raw_build_info(&self, origin: &Origin, ver: &str, stdout: &str, stderr: &str) -> Result<()> {
        let conn = self.conn.lock();
        let mut ins = conn.prepare_cached(r"INSERT INTO raw_builds(origin, version, stdout, stderr) VALUES(?1, ?2, ?3, ?4)")?;
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
