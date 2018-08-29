extern crate cargo_author;
extern crate cargo_toml;
extern crate categories;
extern crate crates_index;
extern crate crates_io_client;
extern crate parse_cfg;
extern crate render_readme;
extern crate repo_url;
extern crate semver;
extern crate serde;
#[macro_use]
extern crate serde_derive;

pub use cargo_author::*;
mod rich_crate;
pub use rich_crate::*;
mod rich_crate_version;
pub use rich_crate_version::*;

pub use render_readme::Markup;
pub use render_readme::Readme;
pub use repo_url::Repo;
pub use repo_url::RepoHost;

/// URL-like identifier of location where crate has been published + normalized crate name
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum Origin {
    CratesIO(Box<str>),
}

impl Origin {
    pub fn from_crates_io_name(name: &str) -> Self {
        Origin::CratesIO(name.to_lowercase().into())
    }

    pub fn from_string(s: impl AsRef<str>) -> Self {
        let s = s.as_ref();
        let mut n = s.split(':');
        assert_eq!("crates.io", n.next().unwrap());
        Self::from_crates_io_name(n.next().unwrap())
    }

    pub fn to_str(&self) -> String {
        match *self {
            Origin::CratesIO(ref s) => format!("crates.io:{}", s),
        }
    }
}

#[test]
fn roundtrip() {
    let o1 = Origin::from_crates_io_name("hello");
    let o2 = Origin::from_string(o1.to_str());
    assert_eq!(o1, o2);
}
