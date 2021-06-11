#[macro_use]
extern crate serde_derive;
pub use cargo_author::*;
use std::fmt;
mod rich_crate;
pub use crate::rich_crate::*;
mod rich_crate_version;
pub use crate::rich_crate_version::*;

pub use cargo_toml::Manifest;
pub use render_readme::Markup;
pub use render_readme::Readme;
pub use repo_url::Repo;
pub use repo_url::RepoHost;
pub use repo_url::SimpleRepo;

/// URL-like identifier of location where crate has been published + normalized crate name
#[derive(Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum Origin {
    CratesIo(Box<str>),
    GitHub { repo: SimpleRepo, package: Box<str> },
    GitLab { repo: SimpleRepo, package: Box<str> },
}

impl fmt::Debug for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Origin::CratesIo(name) => write!(f, "Origin( lib.rs/{} )", name),
            Origin::GitHub { repo, package } => write!(f, "Origin( github.com/{}/{} {} )", repo.owner, repo.repo, package),
            Origin::GitLab { repo, package } => write!(f, "Origin( gitlab.com/{}/{} {} )", repo.owner, repo.repo, package),
        }
    }
}

impl Origin {
    #[inline]
    pub fn from_crates_io_name(name: &str) -> Self {
        match Self::try_from_crates_io_name(name) {
            Some(n) => n,
            None => panic!("bad crate name: '{}'", name),
        }
    }

    #[inline]
    pub fn try_from_crates_io_name(name: &str) -> Option<Self> {
        if Self::is_valid_crate_name(name) {
            Some(Origin::CratesIo(name.to_ascii_lowercase().into()))
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn is_valid_crate_name(name: &str) -> bool {
        !name.is_empty() && is_alnum(name)
    }

    #[inline]
    pub fn from_github(repo: SimpleRepo, package: impl Into<Box<str>>) -> Self {
        Origin::GitHub { repo, package: package.into() }
    }

    #[inline]
    pub fn from_gitlab(repo: SimpleRepo, package: impl Into<Box<str>>) -> Self {
        Origin::GitLab { repo, package: package.into() }
    }

    #[inline]
    pub fn from_repo(r: &Repo, package: &str) -> Option<Self> {
        match r.host() {
            RepoHost::GitHub(r) => Some(Self::from_github(r.clone(), package)),
            RepoHost::GitLab(r) => Some(Self::from_gitlab(r.clone(), package)),
            _ => None,
        }
    }

    pub fn from_str(s: impl AsRef<str>) -> Self {
        let s = s.as_ref();
        let mut n = s.splitn(2, ':');
        let host = n.next().unwrap();
        match host {
            "crates.io" => Self::from_crates_io_name(n.next().expect("parse")),
            "github" | "gitlab" => {
                let mut n = n.next().expect("parse").splitn(3, '/');
                let owner = n.next().expect("parse").into();
                let repo = n.next().expect("parse").into();
                let package = n.next().expect("parse");
                if host == "github" {
                    Self::from_github(SimpleRepo { owner, repo }, package)
                } else {
                    Self::from_gitlab(SimpleRepo { owner, repo }, package)
                }
            },
            _ => panic!("bad str {}", s),
        }
    }

    pub fn to_str(&self) -> String {
        match *self {
            Origin::CratesIo(ref s) => format!("crates.io:{}", s),
            Origin::GitHub { ref repo, ref package } => format!("github:{}/{}/{}", repo.owner, repo.repo, package),
            Origin::GitLab { ref repo, ref package } => format!("gitlab:{}/{}/{}", repo.owner, repo.repo, package),
        }
    }

    #[inline]
    pub fn short_crate_name(&self) -> &str {
        match *self {
            Origin::CratesIo(ref s) => s,
            Origin::GitHub { ref package, .. } |
            Origin::GitLab { ref package, .. } => package,
        }
    }

    #[inline]
    pub fn is_crates_io(&self) -> bool {
        matches!(self, Origin::CratesIo(_))
    }

    #[inline]
    pub fn repo(&self) -> Option<(&SimpleRepo, &str)> {
        match *self {
            Origin::CratesIo(_) => None,
            Origin::GitHub { ref package, ref repo } |
            Origin::GitLab { ref package, ref repo } => Some((repo, package)),
        }
    }

    #[inline]
    pub fn into_repo(self) -> Option<(RepoHost, Box<str>)> {
        match self {
            Origin::CratesIo(_) => None,
            Origin::GitHub { package, repo } => Some((RepoHost::GitHub(repo), package)),
            Origin::GitLab { package, repo } => Some((RepoHost::GitLab(repo), package)),
        }
    }
}

#[test]
fn roundtrip() {
    let o1 = Origin::from_crates_io_name("hello");
    let o2 = Origin::from_str(o1.to_str());
    assert_eq!(o1, o2);
    assert_eq!("hello", o2.short_crate_name());
}

#[test]
fn roundtrip_gh() {
    let o1 = Origin::from_github(SimpleRepo { owner: "foo".into(), repo: "bar".into() }, "baz");
    let o3 = Origin::from_github(SimpleRepo { owner: "foo".into(), repo: "bar".into() }, "other_package");
    let o2 = Origin::from_str(o1.to_str());
    assert_eq!(o1, o2);
    assert_ne!(o1, o3);
    assert_eq!("baz", o2.short_crate_name());
}

fn is_alnum(q: &str) -> bool {
    q.as_bytes().iter().copied().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}
