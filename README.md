# Basic caching for crates.rs

Crates.rs saves all successful responses from APIs to quickly (re)build the whole website, and to avoid overloading the APIs.

It's a simple key-value storage with no expiration. Most keys contain crate version, so every new crate release gets fresh data.

The initial cache files are available at [crates.rs/data](https://crates.rs/data). See *reindex* project for details.
