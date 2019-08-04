use crate::CrateDb;
use rusqlite::*;

impl CrateDb {
    pub(crate) fn db(url: &str) -> Result<Connection> {
        let conn = Connection::open(url)?;
        conn.execute_batch(r#"
            BEGIN;
            CREATE TABLE IF NOT EXISTS crates (
                id              INTEGER PRIMARY KEY,
                origin          TEXT NOT NULL UNIQUE,
                ranking         REAL,
                next_update     INTEGER,
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

            CREATE TABLE IF NOT EXISTS crate_repos (
                crate_id        INTEGER NOT NULL UNIQUE,
                repo            TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS repo_crates (
                repo            TEXT NOT NULL,
                path            TEXT NOT NULL,
                crate_name      TEXT NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS repo_crates_idx on repo_crates(repo, path, crate_name);

            CREATE TABLE IF NOT EXISTS repo_changes (
                repo            TEXT NOT NULL,
                crate_name      TEXT NOT NULL,
                replacement     NULL,
                weight          REAL NOT NULL DEFAULT 1.0
            );
            CREATE UNIQUE INDEX IF NOT EXISTS repo_changes_idx on repo_changes(repo, crate_name, replacement);
            CREATE INDEX IF NOT EXISTS repo_changes_idx2 on repo_changes(crate_name);
            CREATE INDEX IF NOT EXISTS repo_changes_idx3 on repo_changes(replacement);

            CREATE TABLE IF NOT EXISTS crate_versions (
                crate_id        INTEGER NOT NULL,
                version         TEXT NOT NULL,
                created         INTEGER NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS crate_versions_idx on crate_versions(crate_id, version);

            CREATE TABLE IF NOT EXISTS crate_derived (
                crate_id        INTEGER NOT NULL UNIQUE,
                readme          TEXT,
                readme_format   TEXT,
                readme_base_url TEXT,
                readme_base_image_url TEXT,
                crate_compressed_size INTEGER NOT NULL,
                crate_decompressed_size INTEGER NOT NULL,
                github_keywords TEXT,
                capitalized_name TEXT NOT NULL,
                lib_file        TEXT,
                has_buildrs     INTEGER, -- bool
                is_nightly      INTEGER, -- bool
                is_yanked       INTEGER, -- bool
                has_code_of_conduct       INTEGER, -- bool
                manifest        BLOB NOT NULL,
                language_stats  BLOB NOT NULL
            );

            CREATE UNIQUE INDEX IF NOT EXISTS keywords_idx on crate_keywords(keyword_id, crate_id);
            CREATE INDEX IF NOT EXISTS keywords_ridx on crate_keywords(crate_id);
            CREATE TABLE IF NOT EXISTS categories (
                crate_id        INTEGER NOT NULL,
                slug            TEXT NOT NULL,
                rank_weight     REAL NOT NULL,
                relevance_weight REAL NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS categories_idx on categories(crate_id, slug);
            COMMIT;"#)?;
        Ok(conn)
    }
}
