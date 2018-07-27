extern crate github_rs;
extern crate file;
extern crate serde;
extern crate urlencoding;
#[macro_use] extern crate serde_derive;
extern crate serde_json;
extern crate repo_url;
extern crate simple_cache;
#[macro_use] extern crate failure;

pub type CResult<T> = std::result::Result<T, failure::Error>;
use std::path::Path;

use urlencoding::encode;
use repo_url::SimpleRepo;
use github_rs::client;
use github_rs::{Headers, StatusCode};
use github_rs::client::Executor;
use failure::SyncFailure;
use std::time::{SystemTime, UNIX_EPOCH};
use std::time::Duration;
use std::thread;
use simple_cache::SimpleCache;
use github_rs::headers::{rate_limit_remaining, rate_limit_reset};

mod model;
pub use model::*;

#[derive(Debug, Fail)]
enum GitApiError {
    #[fail(display = "response w/o body")]
    NoBody,
}

pub struct GitHub {
    token: String,
    cache: SimpleCache,
}

impl GitHub {
    pub fn new(cache_path: impl AsRef<Path>, token: impl Into<String>) -> CResult<Self> {
        Ok(Self {
            token: token.into(),
            cache: SimpleCache::new(cache_path.as_ref())?,
        })
    }

    fn client(&self) -> CResult<client::Github> {
        Ok(client::Github::new(&self.token).map_err(SyncFailure::new)?)
    }

    pub fn user_by_email(&self, email: &str) -> CResult<Option<User>> {
        let std_suffix = "@users.noreply.github.com";
        if email.ends_with(std_suffix) {
            let login = email[0..email.len()-std_suffix.len()].split('+').last().unwrap();
            if let Ok(user) = self.user_by_login(login) {
                return Ok(Some(user));
            }
        }
        let enc_email = encode(email);
        let cache_file = format!("user-{}.json", enc_email);
        let res: SearchResults<User> = self.get_cached(&cache_file, |client| client.get()
                       .custom_endpoint(&format!("search/users?q=in:email%20{}", enc_email))
                       .execute())?;
        Ok(res.items.into_iter().next())
    }

    pub fn user_by_login(&self, login: &str) -> CResult<User> {
        let enc_login = encode(&login.to_lowercase());
        let cache_file = format!("user-{}.json", enc_login);
        Ok(self.get_cached(&cache_file, |client| client.get()
                       .users().username(login)
                       .execute())?)
    }

    pub fn commits(&self, repo: &SimpleRepo) -> CResult<Vec<CommitMeta>> {
        let cache_file = format!("{}-{}-commit.json", repo.owner, repo.repo);
        self.get_cached(&cache_file, |client| client.get()
                           .repos().owner(&repo.owner).repo(&repo.repo)
                           .commits()
                           .execute())
    }

    pub fn contributors(&self, repo: &SimpleRepo) -> CResult<Vec<UserContrib>> {
        let cache_file = format!("{}-{}-contrib.json", repo.owner, repo.repo);
        self.get_cached(&cache_file, |client| client.get()
                           .custom_endpoint(&format!("repos/{}/{}/stats/contributors", repo.owner, repo.repo))
                           .execute())
    }

    fn get_cached<F, B>(&self, key: &str, cb: F) -> CResult<B>
        where B: for<'de> serde::Deserialize<'de>,
        F: FnOnce(&client::Github) -> Result<(Headers, StatusCode, Option<serde_json::Value>), github_rs::errors::Error>
    {
        if let Ok(cached) = self.cache.get(key) {
            Ok(serde_json::from_slice(&cached)?)
        } else {
            let client = &self.client()?;
            eprintln!("Cache miss {}", key);
            let (headers, status, body) = cb(&*client).map_err(SyncFailure::new)?;
            eprintln!("Recvd {} {:?} {:?}", key, status, headers);
            if let (Some(rl), Some(rs)) = (rate_limit_remaining(&headers), rate_limit_reset(&headers)) {
                let end_timestamp = Duration::from_secs(rs.into());
                let now = SystemTime::now().duration_since(UNIX_EPOCH)?;
                let wait = (end_timestamp.checked_sub(now)).and_then(|d| d.checked_div(rl + 1));
                if let Some(wait) = wait {
                    if wait.as_secs() > 0 {
                        eprintln!("need to wait! {:?}", wait);
                        thread::sleep(wait);
                    }
                }
            }
            let stats = body.ok_or(GitApiError::NoBody)?;
            let allow = match status {
                StatusCode::Accepted |
                StatusCode::Created => false,
                StatusCode::NotFound |
                StatusCode::Gone |
                StatusCode::MovedPermanently => true,
                _ => status.is_success(),
            };

            if allow {
                self.cache.set(key, stats.to_string().as_bytes())?;
            }
            Ok(serde_json::from_value(stats)?)
        }
    }
}

#[test]
fn github_contrib() {
    let gh = GitHub::new(
        "../data/github.db",
        std::env::var("GITHUB_TOKEN").expect("GITHUB_TOKEN env var")).unwrap();
    let repo = SimpleRepo{
        owner:"visionmedia".into(),
        repo:"superagent".into(),
    };
    gh.contributors(&repo).unwrap();
    gh.commits(&repo).unwrap();
}

#[test]
fn test_user_by_email() {
    let gh = GitHub::new(
        "../data/github.db",
        std::env::var("GITHUB_TOKEN").expect("GITHUB_TOKEN env var")).unwrap();
    let user = gh.user_by_email("github@pornel.net").unwrap().unwrap();
    assert_eq!("kornelski", user.login);
}
