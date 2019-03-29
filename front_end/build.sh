#!/bin/bash
source conf.sh
cargo build --release --bin website
time cargo run --release --bin website
