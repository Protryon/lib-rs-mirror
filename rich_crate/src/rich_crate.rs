use crate::Origin;
pub use crates_io_client::CrateOwner;

/// Struct representing all versions of the crate
/// (metadata that is version-independent or for all versions).
///
/// Currently just a wrapper around crates.io API data.
#[derive(Debug)]
pub struct RichCrate {
    origin: Origin,
    name: String,
    owners: Vec<CrateOwner>,
    versions: Vec<CrateVersion>,
}

#[derive(Debug, Clone)]
pub struct CrateVersion {
    pub num: String, // "1.4.0",
    pub updated_at: String, // "2018-01-29T23:10:11.539889+00:00",
    pub created_at: String, // "2018-01-29T23:10:11.539889+00:00",
    // pub downloads: usize,   // 154,
    // pub features: HashMap<String, Vec<String>>,
    pub yanked: bool,
    // pub license: Option<String>, // "MIT",
}

impl RichCrate {
    pub fn new(origin: Origin, owners: Vec<CrateOwner>, name: String, versions: Vec<CrateVersion>) -> Self {
        Self { origin, versions, name, owners }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    pub fn owners(&self) -> &[CrateOwner] {
        &self.owners
    }

    pub fn versions(&self) -> &[CrateVersion] {
        &self.versions
    }

    pub fn most_recent_release_date_str(&self) -> &str {
        self.versions.iter().map(|v| v.created_at.as_str()).max().unwrap()
    }

    pub fn is_yanked(&self) -> bool {
        self.versions.iter().all(|v| v.yanked)
    }
}

/// Rev dependencies added/removed given month
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DependerChangesMonthly {
    pub year: u16,
    pub month0: u16,
    pub added: u32,
    /// Actively removed
    pub removed: u32,
    /// Just stopped counting as active crate
    pub expired: u32,

    pub added_total: u32,
    pub removed_total: u32,
    pub expired_total: u32,
}

impl DependerChangesMonthly {
    pub fn running_total(&self) -> u32 {
        self.added_total - self.removed_total - self.expired_total
    }
}
