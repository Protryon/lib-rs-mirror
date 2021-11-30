use serde_derive::*;
use std::collections::HashMap;

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
    pub badge_type: String,
    pub attributes: CrateMetaBadgeAttr,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateMetaBadgeAttr {
    pub repository: Option<String>,
    pub branch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateMeta {
    pub id: String,         // "cargo-deb",
    pub name: String,       // "cargo-deb",
    pub updated_at: String, // "2018-04-26T00:57:41.867975+00:00",
    #[serde(default)]
    pub versions: Vec<usize>, // [90309, 89288, 87534, 86387, 82743, 82697, 81712, 81233, 79188, 76393, 69921, 65169, 64103, 62665, 62074, 61494, 61440, 61393, 61273, 61237, 61236],
    #[serde(default)]
    pub keywords: Vec<String>, // ["cargo", "subcommand", "debian", "package", "deb"],
    #[serde(default)]
    pub categories: Vec<String>, // ["development-tools::build-utils", "command-line-utilities", "development-tools::cargo-plugins"],
    #[serde(default)]
    pub badges: Vec<CrateMetaBadge>,
    pub created_at: String, // "2017-07-31T23:46:03.490855+00:00",
    // #[serde(default)]
    // pub downloads: usize, // 3393, // this meta is updated once per release, not frequently enough to keep downloads relevant
    #[serde(default)]
    pub recent_downloads: Option<usize>, // 1314,
    pub max_version: String, // "1.10.0",
    pub description: Option<String>, // "Make Debian packages (.deb) easily with a Cargo subcommand",
    pub homepage: Option<String>, // "https://github.com/mmstick/cargo-deb#readme",
    pub documentation: Option<String>, // "https://docs.rs/cargo-deb",
    pub repository: Option<String>, // "https://github.com/mmstick/cargo-deb",
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateMetaVersion {
    pub id: usize, // 79188,
    #[serde(rename = "crate")]
    pub krate: String, // "cargo-deb",
    pub num: String, // "1.4.0",
    // pub dl_path: String, // "/api/v1/crates/cargo-deb/1.4.0/download",
    // pub readme_path: String, // "/api/v1/crates/cargo-deb/1.4.0/readme",
    pub updated_at: String, // "2018-01-29T23:10:11.539889+00:00",
    pub created_at: String, // "2018-01-29T23:10:11.539889+00:00",
    pub downloads: usize,   // 154,
    pub features: HashMap<String, Vec<String>>,
    pub yanked: bool,            // false,
    pub license: Option<String>, // "MIT",
    #[serde(default)]
    pub audit_actions: Vec<AuditAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateMetaUser {
    pub id: usize, // crates-io ID, not GitHub
    pub login: String,
    pub name: Option<String>,
    pub avatar: Option<String>, // leaks GitHub ID
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditAction {
    pub action: String,
    pub user: CrateMetaUser,
    pub time: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateMetaKeyword {
    pub id: String,         // "cargo",
    pub keyword: String,    // "cargo",
    pub created_at: String, // "2014-11-28T19:06:33.883165+00:00",
    pub crates_cnt: usize,  // 92
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateMetaCategory {
    pub id: String,          // "development-tools::build-utils",
    pub category: String,    // "Development tools::Build Utils",
    pub slug: String,        // "development-tools::build-utils",
    pub description: String, // "Utilities for build scripts and other build time steps.",
    pub created_at: String,  // "2017-05-13T17:18:45.578208+00:00",
    pub crates_cnt: usize,   // 19
}
