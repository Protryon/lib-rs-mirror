#![allow(renamed_and_removed_lints)]

use std::io;
use reqwest;
use serde_json;
use rusqlite;

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
    }
}
