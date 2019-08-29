#!/bin/bash
set -exuo pipefail

echo "----SNIP----"; echo >&2 "----SNIP----";

rustup show
time cargo check --locked --message-format=json

echo "----SNIP----"; echo >&2 "----SNIP----";

rustup default 1.34.2
rustc --version
time cargo check --locked --message-format=json

echo "----SNIP----"; echo >&2 "----SNIP----";

rustup default 1.24.1
rustc --version
time cargo check --locked --message-format=json

echo "----SNIP----"; echo >&2 "----SNIP----";

cargo +stable clippy
