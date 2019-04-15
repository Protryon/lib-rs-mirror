# Lib.rs: user database

Maintains an sqlite index of author emails, names and their GitHub usernames.

The database is created by `reindex_users` binary which is **not** in this crate, but in the [reindex](https://gitlab.com/crates.rs/reindex) sub project (that's to avoid circular crate references).

Mapping uses both GitHub search as well as commit listing APIs.
