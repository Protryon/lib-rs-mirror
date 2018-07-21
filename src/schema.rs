use rusqlite::*;
use std::path::Path;
use CrateDb;

impl CrateDb {
    pub(crate) fn db(path: &Path) -> Result<Connection> {
        let conn = Connection::open(path)?;
        conn.execute_batch(r#"
            BEGIN;
            CREATE TABLE IF NOT EXISTS crates (
                id              INTEGER PRIMARY KEY,
                origin          TEXT NOT NULL UNIQUE,
                recent_downloads INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS keywords (
                id              INTEGER PRIMARY KEY,
                keyword         TEXT NOT NULL UNIQUE,
                visible         INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS crate_keywords (
                crate_id        INTEGER NOT NULL,
                keyword_id      INTEGER NOT NULL,
                weight          REAL NOT NULL,
                explicit        INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS crate_versions (
                crate_id        INTEGER NOT NULL,
                version         TEXT NOT NULL,
                created         INTEGER NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS crate_versions_idx on crate_versions(crate_id, version);

            CREATE UNIQUE INDEX IF NOT EXISTS keywords_idx on crate_keywords(keyword_id, crate_id);
            CREATE INDEX IF NOT EXISTS keywords_ridx on crate_keywords(crate_id);
            CREATE TABLE IF NOT EXISTS categories (
                crate_id        INTEGER NOT NULL,
                slug            TEXT NOT NULL,
                weight          REAL NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS categories_idx on categories(crate_id, slug);
            COMMIT;"#)?;
        Ok(conn)
    }
}
