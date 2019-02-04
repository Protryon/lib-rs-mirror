# Untarball for `.crate` files

Decompresses `.crate` files (packages from crates.io) and extracts useful files and metadata from them.

crates.rs keeps all crate files compressed, in [crates.db](https://gitlab.com/crates.rs/crate_db) archive, and extracts them on the fly using this code.
