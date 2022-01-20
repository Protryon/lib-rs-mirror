use std::time::Duration;
use tokio::time::error::Elapsed;
use tokio::time::timeout;

#[derive(Debug)]
pub struct Fetcher {
    client: reqwest::Client,
    sem: tokio::sync::Semaphore,
    sem_timeout: u16,
}

use quick_error::quick_error;

quick_error! {
    #[derive(Debug)]
    pub enum Error {
        Req(err: reqwest::Error) {
            display("{}", err)
            from()
        }
        Timeout {
            display("Request timed out")
            from(Elapsed)
        }
    }
}

impl Fetcher {
    pub fn new(max_concurrent: u16) -> Self {
        let client = reqwest::Client::builder().build().unwrap();
        Self {
            client,
            sem_timeout: (max_concurrent + 3).max(5),
            sem: tokio::sync::Semaphore::new(max_concurrent.into()),
        }
    }

    pub async fn fetch(&self, url: &str) -> Result<Vec<u8>, Error> {
        let _s = match self.sem.try_acquire() {
            Ok(s) => {
                log::info!("REQ {}", url);
                s
            },
            Err(_) => {
                log::info!("REQ (waiting up to {}s) {}", self.sem_timeout, url);
                let s = timeout(Duration::from_secs(self.sem_timeout.into()), self.sem.acquire()).await?.expect("reqsem");
                log::debug!("REQ now starts {}", url);
                s
            },
        };

        let res = timeout(Duration::from_secs(21), self.client.get(url)
            .header(reqwest::header::USER_AGENT, "lib.rs/1.1")
            .send()).await??
            .error_for_status()?;
        Ok(timeout(Duration::from_secs(61), res.bytes()).await??.to_vec())
    }
}
