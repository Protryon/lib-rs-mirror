#!/bin/bash
set -exuo pipefail

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

cargo fetch

echo "----SNIP----"; echo >&2 "----SNIP----";

rustup default 1.34.2
rustc --version
time cargo check --locked --message-format=json

echo "----SNIP----"; echo >&2 "----SNIP----";

rustup default 1.24.1
rustc --version
time cargo check --locked --message-format=json

echo "----SNIP----"; echo >&2 "----SNIP----";

done
