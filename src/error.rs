#![allow(renamed_and_removed_lints)]
use reqwest;
use rusqlite;
use serde_json;
use std::io;
use rmp_serde;

quick_error! {
    #[derive(Debug)]
    pub enum Error {
        Io(err: io::Error) {
            from()
            display("Simple cache error: {}", err)
            cause(err)
        }
        Net(err: reqwest::Error) {
            from()
            display("{}", err)
            cause(err)
        }
        Db(err: rusqlite::Error) {
            from()
            display("Simple cache db: {}", err)
            cause(err)
        }
        RmpEnc(err:  rmp_serde::encode::Error) {
            from()
            display("KV cache enc: {}", err)
            cause(err)
        }
        RmpDec(err:  rmp_serde::decode::Error) {
            from()
            display("KV cache dec: {}", err)
            cause(err)
        }
        KvPoison {}
        Status(code: reqwest::StatusCode) {
            from()
            display("Request error {}", code)
        }
        Parse(err: serde_json::Error, data: Vec<u8>) {
            display("{}\n{}", err, String::from_utf8_lossy(data))
            cause(err)
        }
        Other(err: String) {
            display("{}", err)
        }
    }
}
