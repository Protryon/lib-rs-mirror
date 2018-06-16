
#[derive(Debug, Copy, Eq, PartialEq, Clone)]
pub enum UserType {
    Org,
    User,
}

use serde::de;
use serde::de::{Deserializer, Visitor};
use serde::Deserialize;
use std::fmt;

/// Case-insensitive enum
impl<'de> Deserialize<'de> for UserType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>,
    {
        struct UserTypeVisitor;

        impl<'a> Visitor<'a> for UserTypeVisitor {
            type Value = UserType;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("user or org")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                match v.to_lowercase().as_str() {
                    "org" | "organization" => Ok(UserType::Org),
                    "user" => Ok(UserType::User),
                    x => Err(de::Error::unknown_variant(x, &["user", "org"])),
                }
            }

            fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
                self.visit_str(&v)
            }
        }

        deserializer.deserialize_string(UserTypeVisitor)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct User {
    pub id: u32,
    pub login: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>, // "https://avatars0.githubusercontent.com/u/1111?v=4",
    pub gravatar_id: Option<String>, // "",
    pub html_url: String, // "https://github.com/zzzz",
    pub blog: Option<String>, // "https://example.com
    #[serde(rename="type")]
    pub user_type: UserType,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContribWeek {
    #[serde(rename="w")]
    pub week_timestamp: u32,
    #[serde(rename="a")]
    pub added: u32,
    #[serde(rename="d")]
    pub deleted: u32,
    #[serde(rename="c")]
    pub commits: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchResults<T> {
    pub items: Vec<T>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserContrib {
    pub total: usize,
    pub weeks: Vec<ContribWeek>,
    pub author: User,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitCommitAuthor {
    pub date: String, // "2018-04-30T16:24:52Z",
    pub email: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitCommit {
    pub author: GitCommitAuthor,
    pub committer: GitCommitAuthor,
    pub message: String,
    pub comment_count: u32,
    // tree.sha
}

#[derive(Debug, Clone, Deserialize)]
pub struct CommitMeta {
    pub sha: String, // TODO: deserialize to bin
    pub author: User,
    pub committer: User,
    pub commit: GitCommit,
    // parents: [{sha}]
}
