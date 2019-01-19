# Wrapper for crates.rs crate data

It's a struct that combines all data sources used for crates.rs crates. It's a combination of data from crates.io, GitHub and crates.rs' own database.

Creation of this struct requires a lot of input sources, so it's done by the [kitchen_sink](https://gitlab.com/crates.rs/kitchen_sink) library that links them all.
