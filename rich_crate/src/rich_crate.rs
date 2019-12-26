use crate::Origin;
pub use crates_io_client::CrateOwner;

/// Struct representing all versions of the crate
/// (metadata that is version-independent or for all versions).
///
/// Currently just a wrapper around crates.io API data.
#[derive(Debug)]
pub struct RichCrate {
    origin: Origin,
    // crates_io: CratesIoCrate,
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
        Self {
            origin,
            versions,
            name,
            owners,
        }
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
}
