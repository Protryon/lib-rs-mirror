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
    pub id: usize,              // 362,
    pub login: String,          // "github:rust-bus:maintainers",
    pub kind: OwnerKind,        // "team" || "user"
    pub url: String,            // "https://github.com/rust-bus",
    pub name: Option<String>,   // "maintainers",
    pub avatar: Option<String>, // "https://avatars1.githubusercontent.com/u/38887296?v=4"
}

impl CrateOwner {
    pub fn name(&self) -> &str {
        match self.kind {
            OwnerKind::User => self.name.as_ref().unwrap_or(&self.login),
            // teams get crappy names
            OwnerKind::Team => {
                if self.url.starts_with("https://github.com/") {
                    &self.url["https://github.com/".len()..]
                } else {
                    &self.login
                }
            },
        }
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
