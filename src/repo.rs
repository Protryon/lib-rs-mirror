extern crate url;
use url::Url;
use std::borrow::Cow;

pub type GResult<T> = Result<T, GitError>;

#[derive(Debug, Clone)]
pub struct Repo {
    // as set by the create author
    pub url: Url,
    pub host: RepoHost,
}

#[derive(Debug, Clone)]
pub enum RepoHost {
    GitHub(SimpleRepo),
    GitLab(SimpleRepo),
    Other,
}

#[derive(Debug, Clone)]
pub struct SimpleRepo {
    pub owner: String,
    pub repo: String,
}

#[derive(Debug, Clone)]
pub enum GitError {
    IncompleteUrl,
    InvalidUrl(url::ParseError),
}

impl std::error::Error for GitError {
    fn description(&self) -> &str {"git"}
}

impl std::fmt::Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            GitError::IncompleteUrl => f.write_str("Incomplete URL"),
            GitError::InvalidUrl(e) => e.fmt(f),
        }
    }
}

impl Repo {
    /// Parse the given URL
    pub fn new(url: &str) -> GResult<Self> {
        let url = Url::parse(url).map_err(|e| GitError::InvalidUrl(e))?;
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
                _ => RepoHost::Other,
            },
            url,
        })
    }

    fn repo_from_path<'a>(mut path: impl Iterator<Item = &'a str>) -> GResult<SimpleRepo> {
        Ok(SimpleRepo {
            owner: path.next().ok_or(GitError::IncompleteUrl)?.to_lowercase().to_string(),
            repo: path.next().ok_or(GitError::IncompleteUrl)?.to_lowercase().trim_right_matches(".git").to_string(),
        })
    }

    /// True if the URL may be a well-known git repository URL
    pub fn looks_like_repo_url(url: &str) -> bool {
        Url::parse(url).ok().map_or(false, |url| {
            match url.host_str() {
                Some("github.com") | Some("www.github.com") => true,
                Some("gitlab.com") | Some("www.gitlab.com") => true,
                _ => false,
            }
        })
    }

    pub fn raw_url(&self) -> &str {
        self.url.as_str()
    }

    /// Enum with details of git hosting service
    pub fn host(&self) -> &RepoHost {
        &self.host
    }

    /// URL to view who contributed to the repository
    pub fn contributors_http_url(&self) -> Cow<str> {
        match self.host {
            RepoHost::GitHub(SimpleRepo {ref owner, ref repo}) => {
                format!("https://github.com/{}/{}/graphs/contributors", owner, repo).into()
            },
            RepoHost::GitLab(SimpleRepo {ref owner, ref repo}) => {
                format!("https://gitlab.com/{}/{}/graphs/master", owner, repo).into()
            },
            RepoHost::Other => self.url.as_str().into(),
        }
    }

    /// Name of the hosting service
    pub fn site_link_label(&self) -> &'static str {
        match self.host {
            RepoHost::GitHub(..) => "GitHub",
            RepoHost::GitLab(..) => "GitLab",
            RepoHost::Other => "Source Code",
        }
    }

    /// URL for links in readmes hosted on the git website
    ///
    /// Base dir is without leading or trailing `/`, i.e. `""` for root, `"foo/bar"`, etc.
    pub fn readme_base_url(&self, base_dir_in_repo: &str) -> String {
        assert!(!base_dir_in_repo.starts_with('/'));
        let slash = if base_dir_in_repo != "" && !base_dir_in_repo.ends_with('/') {"/"} else {""};
        match self.host {
            RepoHost::GitHub(SimpleRepo {ref owner, ref repo}) => {
                format!("https://github.com/{}/{}/blob/master/{}{}", owner, repo, base_dir_in_repo, slash)
            },
            RepoHost::GitLab(SimpleRepo {ref owner, ref repo}) => {
                format!("https://gitlab.com/{}/{}/blob/master/{}{}", owner, repo, base_dir_in_repo, slash)
            },
            RepoHost::Other => self.url.to_string() // FIXME: how to add base dir?
        }
    }

    /// URL for image embeds in readmes hosted on the git website
    ///
    /// Base dir is without leading or trailing `/`, i.e. `""` for root, `"foo/bar"`, etc.
    pub fn readme_base_image_url(&self, base_dir_in_repo: &str) -> String {
        assert!(!base_dir_in_repo.starts_with('/'));
        let slash = if base_dir_in_repo != "" && !base_dir_in_repo.ends_with('/') {"/"} else {""};
        match self.host {
            RepoHost::GitHub(SimpleRepo {ref owner, ref repo}) => {
                format!("https://raw.githubusercontent.com/{}/{}/master/{}{}", owner, repo, base_dir_in_repo, slash)
            },
            RepoHost::GitLab(SimpleRepo {ref owner, ref repo}) => {
                format!("https://gitlab.com/{}/{}/raw/master/{}{}", owner, repo, base_dir_in_repo, slash)
            },
            RepoHost::Other => self.url.to_string() // FIXME: how to add base dir?
        }
    }

    /// URL for browsing the repository via web browser
    pub fn canonical_http_url(&self, base_dir_in_repo: &str) -> Cow<str> {
        assert!(!base_dir_in_repo.starts_with('/'));
        let slash = if base_dir_in_repo != "" {"/tree/master/"} else {""};
        match self.host {
            RepoHost::GitHub(SimpleRepo {ref owner, ref repo}) => {
                format!("https://github.com/{}/{}{}{}", owner, repo, slash, base_dir_in_repo).into()
            },
            RepoHost::GitLab(SimpleRepo {ref owner, ref repo}) => {
                format!("https://gitlab.com/{}/{}{}{}", owner, repo, slash, base_dir_in_repo).into()
            },
            RepoHost::Other => self.url.as_str().into(), // FIXME: how to add base dir?
        }
    }

    /// URL for cloning the repository via git
    pub fn canonical_git_url(&self) -> Cow<str> {
        match self.host {
            RepoHost::GitHub(SimpleRepo {ref owner, ref repo}) => {
                format!("https://github.com/{}/{}.git", owner, repo).into()
            },
            RepoHost::GitLab(SimpleRepo {ref owner, ref repo}) => {
                format!("https://gitlab.com/{}/{}.git", owner, repo).into()
            }
            RepoHost::Other => self.url.as_str().into(),
        }
    }
}

#[test]
fn repo_parse() {
    let repo = Repo::new("HTTPS://GITHUB.COM/FOO/BAR").unwrap();
    assert_eq!("https://github.com/foo/bar.git", repo.canonical_git_url());
    assert_eq!("https://github.com/foo/bar", repo.canonical_http_url(""));
    assert_eq!("https://github.com/foo/bar/tree/master/subdir", repo.canonical_http_url("subdir"));
    assert_eq!("https://github.com/foo/bar/tree/master/sub/dir", repo.canonical_http_url("sub/dir"));

    let repo = Repo::new("HTTPS://GITlaB.COM/FOO/BAR").unwrap();
    assert_eq!("https://gitlab.com/foo/bar.git", repo.canonical_git_url());
    assert_eq!("https://gitlab.com/foo/bar/blob/master/", repo.readme_base_url(""));
    assert_eq!("https://gitlab.com/foo/bar/blob/master/foo/", repo.readme_base_url("foo"));
    assert_eq!("https://gitlab.com/foo/bar/blob/master/foo/bar/", repo.readme_base_url("foo/bar"));
    assert_eq!("https://gitlab.com/foo/bar/raw/master/baz/", repo.readme_base_image_url("baz/"));
    assert_eq!("https://gitlab.com/foo/bar/tree/master/sub/dir", repo.canonical_http_url("sub/dir"));

    let repo = Repo::new("http://priv@example.com/#111").unwrap();
    assert_eq!("http://priv@example.com/#111", repo.canonical_git_url());
    assert_eq!("http://priv@example.com/#111", repo.canonical_http_url(""));

    let bad = Repo::new("N/A");
    assert!(bad.is_err());
}
