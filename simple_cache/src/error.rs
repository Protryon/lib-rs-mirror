use std::sync::Arc;
use std::io;

quick_error! {
    #[derive(Debug)]
    pub enum Error {
        Io(err: io::Error) {
            from()
            display("Simple cache error: {}", err)
            source(err)
        }
        Net(err: fetcher::Error) {
            from()
            display("{}", err)
            source(err)
        }
        Db(url: String, err: Arc<rusqlite::Error>) {
            display("Simple cache db @{}: {}", url, err)
            source(err)
        }
        RmpEnc(err:  rmp_serde::encode::Error) {
            from()
            display("KV cache enc: {}", err)
            source(err)
        }
        RmpDec(err: rmp_serde::decode::Error) {
            from()
            source(err)
        }
        Timeout {}
        NotInCache {}
        Parse(err: serde_json::Error, data: Vec<u8>) {
            display("{}\n{}", err, String::from_utf8_lossy(data))
            source(err)
        }
        Other(err: String) {
            display("{} (other cache err)", err)
        }
    }
}
