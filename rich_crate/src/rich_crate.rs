use crate::Origin;
pub use crates_io_client::CrateOwner;
use smartstring::alias::String as SmolStr;
use chrono::{DateTime, Utc};

/// Struct representing all versions of the crate
/// (metadata that is version-independent or for all versions).
///
/// Currently just a wrapper around crates.io API data.
#[derive(Debug)]
pub struct RichCrate {
    origin: Origin,
    name: SmolStr,
    versions: Vec<CrateVersion>,
}

#[derive(Debug, Clone)]
pub struct CrateVersion {
    pub num: SmolStr, // "1.4.0",
    pub updated_at: DateTime<Utc>, // "2018-01-29T23:10:11.539889+00:00",
    pub created_at: DateTime<Utc>, // "2018-01-29T23:10:11.539889+00:00",
    // pub downloads: usize,   // 154,
    // pub features: HashMap<String, Vec<String>>,
    pub yanked: bool,
    // pub license: Option<String>, // "MIT",
}

impl RichCrate {
    pub fn new(origin: Origin, name: SmolStr, versions: Vec<CrateVersion>) -> Self {
        Self { origin, versions, name }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    pub fn versions(&self) -> &[CrateVersion] {
        &self.versions
    }

    pub fn most_recent_release(&self) -> DateTime<Utc> {
        *self.versions.iter().map(|v| &v.created_at).max().unwrap()
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
    pub users_total: u16,
}

impl DependerChangesMonthly {
    pub fn running_total(&self) -> u32 {
        self.added_total - self.removed_total - self.expired_total
    }
}


#[derive(Debug, Copy, Clone)]
pub struct TractionStats {
    /// 1.0 - still at its peak
    /// < 1 - heading into obsolescence
    /// < 0.3 - dying
    ///
    pub former_glory: f64,

    /// 1.0 = real traction, 0 = internal crate
    pub external_usage: f64,

    /// current usage / previous quarter usage
    pub growth: f64,

    pub active_users: u16,
}
