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

/// Response from the API
pub struct Response {
    res: reqwest::Response,
    client: Arc<ClientInner>,
}

impl Response {
    /// Fetch a single JSON object from the API
    pub async fn obj<T: DeserializeOwned>(self) -> Result<T, GHError> {
        Ok(self.res.json().await?)
    }

    /// Stream an array of objects from the API
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

    /// Response headers
    pub fn headers(&self) -> &HeaderMap {
        self.res.headers()
    }

    /// Response status
    pub fn status(&self) -> StatusCode {
        self.res.status()
    }
}

/// See `Client::get()`
///
/// Make a new request by constructing the request URL bit by bit
pub struct Builder {
    client: Arc<ClientInner>,
    url: String,
    query_string_started: bool,
}

impl Builder {
    /// Add a constant path to the request, e.g. `.path("users")`
    ///
    /// Inner slashes are OK, but the string must not start or end with a slash.
    ///
    /// Panics if query string has been added.
    ///
    /// It's appended raw, so must be URL-safe.
    pub fn path(mut self, url_part: &'static str) -> Self {
        debug_assert_eq!(url_part, url_part.trim_matches('/'));
        assert!(!self.query_string_started);

        self.url.push('/');
        self.url.push_str(url_part);
        self
    }

    /// Add a user-supplied argument to the request path, e.g. `.path("users").arg(username)`,
    /// or after a call to query(), starts adding fragments to the query string with no delimiters.
    ///
    /// The arg is URL-escaped, so it's safe to use any user-supplied data.
    pub fn arg(mut self, arg: &str) -> Self {
        if !self.query_string_started {
            self.url.push('/');
        }
        self.url.push_str(&urlencoding::encode(arg));
        self
    }

    /// Add a raw unescaped query string. The string must *not* start with `?`
    ///
    /// ```rust
    /// # Client::new(None)
    /// .get().path("search/users").query("q=").arg(somestring)
    /// ```
    pub fn query(mut self, query_string: &str) -> Self {
        debug_assert!(!query_string.starts_with('?'));
        debug_assert!(!query_string.starts_with('&'));
        self.url.push(if self.query_string_started {'&'} else {'?'});
        self.url.push_str(query_string);
        self.query_string_started = true;
        self
    }

    /// Make the request
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

/// API Client. Start here.
pub struct Client {
    inner: Arc<ClientInner>,
}

impl Client {
    /// Reads `GITHUB_TOKEN` env var.
    pub fn new_from_env() -> Self {
        Self::new(std::env::var("GITHUB_TOKEN").ok().as_deref())
    }

    /// Takes API token for authenticated requests (make the token in GitHub settings)
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

    /// Make a new request to the API.
    pub fn get(&self) -> Builder {
        let mut url = String::with_capacity(100);
        url.push_str("https://api.github.com");
        Builder {
            client: self.inner.clone(),
            url,
            query_string_started: false,
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

    /// GitHub's `x-ratelimit-remaining` header
    pub fn rate_limit_remaining(headers: &HeaderMap) -> Option<u32> {
        headers.get("x-ratelimit-remaining")
            .and_then(|s| s.to_str().ok())
            .and_then(|s| s.parse().ok())
    }

    /// GitHub's `x-ratelimit-reset` header
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
            if let Some(start) = part.find('<') {
                let next_link = &part[start + 1..];
                if let Some(end) = next_link.find('>') {
                    return Some(next_link[..end].to_owned());
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

    #[test]
    fn parse_next_link_test() {
        let example = "\"<https://api.github.com/organizations/fakeid/repos?page=1>; rel=\"prev\", <https://api.github.com/organizations/fakeid/repos?page=3>; rel=\"next\", <https://api.github.com/organizations/fakeid/repos?page=38>; rel=\"last\", <https://api.github.com/organizations/fakeid/repos?page=1>; rel=\"first\"";

        let expected = Some(String::from("https://api.github.com/organizations/fakeid/repos?page=3"));
        assert_eq!(parse_next_link(example), expected)
    }
}
