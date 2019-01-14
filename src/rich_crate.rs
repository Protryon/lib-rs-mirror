use crates_io_client::CrateDownloadsFile;
use crates_io_client::CrateOwner;
use crates_io_client::CrateMetaVersion;
use crates_io_client::CratesIoCrate;
pub use crates_io_client::DownloadWeek;
use crate::Origin;

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
    versions: Vec<CrateMetaVersion>,
    downloads: CrateDownloadsFile,
    downloads_recent: usize,
    downloads_total: usize,
}

impl RichCrate {
    pub fn new(crates_io: CratesIoCrate) -> Self {
        Self {
            origin: Origin::from_crates_io_name(&crates_io.meta.krate.name),
            versions: crates_io.meta.versions().collect(),
            downloads: crates_io.downloads,
            name: crates_io.meta.krate.name,
            owners: crates_io.owners,
            downloads_recent: crates_io.meta.krate.recent_downloads.unwrap_or(0),
            downloads_total: crates_io.meta.krate.downloads,
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

    pub fn weekly_downloads(&self) -> Vec<DownloadWeek> {
        self.downloads.weekly_downloads()
    }

    pub fn versions(&self) -> impl Iterator<Item = &CrateMetaVersion> {
        self.versions.iter()
    }

    pub fn downloads_total(&self) -> usize {
        self.downloads_total
    }

    /// Per 90 days
    pub fn downloads_recent(&self) -> usize {
        self.downloads_recent
    }

    pub fn downloads_per_month(&self) -> usize {
        self.downloads_recent() / 3
    }
}
