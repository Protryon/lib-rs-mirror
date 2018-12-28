#!/bin/bash
source conf.sh
cargo build --release --bin website
cargo run --release --bin website &
( cd ../reindex; nice cargo build --release --bin reindex_search )
wait
