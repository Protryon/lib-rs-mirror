
#[derive(Debug)]
pub struct Fetcher {
    sem: tokio::sync::Semaphore,
}

pub type Error = reqwest::Error;

impl Fetcher {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            sem: tokio::sync::Semaphore::new(max_concurrent),
        }
    }

    pub async fn fetch(&self, url: &str) -> Result<Vec<u8>, Error> {
        log::info!("REQ {}", url);

        let _s = self.sem.acquire().await;
        let client = reqwest::Client::builder().build()?;
        let res = client.get(url)
            .header(reqwest::header::USER_AGENT, "lib.rs/1.1")
            .send().await?
            .error_for_status()?;
        Ok(res.bytes().await?.to_vec())
    }
}
