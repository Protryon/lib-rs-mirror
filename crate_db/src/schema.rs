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
            CREATE TABLE IF NOT EXISTS author_crates (
                github_id       INTEGER NOT NULL,
                crate_id        INTEGER NOT NULL,
                invited_by_github_id INTEGER,
                invited_at      TEXT
            );
            CREATE UNIQUE INDEX IF NOT EXISTS author_crates_idx ON author_crates(github_id, crate_id);
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
                crate_name      TEXT NOT NULL,
                revision        TEXT -- when it's been indexed
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

            CREATE UNIQUE INDEX IF NOT EXISTS keywords_idx on crate_keywords(keyword_id, crate_id);
            CREATE INDEX IF NOT EXISTS keywords_ridx on crate_keywords(crate_id);
            CREATE TABLE IF NOT EXISTS categories (
                crate_id        INTEGER NOT NULL,
                slug            TEXT NOT NULL,
                rank_weight     REAL NOT NULL,
                relevance_weight REAL NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS categories_idx on categories(crate_id, slug);
            CREATE INDEX IF NOT EXISTS categories_slug_idx on categories(slug);
            COMMIT;"#)?;
        conn.execute_batch("
            PRAGMA cache_size = 500000;
            PRAGMA threads = 4;")?;
        Ok(conn)
    }
}
