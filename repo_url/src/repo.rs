use std::borrow::Cow;
use std::convert::TryFrom;
use smartstring::alias::String as SmolStr;
use url::Url;

pub type GResult<T> = Result<T, GitError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    // as set by the create author
    pub url: Url,
    pub host: RepoHost,
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum RepoHost {
    GitHub(SimpleRepo),
    GitLab(SimpleRepo),
    BitBucket(SimpleRepo),
    Other,
}

#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct SimpleRepo {
    pub owner: SmolStr,
    pub repo: SmolStr,
}

impl SimpleRepo {
    pub fn new(owner: impl Into<SmolStr>, repo: impl Into<SmolStr>) -> Self {
        Self {
            owner: owner.into(),
            repo: repo.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum GitError {
    IncompleteUrl,
    InvalidUrl(url::ParseError),
}

impl std::error::Error for GitError {}

impl std::fmt::Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            GitError::IncompleteUrl => f.write_str("Incomplete URL"),
            GitError::InvalidUrl(e) => e.fmt(f),
        }
    }
}

impl Repo {
    /// Parse the given URL
    pub fn new(url: &str) -> GResult<Self> {
        let url = Url::parse(url).map_err(GitError::InvalidUrl)?;
        Ok(Repo {
            host: match (&url.host_str(), url.path_segments()) {
                (Some("www.github.com"), Some(path)) |
                (Some("github.com"), Some(path)) => {
                    RepoHost::GitHub(Self::repo_from_path(path)?)
                },
                (Some("www.gitlab.com"), Some(path)) |
                (Some("gitlab.com"), Some(path)) => {
                    RepoHost::GitLab(Self::repo_from_path(path)?)
                },
                (Some("bitbucket.org"), Some(path)) => {
                    RepoHost::BitBucket(Self::repo_from_path(path)?)
                },
                _ => RepoHost::Other,
            },
            url,
        })
    }

    fn repo_from_path<'a>(mut path: impl Iterator<Item = &'a str>) -> GResult<SimpleRepo> {
        let mut owner = SmolStr::from(path.next().ok_or(GitError::IncompleteUrl)?);
        owner.make_ascii_lowercase();
        let mut repo = SmolStr::from(path.next().ok_or(GitError::IncompleteUrl)?.trim_end_matches(".git"));
        repo.make_ascii_lowercase();
        Ok(SimpleRepo {
            owner,
            repo,
        })
    }

    /// True if the URL may be a well-known git repository URL
    pub fn looks_like_repo_url(url: &str) -> bool {
        Url::parse(url).ok().map_or(false, |url| match url.host_str() {
            Some("github.com") | Some("www.github.com") => true,
            Some("gitlab.com") | Some("www.gitlab.com") => true,
            Some("bitbucket.org") => true,
            _ => false,
        })
    }

    pub fn raw_url(&self) -> &str {
        self.url.as_str()
    }

    /// Enum with details of git hosting service
    pub fn host(&self) -> &RepoHost {
        &self.host
    }

    /// Enum with details of git hosting service
    pub fn github_host(&self) -> Option<&RepoHost> {
        match &self.host {
            ok @ RepoHost::GitHub(_) => Some(ok),
            _ => None,
        }
    }

    /// URL to view who contributed to the repository
    pub fn contributors_http_url(&self) -> Cow<'_, str> {
        match self.host {
            RepoHost::GitHub(SimpleRepo {ref owner, ref repo}) => {
                format!("https://github.com/{owner}/{repo}/graphs/contributors").into()
            },
            RepoHost::GitLab(SimpleRepo {ref owner, ref repo}) => {
                format!("https://gitlab.com/{owner}/{repo}/graphs/master").into()
            },
            RepoHost::BitBucket(SimpleRepo {ref owner, ref repo}) => {
                // not really…
                format!("https://bitbucket.org/{owner}/{repo}/commits/all").into()
            },
            RepoHost::Other => self.url.as_str().into(),
        }
    }

    /// Name of the hosting service
    pub fn site_link_label(&self) -> &'static str {
        match self.host {
            RepoHost::GitHub(..) => "GitHub",
            RepoHost::GitLab(..) => "GitLab",
            RepoHost::BitBucket(..) => "BitBucket",
            RepoHost::Other => "Source Code",
        }
    }

    /// URL for links in readmes hosted on the git website
    ///
    /// Base dir is without leading or trailing `/`, i.e. `""` for root, `"foo/bar"`, etc.
    pub fn readme_base_url(&self, base_dir_in_repo: &str, treeish_revision: Option<&str>) -> String {
        assert!(!base_dir_in_repo.starts_with('/'));
        let treeish_revision = treeish_revision.unwrap_or("HEAD");
        let slash = if !base_dir_in_repo.is_empty() && !base_dir_in_repo.ends_with('/') { "/" } else { "" };
        match self.host {
            RepoHost::GitHub(SimpleRepo {ref owner, ref repo}) => {
                format!("https://github.com/{owner}/{repo}/blob/{treeish_revision}/{base_dir_in_repo}{slash}")
            },
            RepoHost::GitLab(SimpleRepo {ref owner, ref repo}) => {
                format!("https://gitlab.com/{owner}/{repo}/blob/{treeish_revision}/{base_dir_in_repo}{slash}")
            },
            RepoHost::BitBucket(SimpleRepo {ref owner, ref repo}) => {
                format!("https://bitbucket.org/{owner}/{repo}/src/{treeish_revision}/{base_dir_in_repo}{slash}")
            },
            RepoHost::Other => self.url.to_string() // FIXME: how to add base dir?
        }
    }

    /// URL for image embeds in readmes hosted on the git website
    ///
    /// Base dir is without leading or trailing `/`, i.e. `""` for root, `"foo/bar"`, etc.
    pub fn readme_base_image_url(&self, base_dir_in_repo: &str, treeish_revision: Option<&str>) -> String {
        let treeish_revision = treeish_revision.unwrap_or("HEAD");
        assert!(!base_dir_in_repo.starts_with('/'));
        let slash = if !base_dir_in_repo.is_empty() && !base_dir_in_repo.ends_with('/') { "/" } else { "" };
        match self.host {
            RepoHost::GitHub(SimpleRepo {ref owner, ref repo}) => {
                format!("https://raw.githubusercontent.com/{owner}/{repo}/{treeish_revision}/{base_dir_in_repo}{slash}")
            },
            RepoHost::GitLab(SimpleRepo {ref owner, ref repo}) => {
                format!("https://gitlab.com/{owner}/{repo}/raw/{treeish_revision}/{base_dir_in_repo}{slash}")
            },
            RepoHost::BitBucket(SimpleRepo {ref owner, ref repo}) => {
                format!("https://bitbucket.org/{owner}/{repo}/raw/{treeish_revision}/{base_dir_in_repo}{slash}")
            },
            RepoHost::Other => self.url.to_string() // FIXME: how to add base dir?
        }
    }

    /// URL for browsing the repository via web browser
    pub fn canonical_http_url(&self, base_dir_in_repo: &str, treeish_revision: Option<&str>) -> Cow<'_, str> {
        self.host.canonical_http_url(base_dir_in_repo, treeish_revision).map(Cow::from)
            .unwrap_or_else(|| self.url.as_str().into())
    }

    pub fn canonical_git_url(&self) -> Cow<'_, str> {
        match self.host.canonical_git_url() {
            Some(s) => s.into(),
            None => self.url.as_str().into(),
        }
    }

    pub fn owner_name(&self) -> Option<&str> {
        self.host.owner_name()
    }

    pub fn repo_name(&self) -> Option<&str> {
        self.host.repo_name()
    }
}

impl RepoHost {
    /// URL for cloning the repository via git
    pub fn canonical_git_url(&self) -> Option<String> {
        match self {
            RepoHost::GitHub(SimpleRepo {ref owner, ref repo}) => {
                Some(format!("https://github.com/{owner}/{repo}.git"))
            },
            RepoHost::GitLab(SimpleRepo {ref owner, ref repo}) => {
                Some(format!("https://gitlab.com/{owner}/{repo}.git"))
            },
            RepoHost::BitBucket(SimpleRepo {ref owner, ref repo}) => {
                Some(format!("https://bitbucket.org/{owner}/{repo}"))
            },
            RepoHost::Other => None,
        }
    }

    /// URL for browsing the repository via web browser
    pub fn canonical_http_url(&self, base_dir_in_repo: &str, treeish_revision: Option<&str>) -> Option<String> {
        assert!(!base_dir_in_repo.starts_with('/'));
        let treeish_revision = treeish_revision.unwrap_or("HEAD");
        let path_part = if !base_dir_in_repo.is_empty() || treeish_revision != "HEAD" {
            let dir_name = match self {
                RepoHost::BitBucket(_) => "src",
                _ => "tree",
            };
            format!("/{dir_name}/{treeish_revision}/{base_dir_in_repo}")
        } else {
            String::new()
        };
        match self {
            RepoHost::GitHub(SimpleRepo {ref owner, ref repo}) => {
                Some(format!("https://github.com/{owner}/{repo}{path_part}"))
            },
            RepoHost::GitLab(SimpleRepo {ref owner, ref repo}) => {
                Some(format!("https://gitlab.com/{owner}/{repo}{path_part}"))
            },
            RepoHost::BitBucket(SimpleRepo {ref owner, ref repo}) => {
                Some(format!("https://bitbucket.org/{owner}/{repo}{path_part}"))
            },
            RepoHost::Other => None,
        }
    }

    pub fn owner_name(&self) -> Option<&str> {
        match self {
            RepoHost::GitHub(SimpleRepo { ref owner, .. }) |
            RepoHost::BitBucket(SimpleRepo { ref owner, .. }) |
            RepoHost::GitLab(SimpleRepo { ref owner, .. }) => Some(owner),
            RepoHost::Other => None,
        }
    }

    pub fn repo_name(&self) -> Option<&str> {
        self.repo().map(|r| &*r.repo)
    }

    pub fn repo(&self) -> Option<&SimpleRepo> {
        match self {
            RepoHost::GitHub(repo) |
            RepoHost::BitBucket(repo) |
            RepoHost::GitLab(repo) => Some(repo),
            RepoHost::Other => None,
        }
    }
}

impl TryFrom<RepoHost> for Repo {
    type Error = &'static str;

    fn try_from(host: RepoHost) -> Result<Self, Self::Error> {
        host.canonical_git_url()
            .and_then(|url| url.parse().ok())
            .map(|url| Repo {host, url})
            .ok_or("not a known git host")
    }
}

#[test]
fn repo_parse() {
    let repo = Repo::new("HTTPS://GITHUB.COM/FOO/BAR").unwrap();
    assert_eq!("https://github.com/foo/bar.git", repo.canonical_git_url());
    assert_eq!("https://github.com/foo/bar", repo.canonical_http_url("", None));
    assert_eq!("https://github.com/foo/bar/tree/HEAD/subdir", repo.canonical_http_url("subdir", None));
    assert_eq!("https://github.com/foo/bar/tree/HEAD/sub/dir", repo.canonical_http_url("sub/dir", None));

    let repo = Repo::new("HTTPS://GITlaB.COM/FOO/BAR").unwrap();
    assert_eq!("https://gitlab.com/foo/bar.git", repo.canonical_git_url());
    assert_eq!("https://gitlab.com/foo/bar/blob/HEAD/", repo.readme_base_url("", None));
    assert_eq!("https://gitlab.com/foo/bar/blob/main/foo/", repo.readme_base_url("foo", Some("main")));
    assert_eq!("https://gitlab.com/foo/bar/blob/HEAD/foo/bar/", repo.readme_base_url("foo/bar", None));
    assert_eq!("https://gitlab.com/foo/bar/raw/HEAD/baz/", repo.readme_base_image_url("baz/", None));
    assert_eq!("https://gitlab.com/foo/bar/raw/main/baz/", repo.readme_base_image_url("baz/", Some("main")));
    assert_eq!("https://gitlab.com/foo/bar/tree/HEAD/sub/dir", repo.canonical_http_url("sub/dir", None));

    let repo = Repo::new("http://priv@example.com/#111").unwrap();
    assert_eq!("http://priv@example.com/#111", repo.canonical_git_url());
    assert_eq!("http://priv@example.com/#111", repo.canonical_http_url("", None));

    let bad = Repo::new("N/A");
    assert!(bad.is_err());
}
