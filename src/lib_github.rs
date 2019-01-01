use github_rs;
use hyper::header::{HeaderValue, ACCEPT};

use serde;

#[macro_use] extern crate serde_derive;
use serde_json;

use simple_cache;
#[macro_use] extern crate quick_error;

use std::path::Path;

use urlencoding::encode;
use repo_url::SimpleRepo;
use github_rs::client;
use github_rs::{HeaderMap, StatusCode};
use github_rs::client::Executor;
use std::time::{SystemTime, UNIX_EPOCH};
use std::time::Duration;
use std::thread;
use simple_cache::TempCache;
use github_rs::headers::{rate_limit_remaining, rate_limit_reset};

mod model;
pub use crate::model::*;

pub type CResult<T> = Result<T, Error>;

quick_error! {
    #[derive(Debug)]
    pub enum Error {
        NoBody {
            display("Reponse with no body")
        }
        TryAgainLater {
            display("Accepted, but no data available yet")
        }
        Cache(err: Box<simple_cache::Error>) {
            display("GH can't decode cache: {}", err)
            from(e: simple_cache::Error) -> (Box::new(e))
            cause(err)
        }
        GitHub(err: String) {
            display("{}", err)
            from(e: github_rs::errors::Error) -> (e.to_string()) // non-Sync
        }
        Json(err: Box<serde_json::Error>, call: Option<&'static str>) {
            display("JSON decode error {} in {}", err, call.unwrap_or("github_info"))
            from(e: serde_json::Error) -> (Box::new(e), None)
            cause(err)
        }
        Time(err: std::time::SystemTimeError) {
            display("{}", err)
            from()
            cause(err)
        }
    }
}

impl Error {
    pub fn context(self, ctx: &'static str) -> Self {
        match self {
            Error::Json(e, _) => Error::Json(e, Some(ctx)),
            as_is => as_is,
        }
    }
}

pub struct GitHub {
    token: String,
    cache: TempCache<(String, Payload)>,
    orgs: TempCache<(String, Option<Vec<UserOrg>>)>,
    users: TempCache<(String, Option<User>)>,
    commits: TempCache<(String, Option<Vec<CommitMeta>>)>,
    releases: TempCache<(String, Option<Vec<GitHubRelease>>)>,
    contribs: TempCache<(String, Option<Vec<UserContrib>>)>,
    topics: TempCache<(String, Option<Vec<String>>)>,
}

impl GitHub {
    pub fn new(cache_path: impl AsRef<Path>, token: impl Into<String>) -> CResult<Self> {
        Ok(Self {
            token: token.into(),
            cache: TempCache::new(&cache_path.as_ref())?,
            orgs: TempCache::new(&cache_path.as_ref().with_file_name("github_orgs.bin"))?,
            users: TempCache::new(&cache_path.as_ref().with_file_name("github_users.bin"))?,
            commits: TempCache::new(&cache_path.as_ref().with_file_name("github_commits.bin"))?,
            releases: TempCache::new(&cache_path.as_ref().with_file_name("github_releases.bin"))?,
            contribs: TempCache::new(&cache_path.as_ref().with_file_name("github_contribs.bin"))?,
            topics: TempCache::new(&cache_path.as_ref().with_file_name("github_topics.bin"))?,
        })
    }

    fn client(&self) -> CResult<client::Github> {
        Ok(client::Github::new(&self.token)?)
    }

    pub fn user_by_email(&self, email: &str) -> CResult<Option<User>> {
        let std_suffix = "@users.noreply.github.com";
        if email.ends_with(std_suffix) {
            let login = email[0..email.len() - std_suffix.len()].split('+').last().unwrap();
            if let Some(user) = self.user_by_login(login)? {
                return Ok(Some(user));
            }
        }
        let key = format!("search/{}", email);
        let enc_email = encode(email);
        let res: Option<SearchResults<User>> = self.get_cached_old(&self.cache, (&key, ""), |client| client.get()
                       .custom_endpoint(&format!("search/users?q=in:email%20{}", enc_email))
                       .execute())?;
        Ok(res.and_then(|res| res.items.into_iter().next()))
    }

    pub fn user_by_login(&self, login: &str) -> CResult<Option<User>> {
        let enc_login = encode(&login.to_lowercase());
        let key = format!("user/{}", enc_login);
        self.get_cached(&self.users, (&key, ""), |client| client.get()
                       .users().username(login)
                       .execute(), id).map_err(|e| e.context("user_by_login"))
    }

    pub fn user_orgs(&self, login: &str) -> CResult<Option<Vec<UserOrg>>> {
        let login = login.to_lowercase();
        let key = format!("user/{}", login);
        self.get_cached(&self.orgs, (&key, ""), |client| client.get()
                       .users().username(&login).orgs()
                       .execute(), id).map_err(|e| e.context("user_orgs"))
    }

    pub fn commits(&self, repo: &SimpleRepo, as_of_version: &str) -> CResult<Option<Vec<CommitMeta>>> {
        let key = format!("commits/{}/{}", repo.owner, repo.repo);
        self.get_cached(&self.commits, (&key, as_of_version), |client| client.get()
                           .repos().owner(&repo.owner).repo(&repo.repo)
                           .commits()
                           .execute(), id).map_err(|e| e.context("commits"))
    }

    pub fn releases(&self, repo: &SimpleRepo, as_of_version: &str) -> CResult<Option<Vec<GitHubRelease>>> {
        let key = format!("release/{}/{}", repo.owner, repo.repo);
        let path = format!("repos/{}/{}/releases", repo.owner, repo.repo);
        self.get_cached(&self.releases, (&key, as_of_version), |client| client.get()
                           .custom_endpoint(&path)
                           .execute(), id).map_err(|e| e.context("releases"))
    }

    pub fn topics(&self, repo: &SimpleRepo, as_of_version: &str) -> CResult<Option<Vec<String>>> {
        let key = format!("{}/{}", repo.owner, repo.repo);
        let path = format!("repos/{}/{}/topics", repo.owner, repo.repo);
        self.get_cached(&self.topics, (&key, as_of_version), |client| client.get()
                           .custom_endpoint(&path)
                           .set_header(ACCEPT, HeaderValue::from_static("application/vnd.github.mercy-preview+json"))
                           .execute(), |t: Topics| t.names).map_err(|e| e.context("topics"))
    }

    pub fn repo(&self, repo: &SimpleRepo, as_of_version: &str) -> CResult<Option<GitHubRepo>> {
        let key = format!("{}/{}/repo", repo.owner, repo.repo);
        let mut ghdata: GitHubRepo = match self.get_cached_old(&self.cache, (&key, as_of_version), |client| client.get()
                           .repos().owner(&repo.owner).repo(&repo.repo)
                           .execute()).map_err(|e| e.context("repo"))? {
            Some(data) => data,
            None => return Ok(None),
        };

        // Keep GH-specific logic in here
        if ghdata.has_pages {
            // Name is case-sensitive
            ghdata.github_page_url = Some(format!("https://{}.github.io/{}/", repo.owner, ghdata.name));
        }
        // Some homepages are empty strings
        if ghdata.homepage.as_ref().map_or(false, |h| !h.starts_with("http")) {
            ghdata.homepage = None;
        }
        if !ghdata.has_issues {
            ghdata.open_issues_count = None;
        }
        Ok(Some(ghdata))
    }

    pub fn contributors(&self, repo: &SimpleRepo, as_of_version: &str) -> CResult<Option<Vec<UserContrib>>> {
        let path = format!("repos/{}/{}/stats/contributors", repo.owner, repo.repo);
        let key = (path.as_str(), as_of_version);
        let callback = |client: &client::Github| {
            client.get().custom_endpoint(&path).execute()
        };
        match self.get_cached(&self.contribs, key, callback, id) {
            Err(Error::TryAgainLater) => {
                thread::sleep(Duration::from_secs(1));
                match self.get_cached(&self.contribs, key, callback, id) {
                    Err(Error::TryAgainLater) => {
                        thread::sleep(Duration::from_secs(4));
                        self.get_cached(&self.contribs, key, callback, id)
                    },
                    res => res,
                }
            },
            Err(e) => Err(e.context("contributors")),
            res => res,
        }
    }

    fn get_cached<F, P, B, R>(&self, cache: &TempCache<(String, Option<R>)>, key: (&str, &str), cb: F, postproc: P) -> CResult<Option<R>>
        where P: FnOnce(B) -> R,
        F: FnOnce(&client::Github) -> Result<(HeaderMap, StatusCode, Option<serde_json::Value>), github_rs::errors::Error>,
        B: for <'de> serde::Deserialize<'de> + serde::Serialize + Clone + Send + 'static,
        R: for <'de> serde::Deserialize<'de> + serde::Serialize + Clone + Send + 'static
    {
        if let Some((ver, payload)) = cache.get(key.0)? {
            if ver == key.1 {
                return Ok(payload);
            }
            eprintln!("Cache near miss {}@{} vs {}", key.0, ver, key.1);
        }

        let client = &self.client()?;
        // eprintln!("Cache miss {}@{}", key.0, key.1);
        let (headers, status, body) = cb(&*client)?;
        eprintln!("Recvd {}@{} {:?} {:?}", key.0, key.1, status, headers);
        if let (Some(rl), Some(rs)) = (rate_limit_remaining(&headers), rate_limit_reset(&headers)) {
            let end_timestamp = Duration::from_secs(rs.into());
            let now = SystemTime::now().duration_since(UNIX_EPOCH)?;
            let wait = (end_timestamp.checked_sub(now)).and_then(|d| d.checked_div(rl + 2));
            if let Some(wait) = wait {
                if wait.as_secs() > 2 {
                    eprintln!("need to wait! {:?}", wait);
                    thread::sleep(wait);
                }
            }
        }

        let non_parsable_body = match status {
            StatusCode::ACCEPTED |
            StatusCode::CREATED => return Err(Error::TryAgainLater),
            StatusCode::NO_CONTENT |
            StatusCode::NOT_FOUND |
            StatusCode::GONE |
            StatusCode::MOVED_PERMANENTLY => true,
            _ => false,
        };

        let keep_cached = match status {
            StatusCode::NOT_FOUND |
            StatusCode::GONE |
            StatusCode::MOVED_PERMANENTLY => true,
            _ => status.is_success(),
        };

        match body.ok_or(Error::NoBody).and_then(|stats| Ok(postproc(serde_json::from_value(stats)?))) {
            Ok(val) => {
                let res = (key.1.to_string(), Some(val));
                if keep_cached {
                    cache.set(key.0, &res)?;
                }
                Ok(res.1)
            },
            Err(_) if non_parsable_body => {
                if keep_cached {
                    cache.set(key.0, (key.1.to_string(), None))?;
                }
                Ok(None)
            },
            Err(err) => Err(err)?
        }
    }

    fn get_cached_old<F, B>(&self, cache: &TempCache<(String, Payload)>, key: (&str, &str), cb: F) -> CResult<Option<B>>
        where B: for<'de> serde::Deserialize<'de> + Payloadable,
        F: FnOnce(&client::Github) -> Result<(HeaderMap, StatusCode, Option<serde_json::Value>), github_rs::errors::Error>
    {
        if let Some((ver, payload)) = cache.get(key.0)? {
            if ver == key.1 {
                return Ok(B::from(payload));
            }
        }

        let client = &self.client()?;
        let (headers, status, body) = cb(&*client)?;
        eprintln!("Recvd {}@{} {:?} {:?}", key.0, key.1, status, headers);
        if let (Some(rl), Some(rs)) = (rate_limit_remaining(&headers), rate_limit_reset(&headers)) {
            let end_timestamp = Duration::from_secs(rs.into());
            let now = SystemTime::now().duration_since(UNIX_EPOCH)?;
            let wait = (end_timestamp.checked_sub(now)).and_then(|d| d.checked_div(rl + 2));
            if let Some(wait) = wait {
                if wait.as_secs() > 2 {
                    eprintln!("need to wait! {:?}", wait);
                    thread::sleep(wait);
                }
            }
        }

        let non_parsable_body = match status {
            StatusCode::ACCEPTED |
            StatusCode::CREATED => return Err(Error::TryAgainLater),
            StatusCode::NO_CONTENT |
            StatusCode::NOT_FOUND |
            StatusCode::GONE |
            StatusCode::MOVED_PERMANENTLY => true,
            _ => false,
        };

        let keep_cached = match status {
            StatusCode::NOT_FOUND |
            StatusCode::GONE |
            StatusCode::MOVED_PERMANENTLY => true,
            _ => status.is_success(),
        };

        match body.ok_or(Error::NoBody).and_then(|stats| Ok(serde_json::from_value(stats)?)) {
            Ok(val) => {
                let val: B = val;
                if keep_cached {
                    cache.set(key.0, (key.1.to_string(), val.to()))?;
                }
                Ok(Some(val))
            },
            Err(_) if non_parsable_body => {
                if keep_cached {
                    cache.set(key.0, (key.1.to_string(), Payload::Dud))?;
                }
                Ok(None)
            },
            Err(err) => Err(err)?
        }
    }
}

fn id<T>(v: T) -> T {
    v
}

#[derive(Serialize, Deserialize, Debug, Clone)]
enum Payload {
    Meta(Vec<CommitMeta>),
    Contrib(Vec<UserContrib>),
    Res(SearchResults<User>),
    User(User),
    Topics(Topics),
    GitHubRepo(GitHubRepo),
    Dud,
}

impl Payloadable for Vec<CommitMeta> {
    fn to(&self) -> Payload {
        Payload::Meta(self.clone())
    }

    fn from(p: Payload) -> Option<Self> {
        match p {
            Payload::Meta(d) => Some(d), _ => None,
        }
    }
}

impl Payloadable for Vec<UserContrib> {
    fn to(&self) -> Payload {
        Payload::Contrib(self.clone())
    }

    fn from(p: Payload) -> Option<Self> {
        match p {
            Payload::Contrib(d) => Some(d), _ => None,
        }
    }
}

impl Payloadable for SearchResults<User> {
    fn to(&self) -> Payload {
        Payload::Res(self.clone())
    }

    fn from(p: Payload) -> Option<Self> {
        match p {
            Payload::Res(d) => Some(d), _ => None,
        }
    }
}

impl Payloadable for GitHubRepo {
    fn to(&self) -> Payload {
        Payload::GitHubRepo(self.clone())
    }

    fn from(p: Payload) -> Option<Self> {
        match p {
            Payload::GitHubRepo(d) => Some(d), _ => None,
        }
    }
}

impl Payloadable for User {
    fn to(&self) -> Payload {
        Payload::User(self.clone())
    }

    fn from(p: Payload) -> Option<Self> {
        match p {
            Payload::User(d) => Some(d), _ => None,
        }
    }
}
impl Payloadable for Topics {
    fn to(&self) -> Payload {
        Payload::Topics(self.clone())
    }

    fn from(p: Payload) -> Option<Self> {
        match p {
            Payload::Topics(d) => Some(d), _ => None,
        }
    }
}

pub(crate) trait Payloadable: Sized {
    fn to(&self) -> Payload;
    fn from(val: Payload) -> Option<Self>;
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
    gh.contributors(&repo, "").unwrap();
    gh.commits(&repo, "").unwrap();
}

#[test]
fn github_releases() {
    let gh = GitHub::new(
        "../data/github.db",
        std::env::var("GITHUB_TOKEN").expect("GITHUB_TOKEN env var")).unwrap();
    let repo = SimpleRepo{
        owner:"kornelski".into(),
        repo:"pngquant".into(),
    };
    assert!(gh.releases(&repo, "").unwrap().unwrap().len() > 2);
}

#[test]
fn test_user_by_email() {
    let gh = GitHub::new(
        "../data/github.db",
        std::env::var("GITHUB_TOKEN").expect("GITHUB_TOKEN env var")).unwrap();
    let user = gh.user_by_email("github@pornel.net").unwrap().unwrap();
    assert_eq!("kornelski", user.login);
}

