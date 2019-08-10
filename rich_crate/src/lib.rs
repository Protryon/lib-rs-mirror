#[macro_use]
extern crate serde_derive;
use std::fmt;
pub use cargo_author::*;
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
#[derive(Clone, Eq, PartialEq, Hash)]
pub enum Origin {
    CratesIo(Box<str>),
    GitHub { repo: SimpleRepo, package: Box<str> },
}

impl fmt::Debug for Origin {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Origin::CratesIo(name) => write!(f, "Origin( lib.rs/{} )", name),
            Origin::GitHub {repo, package} => write!(f, "Origin( github.com/{}/{}#{} )", repo.owner, repo.repo, package),
        }
    }
}

impl Origin {
    pub fn from_crates_io_name(name: &str) -> Self {
        Origin::CratesIo(name.to_ascii_lowercase().into())
    }

    pub fn from_github(repo: SimpleRepo, package: impl Into<Box<str>>) -> Self {
        Origin::GitHub { repo, package: package.into() }
    }

    pub fn from_repo(r: &Repo, package: &str) -> Option<Self> {
        match r.host() {
            RepoHost::GitHub(r) => Some(Self::from_github(r.clone(), package)),
            _ => None,
        }
    }

    pub fn from_str(s: impl AsRef<str>) -> Self {
        let s = s.as_ref();
        let mut n = s.splitn(2, ':');
        let host = n.next().unwrap();
        match host {
            "crates.io" => Self::from_crates_io_name(n.next().expect("parse")),
            "github" => {
                let mut n = n.next().expect("parse").splitn(3, "/");
                let owner = n.next().expect("parse").into();
                let repo = n.next().expect("parse").into();
                let package = n.next().expect("parse");
                Self::from_github(SimpleRepo {owner, repo}, package)
            },
            _ => panic!("bad str {}", s),
        }
    }

    pub fn to_str(&self) -> String {
        match *self {
            Origin::CratesIo(ref s) => format!("crates.io:{}", s),
            Origin::GitHub { ref repo, ref package } => format!("github:{}/{}/{}", repo.owner, repo.repo, package),
        }
    }

    pub fn short_crate_name(&self) -> &str {
        match *self {
            Origin::CratesIo(ref s) => s,
            Origin::GitHub { ref package, .. } => package,
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
