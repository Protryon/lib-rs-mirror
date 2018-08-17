#![allow(renamed_and_removed_lints)]

use reqwest;
use rusqlite;
use serde_json;
use std::io;

quick_error! {
    #[derive(Debug)]
    pub enum Error {
        Io(err: io::Error) {
            from()
            display("Simple cache error: {}", err)
        }
        Net(err: reqwest::Error) {
            from()
            display("{}", err)
        }
        Db(err: rusqlite::Error) {
            from()
            display("Simple cache db: {}", err)
        }
        Status(code: reqwest::StatusCode) {
            from()
            display("Request error {}", code)
        }
        Parse(err: serde_json::Error, data: Vec<u8>) {
            display("{}\n{}", err, String::from_utf8_lossy(data))
        }
        NotCached {}
        Other(err: String) {
            display("{}", err)
        }
    }
}
