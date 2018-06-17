use rusqlite::*;
use std::path::Path;
use UserDb;

impl UserDb {
    pub(crate) fn db(path: &Path) -> Result<Connection> {
        let conn = Connection::open(path)?;
        conn.execute_batch(r#"
            BEGIN;
            CREATE TABLE IF NOT EXISTS github_users (
                id            INTEGER NOT NULL PRIMARY KEY,
                login         TEXT NOT NULL,
                name          TEXT,
                avatar_url    TEXT,
                gravatar_id   TEXT,
                html_url      TEXT,
                type          TEXT NOT NULL DEFAULT 'user'
            );
            CREATE INDEX IF NOT EXISTS github_users_idx on github_users(login); -- not unique, logins change!

            CREATE TABLE IF NOT EXISTS github_emails (
                github_id     INTEGER NOT NULL,
                email         TEXT NOT NULL,
                name          TEXT
            );
            CREATE UNIQUE INDEX IF NOT EXISTS github_emails_idx on github_emails(github_id, email);
            COMMIT;"#)?;
        Ok(conn)
    }
}
