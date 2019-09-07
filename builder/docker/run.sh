#!/bin/bash
set -exuo pipefail

for rustv in 1.34.2 1.24.1
do
for crate in "$@"
do
    echo > Cargo.toml "
[package]
name = "\""______"\""
version = "\""0.0.0"\""

[lib]
path = "\""/dev/null"\""

[dependencies]
$crate
"

cargo +nightly generate-lockfile -Z avoid-dev-deps || continue; # just a deps issue
cargo +nightly fetch --locked -Z avoid-dev-deps -Z no-index-update || continue; # network prob?

echo "----SNIP----"; echo >&2 "----SNIP----";

rustup default $rustv
rustc --version
time cargo check --locked --message-format=json || continue;

done
done
