# Reindexing helper databases for crates.rs

Requires `cache.db`, `crates.db` and `github.db` data files.


## Usage

To run:

0. [Install Rust](https://www.rust-lang.org/install.html)
1. [Obtain data files](https://crates.rs/data)
2. Set environmental variables (`export` on Unix, `set` on Windows)
  * `GITHUB_TOKEN` with [GitHub token](https://blog.github.com/2013-05-16-personal-api-tokens/) from [here](https://github.com/settings/tokens). It doesn't need any additional permissions.
  * `CRATES_DATA_DIR` data with path to the directory that contains the data files.
3. `cargo run --bin reindex_crates`
4. `cargo run --bin reindex_users`

It should take about 5 minutes to run.

```sh
# examples - set your own values
export GITHUB_TOKEN=abc123abc123
export CRATES_DATA_DIR=/home/exampl/crates.rs/
```

## Troubleshooting

- `GitHubTokenEnvVarMissing` error: `GITHUB_TOKEN` was not set. Make sure you've got the syntax correct.
- `CratesDataDirEnvVarMissing` error: `CRATES_DATA_DIR` was not set.
- Makes lost of network requests and takes hours to run: the data files weren't downloaded completely, or `CRATES_DATA_DIR` points to a wrong directory.
- SQL error that `cache` table is missing: the data files weren't downloaded completely, or `CRATES_DATA_DIR` points to a wrong directory.
- error: no bin target named …: you must `cd` to the `reindex` subproject directory. If you don't have this directory, you may have forgotten the `--recursive` flag when cloning the [crates.rs repository](https://gitlab.com/crates.rs/crates.rs).
