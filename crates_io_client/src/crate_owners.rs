use chrono::FixedOffset;
use chrono::DateTime;
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
    #[serde(rename = "login")]
    pub crates_io_login: String,          // "github:rust-bus:maintainers",
    pub kind: OwnerKind,        // "team" || "user"
    pub url: Option<String>,    // "https://github.com/rust-bus",
    pub name: Option<String>,   // "maintainers",
    pub avatar: Option<String>, // "https://avatars1.githubusercontent.com/u/38887296?v=4"

    #[serde(default)]
    pub github_id: Option<u32>,

    #[serde(default)]
    pub invited_at: Option<DateTime<FixedOffset>>,

    #[serde(default)]
    pub invited_by_github_id: Option<u32>,

    #[serde(default)]
    pub last_seen_at: Option<DateTime<FixedOffset>>,

    /// not from the API, added later
    #[serde(default)]
    pub contributor_only: bool,
}

impl CrateOwner {
    pub fn name(&self) -> &str {
        match self.kind {
            OwnerKind::User => match &self.name.as_ref() {
                Some(name) if !name.trim_start().is_empty() => name,
                _ => &self.crates_io_login,
            },
            // teams get crappy names
            OwnerKind::Team => {
                self.crates_io_login.trim_start_matches("github:")
            },
        }
    }

    pub fn invited_at(&self) -> Option<DateTime<Utc>> {
        Some(self.invited_at?.with_timezone(&Utc))
    }

    /// Be careful about case-insensitivity
    pub fn github_login(&self) -> Option<&str> {
        match self.kind {
            OwnerKind::User => Some(&self.crates_io_login),
            OwnerKind::Team => {
                let mut w = self.crates_io_login.split(':');
                match w.next().expect("team parse") {
                    "github" => w.next(),
                    _ => None,
                }
            },
        }
    }
}
