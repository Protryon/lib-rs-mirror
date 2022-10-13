use std::time::Duration;
use repo_url::SimpleRepo;
use simple_cache::TempCache;
use std::future::Future;
use std::path::Path;

use github_v3::StatusCode;
use serde::{Deserialize, Serialize};

mod model;
pub use crate::model::*;

pub type CResult<T> = Result<T, Error>;
use quick_error::quick_error;

quick_error! {
    #[derive(Debug)]
    pub enum Error {
        NoBody(key: String) {
            display("Reponse with no body ({})", key)
        }
        TryAgainLater {
            display("Accepted, but no data available yet")
        }
        Cache(err: Box<simple_cache::Error>) {
            display("GH can't decode cache: {}", err)
            from(e: simple_cache::Error) -> (Box::new(e))
            source(err)
        }
        GitHub(err: String) {
            display("{}", err)
            from(e: github_v3::GHError) -> (e.to_string()) // non-Sync
        }
        Json(err: Box<serde_json::Error>, call: Option<&'static str>) {
            display("JSON decode error {} in {}", err, call.unwrap_or("github_info"))
            from(e: serde_json::Error) -> (Box::new(e), None)
            source(err)
        }
        Time(err: std::time::SystemTimeError) {
            display("{}", err)
            from()
            source(err)
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
    client: github_v3::Client,
    user_orgs: TempCache<(String, Option<Vec<UserOrg>>)>,
    orgs: TempCache<(String, Option<Org>)>,
    users: TempCache<(String, Option<User>)>,
    commits: TempCache<(String, Option<Vec<CommitMeta>>)>,
    releases: TempCache<(String, Option<Vec<GitHubRelease>>)>,
    contribs: TempCache<(String, Option<Vec<UserContrib>>)>,
    repos: TempCache<(String, Option<GitHubRepo>)>,
    emails: TempCache<(String, Option<Vec<User>>)>,
}

impl GitHub {
    pub fn new(cache_path: impl AsRef<Path>, token: &str) -> CResult<Self> {
        Ok(Self {
            client: github_v3::Client::new(Some(token)),
            user_orgs: TempCache::new(&cache_path.as_ref().with_file_name("github_user_orgs.bin"), Duration::from_secs(3600*24*14))?,
            orgs: TempCache::new(&cache_path.as_ref().with_file_name("github_orgs2.bin"), Duration::from_secs(3600*24*31))?,
            users: TempCache::new(&cache_path.as_ref().with_file_name("github_users3.bin"), Duration::from_secs(3600*24*31*2))?,
            commits: TempCache::new(&cache_path.as_ref().with_file_name("github_commits2.bin"), Duration::from_secs(3600*24*30))?,
            releases: TempCache::new(&cache_path.as_ref().with_file_name("github_releases2.bin"), Duration::from_secs(3600*24*8))?,
            contribs: TempCache::new(&cache_path.as_ref().with_file_name("github_contribs.bin"), Duration::from_secs(3600*24*31*2))?,
            repos: TempCache::new(&cache_path.as_ref().with_file_name("github_repos2.bin"), Duration::from_secs(3600*24*15))?,
            emails: TempCache::new(&cache_path.as_ref().with_file_name("github_emails.bin"), Duration::from_secs(3600*24*31*3))?,
        })
    }

    pub async fn user_by_email(&self, email: &str) -> CResult<Option<Vec<User>>> {
        let std_suffix = "@users.noreply.github.com";
        if let Some(rest) = email.strip_suffix(std_suffix) {
            let login = rest.split('+').last().unwrap();
            if let Some(user) = self.user_by_login(login).await? {
                return Ok(Some(vec![user]));
            }
        }
        self.get_cached(&self.emails, (email, ""), |client| client.get()
                       .path("search/users")
                       .query("q=in:email%20").arg(email)
                       .send(), |res: SearchResults<User>| {
                        println!("Found {email} = {:#?}", res.items);
                        res.items
                    }).await
    }

    pub async fn user_by_login(&self, login: &str) -> CResult<Option<User>> {
        if login.contains(':') {
            return Err(Error::GitHub(format!("bad login '{login}'")));
        }

        let key = login.to_ascii_lowercase();
        self.get_cached(&self.users, (&key, ""), |client| client.get()
                       .path("users").arg(login)
                       .send(), id).await.map_err(|e| e.context("user_by_login"))
    }

    pub async fn user_orgs(&self, login: &str) -> CResult<Option<Vec<UserOrg>>> {
        let key = login.to_ascii_lowercase();
        self.get_cached(&self.user_orgs, (&key, ""), |client| client.get()
                       .path("users").arg(login).path("orgs")
                       .send(), id).await.map_err(|e| e.context("user_orgs"))
    }

    pub async fn org(&self, login: &str) -> CResult<Option<Org>> {
        let key = login.to_ascii_lowercase();
        self.get_cached(&self.orgs, (&key, ""), |client| client.get()
                       .path("orgs").arg(login)
                       .send(), id).await.map_err(|e| e.context("user_orgs"))
    }

    pub async fn commits(&self, repo: &SimpleRepo, as_of_version: &str) -> CResult<Option<Vec<CommitMeta>>> {
        let key = format!("commits/{}/{}", repo.owner, repo.repo);
        self.get_cached(&self.commits, (&key, as_of_version), |client| client.get()
                           .path("repos").arg(&repo.owner).arg(&repo.repo)
                           .path("commits")
                           .send(), id).await.map_err(|e| e.context("commits"))
    }

    pub async fn releases(&self, repo: &SimpleRepo, as_of_version: &str) -> CResult<Option<Vec<GitHubRelease>>> {
        let key = format!("release/{}/{}", repo.owner, repo.repo);
        self.get_cached(&self.releases, (&key, as_of_version), |client| client.get()
                           .path("repos").arg(&repo.owner).arg(&repo.repo).path("releases")
                           .send(), id).await.map_err(|e| e.context("releases"))
    }

    pub async fn topics(&self, repo: &SimpleRepo, as_of_version: &str) -> CResult<Option<Vec<String>>> {
        let repo = self.repo(repo, as_of_version).await?;
        Ok(repo.map(|r| r.topics))
    }

    pub async fn repo(&self, repo: &SimpleRepo, as_of_version: &str) -> CResult<Option<GitHubRepo>> {
        let key = format!("{}/{}", repo.owner, repo.repo);
        self.get_cached(&self.repos, (&key, as_of_version), |client| client.get()
                .path("repos").arg(&repo.owner).arg(&repo.repo)
                .send(), |mut ghdata: GitHubRepo| {
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
                }).await
                .map_err(|e| e.context("repo"))
    }

    pub async fn contributors(&self, repo: &SimpleRepo, as_of_version: &str) -> CResult<Option<Vec<UserContrib>>> {
        let path = format!("repos/{}/{}/stats/contributors", repo.owner, repo.repo);
        let key = (path.as_str(), as_of_version);
        self.get_cached(&self.contribs, key, |client: &github_v3::Client| {
            client.get().path("repos").arg(&repo.owner).arg(&repo.repo).path("stats/contributors").send()
        }, id).await
    }

    async fn get_cached<F, P, B, R, A>(&self, cache: &TempCache<(String, Option<R>)>, key: (&str, &str), cb: F, postproc: P) -> CResult<Option<R>>
    where
        P: FnOnce(B) -> R,
        F: FnOnce(&github_v3::Client) -> A,
        A: Future<Output = Result<github_v3::Response, github_v3::GHError>>,
        B: for<'de> serde::Deserialize<'de> + serde::Serialize + Clone + Send + 'static,
        R: for<'de> serde::Deserialize<'de> + serde::Serialize + Clone + Send + 'static,
    {
        if let Some((ver, payload)) = cache.get(key.0)? {
            if ver == key.1 {
                return Ok(payload);
            }
            eprintln!("Cache near miss {}@{ver} vs {}", key.0, key.1);
        }

        let (status, res) = match Box::pin(cb(&self.client)).await {
            Ok(res) => {
                let status = res.status();
                let headers = res.headers();
                eprintln!("Recvd {}@{} {status:?} {headers:?}", key.0, key.1);
                (status, Some(res))
            },
            Err(github_v3::GHError::Response { status, message }) => {
                eprintln!("GH Error {status} {}", message.as_deref().unwrap_or("??"));
                (status, None)
            },
            Err(e) => return Err(e.into()),
        };
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
        let body = match res {
            Some(res) if !non_parsable_body => Some(Box::pin(res.obj()).await?),
            _ => None,
        };
        match body.ok_or_else(|| Error::NoBody(format!("{},{}", key.0, key.1))).and_then(|stats| {
            let dbg = format!("stats={stats:?}");
            Ok(postproc(serde_json::from_value(stats).map_err(|e| {
                eprintln!("Error matching JSON: {e}\n {} data: {dbg}", key.0); e
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
            Err(err) => Err(err),
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

#[cfg(test)]
#[tokio::test(flavor = "multi_thread")]
async fn github_contrib() {
    let gh = GitHub::new(
        "../data/github.db",
        &std::env::var("GITHUB_TOKEN").expect("GITHUB_TOKEN env var")).unwrap();
    let repo = SimpleRepo{
        owner:"visionmedia".into(),
        repo:"superagent".into(),
    };
    gh.contributors(&repo, "").await.unwrap();
    gh.commits(&repo, "").await.unwrap();
}

#[cfg(test)]
#[tokio::test(flavor = "multi_thread")]
async fn github_releases() {
    let gh = GitHub::new(
        "../data/github.db",
        &std::env::var("GITHUB_TOKEN").expect("GITHUB_TOKEN env var")).unwrap();
    let repo = SimpleRepo{
        owner:"ImageOptim".into(),
        repo:"gifski".into(),
    };
    let releases = gh.releases(&repo, "").await.unwrap().unwrap();
    assert!(releases.len() > 4, "{releases:?}");
}

#[cfg(test)]
#[tokio::test(flavor = "multi_thread")]
async fn test_user_by_email() {
    let gh = GitHub::new(
        "../data/github.db",
        &std::env::var("GITHUB_TOKEN").expect("GITHUB_TOKEN env var")).unwrap();
    let user = gh.user_by_email("github@pornel.net").await.unwrap().unwrap();
    assert_eq!("kornelski", user[0].login);
}

