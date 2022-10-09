use crate::UserDb;
use rusqlite::*;
use std::path::Path;


impl UserDb {
    pub(crate) fn db(path: &Path) -> Result<Connection> {
        let conn = Connection::open(path)?;
        conn.execute_batch(r#"
            BEGIN;
            CREATE TABLE IF NOT EXISTS github_users (
                id            INTEGER NOT NULL,
                login         TEXT NOT NULL, -- lowercase
                login_case    TEXT, -- case-preserving
                name          TEXT,
                avatar_url    TEXT,
                gravatar_id   TEXT,
                html_url      TEXT,
                type          TEXT NOT NULL DEFAULT 'user',
                two_factor_authentication INTEGER,
                blog       TEXT,
                created_at TEXT,
                fetched_timestamp INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY(id, login)
            );
            DROP INDEX IF EXISTS "github_users_idx";
            DROP INDEX IF EXISTS "github_users_idx2";

            CREATE TABLE IF NOT EXISTS github_emails (
                github_id     INTEGER NOT NULL,
                email         TEXT NOT NULL,
                name          TEXT
            );
            CREATE UNIQUE INDEX IF NOT EXISTS github_emails_idx on github_emails(github_id, email);
            CREATE INDEX IF NOT EXISTS github_login_idx on github_users(login);
            COMMIT;"#)?;
        Ok(conn)
    }
}

