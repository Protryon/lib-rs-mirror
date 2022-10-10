use std::borrow::Cow;
use smartstring::alias::String as SmolStr;
use url::Url;

pub type GResult<T> = Result<T, GitError>;


#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum Repo {
    GitHub(SimpleRepo),
    GitLab(SimpleRepo),
    BitBucket(SimpleRepo),
    /// as set by the create author
    Other(Url),
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
        Ok(match (&url.host_str(), url.path_segments()) {
            (Some("www.github.com" | "github.com"), Some(path)) => {
                Self::GitHub(Self::repo_from_path(path)?)
            },
            (Some("www.gitlab.com" | "gitlab.com"), Some(path)) => {
                Self::GitLab(Self::repo_from_path(path)?)
            },
            (Some("bitbucket.org"), Some(path)) => {
                Self::BitBucket(Self::repo_from_path(path)?)
            },
            _ => Self::Other(url),
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
            Some("github.com" | "www.github.com") => true,
            Some("gitlab.com" | "www.gitlab.com") => true,
            Some("bitbucket.org") => true,
            _ => false,
        })
    }

    /// Enum with details of git hosting service
    pub fn host(&self) -> &Repo {
        self
    }

    /// Enum with details of git hosting service
    pub fn github_host(&self) -> Option<&Repo> {
        match self {
            ok @ Self::GitHub(_) => Some(ok),
            _ => None,
        }
    }

    /// URL to view who contributed to the repository
    pub fn contributors_http_url(&self) -> Cow<'_, str> {
        match self {
            Self::GitHub(SimpleRepo {ref owner, ref repo}) => {
                format!("https://github.com/{owner}/{repo}/graphs/contributors").into()
            },
            Self::GitLab(SimpleRepo {ref owner, ref repo}) => {
                format!("https://gitlab.com/{owner}/{repo}/graphs/master").into()
            },
            Self::BitBucket(SimpleRepo {ref owner, ref repo}) => {
                // not really…
                format!("https://bitbucket.org/{owner}/{repo}/commits/all").into()
            },
            Self::Other(url) => url.as_str().into(),
        }
    }

    /// Name of the hosting service
    pub fn site_link_label(&self) -> &'static str {
        match self {
            Self::GitHub(..) => "GitHub",
            Self::GitLab(..) => "GitLab",
            Self::BitBucket(..) => "BitBucket",
            Self::Other(_) => "Source Code",
        }
    }

    /// URL for links in readmes hosted on the git website
    ///
    /// Base dir is without leading or trailing `/`, i.e. `""` for root, `"foo/bar"`, etc.
    pub fn readme_base_url(&self, base_dir_in_repo: &str, treeish_revision: Option<&str>) -> String {
        assert!(!base_dir_in_repo.starts_with('/'));
        let treeish_revision = treeish_revision.unwrap_or("HEAD");
        let slash = if !base_dir_in_repo.is_empty() && !base_dir_in_repo.ends_with('/') { "/" } else { "" };
        match self {
            Self::GitHub(SimpleRepo {ref owner, ref repo}) => {
                format!("https://github.com/{owner}/{repo}/blob/{treeish_revision}/{base_dir_in_repo}{slash}")
            },
            Self::GitLab(SimpleRepo {ref owner, ref repo}) => {
                format!("https://gitlab.com/{owner}/{repo}/blob/{treeish_revision}/{base_dir_in_repo}{slash}")
            },
            Self::BitBucket(SimpleRepo {ref owner, ref repo}) => {
                format!("https://bitbucket.org/{owner}/{repo}/src/{treeish_revision}/{base_dir_in_repo}{slash}")
            },
            Self::Other(url) => url.to_string() // FIXME: how to add base dir?
        }
    }

    /// URL for image embeds in readmes hosted on the git website
    ///
    /// Base dir is without leading or trailing `/`, i.e. `""` for root, `"foo/bar"`, etc.
    pub fn readme_base_image_url(&self, base_dir_in_repo: &str, treeish_revision: Option<&str>) -> String {
        let treeish_revision = treeish_revision.unwrap_or("HEAD");
        assert!(!base_dir_in_repo.starts_with('/'));
        let slash = if !base_dir_in_repo.is_empty() && !base_dir_in_repo.ends_with('/') { "/" } else { "" };
        match self {
            Self::GitHub(SimpleRepo {ref owner, ref repo}) => {
                format!("https://raw.githubusercontent.com/{owner}/{repo}/{treeish_revision}/{base_dir_in_repo}{slash}")
            },
            Self::GitLab(SimpleRepo {ref owner, ref repo}) => {
                format!("https://gitlab.com/{owner}/{repo}/raw/{treeish_revision}/{base_dir_in_repo}{slash}")
            },
            Self::BitBucket(SimpleRepo {ref owner, ref repo}) => {
                format!("https://bitbucket.org/{owner}/{repo}/raw/{treeish_revision}/{base_dir_in_repo}{slash}")
            },
            Self::Other(url) => url.to_string() // FIXME: how to add base dir?
        }
    }

    /// URL for cloning the repository via git
    pub fn canonical_git_url(&self) -> String {
        match self {
            Self::GitHub(SimpleRepo {ref owner, ref repo}) => {
                format!("https://github.com/{owner}/{repo}.git")
            },
            Self::GitLab(SimpleRepo {ref owner, ref repo}) => {
                format!("https://gitlab.com/{owner}/{repo}.git")
            },
            Self::BitBucket(SimpleRepo {ref owner, ref repo}) => {
                format!("https://bitbucket.org/{owner}/{repo}")
            },
            Self::Other(url) => url.to_string()
        }
    }

    /// URL for browsing the repository via web browser
    pub fn canonical_http_url(&self, base_dir_in_repo: &str, treeish_revision: Option<&str>) -> String {
        assert!(!base_dir_in_repo.starts_with('/'));
        let treeish_revision = treeish_revision.unwrap_or("HEAD");
        let path_part = if !base_dir_in_repo.is_empty() || treeish_revision != "HEAD" {
            let dir_name = match self {
                Self::BitBucket(_) => "src",
                _ => "tree",
            };
            format!("/{dir_name}/{treeish_revision}/{base_dir_in_repo}")
        } else {
            String::new()
        };
        match self {
            Self::GitHub(SimpleRepo {ref owner, ref repo}) => {
                format!("https://github.com/{owner}/{repo}{path_part}")
            },
            Self::GitLab(SimpleRepo {ref owner, ref repo}) => {
                format!("https://gitlab.com/{owner}/{repo}{path_part}")
            },
            Self::BitBucket(SimpleRepo {ref owner, ref repo}) => {
                format!("https://bitbucket.org/{owner}/{repo}{path_part}")
            },
            Self::Other(url) => url.to_string(),
        }
    }

    pub fn owner_name(&self) -> Option<&str> {
        match self {
            Self::GitHub(SimpleRepo { ref owner, .. }) |
            Self::BitBucket(SimpleRepo { ref owner, .. }) |
            Self::GitLab(SimpleRepo { ref owner, .. }) => Some(owner),
            Self::Other(_) => None,
        }
    }

    pub fn repo_name(&self) -> Option<&str> {
        self.repo().map(|r| &*r.repo)
    }

    pub fn repo(&self) -> Option<&SimpleRepo> {
        match self {
            Self::GitHub(repo) |
            Self::BitBucket(repo) |
            Self::GitLab(repo) => Some(repo),
            Self::Other(_) => None,
        }
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
