use chrono::DateTime;
use chrono::Utc;
use serde_derive::*;
use ahash::HashMap;
use smartstring::alias::String as SmolStr;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateMetaFile {
    #[serde(rename = "crate")]
    pub krate: CrateMeta,
    pub versions: Vec<CrateMetaVersion>,
    pub keywords: Vec<CrateMetaKeyword>,
    pub categories: Vec<CrateMetaCategory>,
}

impl CrateMetaFile {
    pub fn versions(&self) -> impl Iterator<Item=CrateMetaVersion> + '_ {
        self.krate.versions.iter().filter_map(move |&id| {
            self.versions.iter().find(|v| v.id == id).cloned()
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateMetaBadge {
    pub badge_type: SmolStr,
    pub attributes: CrateMetaBadgeAttr,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateMetaBadgeAttr {
    pub repository: Option<Box<str>>,
    pub branch: Option<Box<str>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateMeta {
    pub id: SmolStr,         // "cargo-deb",
    pub name: SmolStr,       // "cargo-deb",
    pub updated_at: Box<str>, // "2018-04-26T00:57:41.867975+00:00",
    #[serde(default)]
    pub versions: Vec<usize>, // [90309, 89288, 87534, 86387, 82743, 82697, 81712, 81233, 79188, 76393, 69921, 65169, 64103, 62665, 62074, 61494, 61440, 61393, 61273, 61237, 61236],
    #[serde(default)]
    pub keywords: Vec<SmolStr>, // ["cargo", "subcommand", "debian", "package", "deb"],
    #[serde(default)]
    pub categories: Vec<SmolStr>, // ["development-tools::build-utils", "command-line-utilities", "development-tools::cargo-plugins"],
    #[serde(default)]
    pub badges: Vec<CrateMetaBadge>,
    pub created_at: Box<str>, // "2017-07-31T23:46:03.490855+00:00",
    // #[serde(default)]
    // pub downloads: usize, // 3393, // this meta is updated once per release, not frequently enough to keep downloads relevant
    #[serde(default)]
    pub recent_downloads: Option<usize>, // 1314,
    pub max_version: SmolStr, // "1.10.0",
    pub description: Option<Box<str>>, // "Make Debian packages (.deb) easily with a Cargo subcommand",
    pub homepage: Option<Box<str>>, // "https://github.com/mmstick/cargo-deb#readme",
    pub documentation: Option<Box<str>>, // "https://docs.rs/cargo-deb",
    pub repository: Option<Box<str>>, // "https://github.com/mmstick/cargo-deb",
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateMetaVersion {
    pub id: usize, // 79188,
    #[serde(rename = "crate")]
    pub krate: SmolStr, // "cargo-deb",
    pub num: SmolStr, // "1.4.0",
    // pub dl_path: String, // "/api/v1/crates/cargo-deb/1.4.0/download",
    // pub readme_path: String, // "/api/v1/crates/cargo-deb/1.4.0/readme",
    pub updated_at: Box<str>, // "2018-01-29T23:10:11.539889+00:00",
    pub created_at: Box<str>, // "2018-01-29T23:10:11.539889+00:00",
    pub downloads: usize,   // 154,
    pub features: HashMap<SmolStr, Vec<SmolStr>>,
    pub yanked: bool,            // false,
    pub license: Option<SmolStr>, // "MIT",
    #[serde(default)]
    pub audit_actions: Vec<AuditAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateMetaUser {
    pub id: usize, // crates-io ID, not GitHub
    pub login: SmolStr,
    pub name: Option<SmolStr>,
    pub avatar: Option<Box<str>>, // leaks GitHub ID
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditAction {
    pub action: String,
    pub user: CrateMetaUser,
    pub time: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateMetaKeyword {
    pub id: SmolStr,         // "cargo",
    pub keyword: SmolStr,    // "cargo",
    pub created_at: Box<str>, // "2014-11-28T19:06:33.883165+00:00",
    pub crates_cnt: usize,  // 92
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateMetaCategory {
    pub id: SmolStr,          // "development-tools::build-utils",
    pub category: SmolStr,    // "Development tools::Build Utils",
    pub slug: SmolStr,        // "development-tools::build-utils",
    pub description: Box<str>, // "Utilities for build scripts and other build time steps.",
    pub created_at: Box<str>,  // "2017-05-13T17:18:45.578208+00:00",
    pub crates_cnt: usize,   // 19
}
