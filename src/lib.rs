extern crate cargo_author;
extern crate categories;
extern crate cargo_toml;
extern crate crates_index;
extern crate crates_io_client;
extern crate parse_cfg;
extern crate render_readme;
extern crate repo_url;
extern crate semver;

pub use cargo_author::*;
mod rich_crate;
pub use rich_crate::*;
mod rich_crate_version;
pub use rich_crate_version::*;

pub use repo_url::Repo;
pub use render_readme::Markup;
pub use render_readme::Readme;

/// URL-like identifier of location where crate has been published + normalized crate name
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum Origin {
    CratesIO(String)
}

impl Origin {
    pub fn from_crates_io_name(name: &str) -> Self {
        Origin::CratesIO(name.to_lowercase())
    }

    pub fn from_string(s: &str) -> Self {
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
