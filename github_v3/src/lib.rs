use futures::Stream;
pub use futures::StreamExt;
pub use reqwest::header::HeaderMap;
pub use reqwest::header::HeaderValue;
pub use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

pub struct Response {
    res: reqwest::Response,
    client: Arc<ClientInner>,
}

impl Response {
    pub async fn obj<T: DeserializeOwned>(self) -> Result<T, GHError> {
        Ok(self.res.json().await?)
    }

    pub fn array<T: DeserializeOwned + std::marker::Unpin + 'static>(self) -> impl Stream<Item = Result<T, GHError>> {
        let mut res = self.res;
        let client = self.client;

        // Pin is required for easy iteration, otherwise the caller would have to pin it
        Box::pin(async_stream::try_stream! {
            loop {
                let next_link = res.headers().get("link")
                    .and_then(|h| h.to_str().ok())
                    .and_then(parse_next_link);
                let items = res.json::<Vec<T>>().await?;
                for item in items {
                    yield item;
                }
                match next_link {
                    Some(url) => res = client.raw_get(&url).await?,
                    None => break,
                }
            }
        })
    }

    pub fn headers(&self) -> &HeaderMap {
        self.res.headers()
    }

    pub fn status(&self) -> StatusCode {
        self.res.status()
    }
}

pub struct Builder {
    client: Arc<ClientInner>,
    url: String,
}

impl Builder {
    pub fn path(mut self, url_part: &str) -> Self {
        debug_assert_eq!(url_part, url_part.trim_matches('/'));

        self.url.push('/');
        self.url.push_str(url_part);
        self
    }

    pub fn arg(mut self, arg: &str) -> Self {
        self.url.push('/');
        self.url.push_str(arg);
        self
    }

    pub async fn send(self) -> Result<Response, GHError> {
        let res = self.client.raw_get(&self.url).await?;
        Ok(Response {
            client: self.client,
            res
        })
    }
}

struct ClientInner {
    client: reqwest::Client,
    // FIXME: this should be per endpoint, because search and others have different throttling
    wait_sec: AtomicU32,
}

pub struct Client {
    inner: Arc<ClientInner>,
}

impl Client {
    pub fn new_from_env() -> Self {
        Self::new(std::env::var("GITHUB_TOKEN").ok().as_deref())
    }

    pub fn new(token: Option<&str>) -> Self {
        let mut default_headers = HeaderMap::with_capacity(2);
        default_headers.insert("Accept", HeaderValue::from_static("application/vnd.github.v3+json"));
        if let Some(token) = token {
            default_headers.insert("Authorization", HeaderValue::from_str(&format!("token {}", token)).unwrap());
        }

        Self {
            inner: Arc::new(ClientInner {
                client: reqwest::Client::builder()
                    .user_agent(concat!("rust-github-v3/{}", env!("CARGO_PKG_VERSION")))
                    .default_headers(default_headers)
                    .connect_timeout(Duration::from_secs(7))
                    .timeout(Duration::from_secs(30))
                    .build()
                    .unwrap(),
                wait_sec: AtomicU32::new(0),
            }),
        }
    }

    pub fn get(&self) -> Builder {
        let mut url = String::with_capacity(60);
        url.push_str("https://api.github.com");
        Builder {
            client: self.inner.clone(),
            url,
        }
    }
}

impl ClientInner {
    // Get a single response
    async fn raw_get(&self, url: &str) -> Result<reqwest::Response, GHError> {
        debug_assert!(url.starts_with("https://api.github.com/"));

        let mut retries = 5u8;
        let mut retry_delay = 1;
        loop {
            let wait_sec = self.wait_sec.load(SeqCst);
            if wait_sec > 0 {
                // This has poor behavior with concurrency. It should be pacing all requests.
                tokio::time::delay_for(Duration::from_secs(wait_sec.into())).await;
            }

            let res = self.client.get(url).send().await?;

            let headers = res.headers();
            let status = res.status();

            let wait_sec = match (Self::rate_limit_remaining(headers), Self::rate_limit_reset(headers)) {
                (Some(rl), Some(rs)) => {
                    rs.duration_since(SystemTime::now()).ok()
                        .and_then(|d| d.checked_div(rl + 2))
                        .map(|d| d.as_secs() as u32)
                        .unwrap_or(0)
                }
                _ => if status == StatusCode::TOO_MANY_REQUESTS {3} else {0},
            };
            self.wait_sec.store(wait_sec, SeqCst);

            let should_wait_for_content = status == StatusCode::ACCEPTED;
            if should_wait_for_content && retries > 0 {
                tokio::time::delay_for(Duration::from_secs(retry_delay)).await;
                retry_delay *= 2;
                retries -= 1;
                continue;
            }

            return if status.is_success() && !should_wait_for_content {
                Ok(res)
            } else {
                Err(error_for_response(res).await)
            };
        }
    }

    pub fn rate_limit_remaining(headers: &HeaderMap) -> Option<u32> {
        headers.get("x-ratelimit-remaining")
            .and_then(|s| s.to_str().ok())
            .and_then(|s| s.parse().ok())
    }

    pub fn rate_limit_reset(headers: &HeaderMap) -> Option<SystemTime> {
        headers.get("x-ratelimit-reset")
            .and_then(|s| s.to_str().ok())
            .and_then(|s| s.parse().ok())
            .map(|s| SystemTime::UNIX_EPOCH + Duration::from_secs(s))
    }
}

async fn error_for_response(res: reqwest::Response) -> GHError {
    let status = res.status();
    let mime = res.headers().get("content-type").and_then(|h| h.to_str().ok()).unwrap_or("");
    GHError::Response {
        status,
        message: if mime.starts_with("application/json") {
            res.json::<GitHubErrorResponse>().await.ok().map(|res| res.message)
        } else {
            None
        },
    }
}

fn parse_next_link(link: &str) -> Option<String> {
    for part in link.split(',') {
        if part.contains(r#"; rel="next""#) {
            if let Some(start) = link.find('<') {
                let link = &link[start + 1..];
                if let Some(end) = link.find('>') {
                    return Some(link[..end].to_owned());
                }
            }
        }
    }
    None
}

#[derive(serde_derive::Deserialize)]
struct GitHubErrorResponse {
    message: String,
}

use thiserror::Error;

#[derive(Error, Debug)]
pub enum GHError {
    #[error("Request timed out")]
    Timeout,
    #[error("Request error: {}", _0)]
    Request(String),
    #[error("{} ({})", message.as_deref().unwrap_or("HTTP error"), status)]
    Response { status: StatusCode, message: Option<String> },
    #[error("Internal error")]
    Internal,
}

impl From<reqwest::Error> for GHError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            return Self::Timeout;
        }
        if let Some(status) = e.status() {
            Self::Response {
                status,
                message: Some(e.to_string()),
            }
        } else {
            Self::Request(e.to_string())
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn req_test() {
        let gh = Client::new_from_env();
        gh.get().path("users/octocat/orgs").send().await.unwrap();
    }
}
