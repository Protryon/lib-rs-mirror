use smartstring::alias::String as SmolStr;

#[derive(Debug, Copy, Eq, PartialEq, Clone)]
pub enum UserType {
    Org,
    User,
    Bot,
}

use serde::de;
use serde::de::{Deserializer, Visitor};
use serde::Deserialize;
use serde::Serialize;
use serde::Serializer;
use std::fmt;

/// Case-insensitive enum
impl<'de> Deserialize<'de> for UserType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de> {
        struct UserTypeVisitor;

        impl<'a> Visitor<'a> for UserTypeVisitor {
            type Value = UserType;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("user/org/bot")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                match v.to_ascii_lowercase().as_str() {
                    "org" | "organization" => Ok(UserType::Org),
                    "user" => Ok(UserType::User),
                    "bot" => Ok(UserType::Bot),
                    x => Err(de::Error::unknown_variant(x, &["user", "org", "bot"])),
                }
            }

            fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
                self.visit_str(&v)
            }
        }

        deserializer.deserialize_string(UserTypeVisitor)
    }
}

impl Serialize for UserType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        serializer.serialize_str(match *self {
            UserType::User => "user",
            UserType::Org => "org",
            UserType::Bot => "bot",
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: u32,
    pub login: SmolStr,
    pub name: Option<SmolStr>,
    pub avatar_url: Option<Box<str>>,  // "https://avatars0.githubusercontent.com/u/1111?v=4",
    pub gravatar_id: Option<Box<str>>, // "",
    pub html_url: Box<str>,            // "https://github.com/zzzz",
    pub blog: Option<Box<str>>,        // "https://example.com
    pub two_factor_authentication: Option<bool>,
    #[serde(rename = "type")]
    pub user_type: UserType,
    pub created_at: Option<Box<str>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinimalUser {
    pub id: Option<u32>,
    pub login: SmolStr,
    pub name: Option<SmolStr>,
    pub avatar_url: Option<Box<str>>,  // "https://avatars0.githubusercontent.com/u/1111?v=4",
    pub gravatar_id: Option<Box<str>>, // "",
    pub html_url: Box<str>,            // "https://github.com/zzzz",
    #[serde(rename = "type")]
    pub user_type: UserType,
    pub created_at: Option<Box<str>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContribWeek {
    #[serde(rename = "w")]
    pub week_timestamp: u32,
    #[serde(rename = "a")]
    pub added_l: i32,
    #[serde(rename = "d")]
    pub deleted_l: i32,
    #[serde(rename = "c")]
    pub commits: i32, // It's sometimes -1! GitHub's bug?
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResults<T> {
    pub items: Vec<T>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserContrib {
    pub total: u32,
    pub weeks: Vec<ContribWeek>,
    pub author: Option<MinimalUser>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitCommitAuthor {
    pub date: String, // "2018-04-30T16:24:52Z",
    pub email: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitCommit {
    pub author: GitCommitAuthor,
    pub committer: GitCommitAuthor,
    pub message: String,
    pub comment_count: u32,
    // tree.sha
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitMeta {
    pub sha: String, // TODO: deserialize to bin
    pub author: Option<MinimalUser>,
    pub committer: Option<MinimalUser>,
    pub commit: GitCommit,
    // parents: [{sha}]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubRepo {
    pub name: String,
    pub description: Option<String>,
    pub fork: bool,
    pub created_at: String,
    pub updated_at: Option<String>,
    pub pushed_at: Option<String>,
    pub homepage: Option<String>,
    pub stargazers_count: u32,  // Stars
    pub forks_count: u32,       // Real number of forks
    pub subscribers_count: u32, // Real number of watches
    pub has_issues: bool,
    pub open_issues_count: Option<u32>,
    // language: JavaScript,
    pub has_downloads: bool,
    // has_wiki: true,
    pub has_pages: bool,
    pub archived: bool,
    pub default_branch: Option<String>,
    pub owner: Option<MinimalUser>,
    #[serde(default)]
    pub topics: Vec<String>,

    /// My custom addition!
    pub github_page_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubRelease {
    // url: Option<String>, // "https://api.github.com/repos/octocat/Hello-World/releases/1",
    // html_url: Option<String>, // "https://github.com/octocat/Hello-World/releases/v1.0.0",
    // assets_url: Option<String>, // "https://api.github.com/repos/octocat/Hello-World/releases/1/assets",
    // upload_url: Option<String>, // "https://uploads.github.com/repos/octocat/Hello-World/releases/1/assets{?name,label}",
    // tarball_url: Option<String>, // "https://api.github.com/repos/octocat/Hello-World/tarball/v1.0.0",
    // zipball_url: Option<String>, // "https://api.github.com/repos/octocat/Hello-World/zipball/v1.0.0",
    // id: Option<String>, // 1,
    // node_id: Option<String>, // "MDc6UmVsZWFzZTE=",
    pub tag_name: Option<SmolStr>, // "v1.0.0",
    // target_commitish: Option<String>, // "master",
    // name: Option<String>, // "v1.0.0",
    pub body: Option<Box<str>>,         // "Description of the release",
    pub draft: Option<bool>,          // false,
    pub prerelease: Option<bool>,     // false,
    pub created_at: Option<SmolStr>,   // "2013-02-27T19:35:32Z",
    pub published_at: Option<SmolStr>, // "2013-02-27T19:35:32Z",
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Topics {
    pub names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserOrg {
    pub login: String, // "github",
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Org {
    id: u32,
    pub login: String, // "github",
    pub name: Option<String>,
    pub blog: Option<String>,
    pub html_url: String,
    pub avatar_url: String,
    // public_members_url: String, // "https://api.github.com/orgs/github/public_members{/member}",
    // avatar_url: String, // "https://github.com/images/error/octocat_happy.gif",
    pub description: Option<String>, // "A great organization"
    pub created_at: String,
}
