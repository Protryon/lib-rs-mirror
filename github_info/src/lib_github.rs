use github_rs;
use serde;

use std::path::Path;

use github_rs::client;
use github_rs::client::Executor;
use github_rs::headers::{rate_limit_remaining, rate_limit_reset};
use github_rs::{HeaderMap, StatusCode};
use repo_url::SimpleRepo;
use simple_cache::TempCache;
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};
use urlencoding::encode;
use serde_derive::*;

mod model;
pub use crate::model::*;

pub type CResult<T> = Result<T, Error>;
use quick_error::quick_error;

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
    orgs: TempCache<(String, Option<Vec<UserOrg>>)>,
    users: TempCache<(String, Option<User>)>,
    commits: TempCache<(String, Option<Vec<CommitMeta>>)>,
    releases: TempCache<(String, Option<Vec<GitHubRelease>>)>,
    contribs: TempCache<(String, Option<Vec<UserContrib>>)>,
    repos: TempCache<(String, Option<GitHubRepo>)>,
    emails: TempCache<(String, Option<Vec<User>>)>,
}

impl GitHub {
    pub fn new(cache_path: impl AsRef<Path>, token: impl Into<String>) -> CResult<Self> {
        Ok(Self {
            token: token.into(),
            orgs: TempCache::new(&cache_path.as_ref().with_file_name("github_orgs.bin"))?,
            users: TempCache::new(&cache_path.as_ref().with_file_name("github_users.bin"))?,
            commits: TempCache::new(&cache_path.as_ref().with_file_name("github_commits.bin"))?,
            releases: TempCache::new(&cache_path.as_ref().with_file_name("github_releases.bin"))?,
            contribs: TempCache::new(&cache_path.as_ref().with_file_name("github_contribs.bin"))?,
            repos: TempCache::new(&cache_path.as_ref().with_file_name("github_repos2.bin"))?,
            emails: TempCache::new(&cache_path.as_ref().with_file_name("github_emails.bin"))?,
        })
    }

    fn client(&self) -> CResult<client::Github> {
        Ok(client::Github::new(&self.token)?)
    }

    pub fn user_by_email(&self, email: &str) -> CResult<Option<Vec<User>>> {
        let std_suffix = "@users.noreply.github.com";
        if email.ends_with(std_suffix) {
            let login = email[0..email.len() - std_suffix.len()].split('+').last().unwrap();
            if let Some(user) = self.user_by_login(login)? {
                return Ok(Some(vec![user]));
            }
        }
        let enc_email = encode(email);
        self.get_cached(&self.emails, (email, ""), |client| client.get()
                       .custom_endpoint(&format!("search/users?q=in:email%20{}", enc_email))
                       .execute(), |res: SearchResults<User>| {
                        println!("Found {} = {:#?}", email, res.items);
                        res.items
                    })
    }

    pub fn user_by_login(&self, login: &str) -> CResult<Option<User>> {
        let key = login.to_ascii_lowercase();
        self.get_cached(&self.users, (&key, ""), |client| client.get()
                       .users().username(login)
                       .execute(), id).map_err(|e| e.context("user_by_login"))
    }

    pub fn user_by_id(&self, user_id: u32) -> CResult<Option<User>> {
        let user_id = user_id.to_string();
        self.get_cached(&self.users, (&user_id, ""), |client| client.get()
                       .users().username(&user_id)
                       .execute(), id).map_err(|e| e.context("user_by_id"))
    }

    pub fn user_orgs(&self, login: &str) -> CResult<Option<Vec<UserOrg>>> {
        let key = login.to_ascii_lowercase();
        self.get_cached(&self.orgs, (&key, ""), |client| client.get()
                       .users().username(login).orgs()
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
        let repo = self.repo(repo, as_of_version)?;
        Ok(repo.map(|r| r.topics))
    }

    pub fn repo(&self, repo: &SimpleRepo, as_of_version: &str) -> CResult<Option<GitHubRepo>> {
        let key = format!("{}/{}", repo.owner, repo.repo);
        self.get_cached(&self.repos, (&key, as_of_version), |client| client.get()
                .repos().owner(&repo.owner).repo(&repo.repo)
                .execute(), |mut ghdata: GitHubRepo| {
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
                    ghdata
                })
                .map_err(|e| e.context("repo"))
    }

    pub fn contributors(&self, repo: &SimpleRepo, as_of_version: &str) -> CResult<Option<Vec<UserContrib>>> {
        let path = format!("repos/{}/{}/stats/contributors", repo.owner, repo.repo);
        let key = (path.as_str(), as_of_version);
        let callback = |client: &client::Github| {
            client.get().custom_endpoint(&path).execute()
        };
        let mut retries = 5;
        let mut delay = 1;
        loop {
            match self.get_cached(&self.contribs, key, callback, id) {
                Err(Error::TryAgainLater) if retries > 0 => {
                    thread::sleep(Duration::from_secs(delay));
                    retries -= 1;
                    delay *= 2;
                },
                Err(e) => return Err(e.context("contributors")),
                res => return res,
            }
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
                if wait.as_secs() > 2 && (rl < 8 || wait.as_secs() < 15) {
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

        match body.ok_or(Error::NoBody).and_then(|stats| {
            let dbg = format!("{:?}", stats);
            Ok(postproc(serde_json::from_value(stats).map_err(|e| {
                eprintln!("Error matching JSON: {}\n data: {}", e, dbg); e
            })?))
        }) {
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
            Err(err) => Err(err)?,
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
    assert_eq!("kornelski", user[0].login);
}

