#!/bin/bash
source conf.sh
cargo build --release --bin website &
( cd ../reindex; nice cargo build --release --bin reindex_search )
time cargo run --release --bin website
wait
