use chrono::DateTime;
use chrono::offset::TimeZone;
use chrono::Utc;
use serde_derive::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateOwnersFile {
    pub users: Vec<CrateOwner>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateTeamsFile {
    pub teams: Vec<CrateOwner>,
}

#[derive(Debug, Copy, Eq, PartialEq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OwnerKind {
    Team,
    User,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateOwner {
    pub login: String,          // "github:rust-bus:maintainers",
    pub kind: OwnerKind,        // "team" || "user"
    pub url: Option<String>,    // "https://github.com/rust-bus",
    pub name: Option<String>,   // "maintainers",
    pub avatar: Option<String>, // "https://avatars1.githubusercontent.com/u/38887296?v=4"

    #[serde(default)]
    pub github_id: Option<u32>,

    #[serde(default)]
    pub invited_at: Option<String>,

    #[serde(default)]
    pub invited_by_github_id: Option<u32>,
}

impl CrateOwner {
    pub fn name(&self) -> &str {
        match self.kind {
            OwnerKind::User => match &self.name.as_ref() {
                Some(name) if !name.trim_start().is_empty() => name,
                _ => &self.login,
            },
            // teams get crappy names
            OwnerKind::Team => {
                match &self.url {
                    Some(url) if url.starts_with("https://github.com/") => {
                        &url["https://github.com/".len()..]
                    },
                    _ => self.login.trim_start_matches("github:")
                }
            },
        }
    }

    pub fn invited_at(&self) -> Option<DateTime<Utc>> {
        self.invited_at.as_ref().and_then(|d| Utc.datetime_from_str(d, "%Y-%m-%d %H:%M:%S").ok())
    }

    /// Be careful about case-insensitivity
    pub fn github_login(&self) -> Option<&str> {
        match self.kind {
            OwnerKind::User => Some(&self.login),
            OwnerKind::Team => {
                let mut w = self.login.split(':');
                match w.next().expect("team parse") {
                    "github" => w.next(),
                    _ => None,
                }
            },
        }
    }
}
