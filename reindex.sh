#!/bin/bash
source ../front_end/conf.sh;
cargo build --release --bin reindex_crates
cargo run --release --bin reindex_crates &
( cd ../front_end; cargo build --release --bin website )
