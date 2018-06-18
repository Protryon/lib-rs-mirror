
use crates_io_client::CratesIoCrate;
use crates_io_client::CrateMetaVersion;
pub use crates_io_client::DownloadWeek;

/// Struct representing all versions of the crate
/// (metadata that is version-independent or for all versions).
///
/// Currently just a wrapper around crates.io API data.
#[derive(Debug)]
pub struct RichCrate {
    crates_io: CratesIoCrate,
}

impl RichCrate {
    pub fn new(crates_io: CratesIoCrate) -> Self {
        Self {
            crates_io,
        }
    }

    pub fn name(&self) -> &str {
        &self.crates_io.meta.krate.name
    }

    pub fn weekly_downloads(&self) -> Vec<DownloadWeek> {
        self.crates_io.downloads.weekly_downloads()
    }

    pub fn versions<'a>(&'a self) -> impl Iterator<Item=&'a CrateMetaVersion> {
        self.crates_io.meta.versions()
    }

    pub fn downloads_total(&self) -> usize {
        self.crates_io.meta.krate.downloads
    }

    pub fn downloads_recent(&self) -> usize {
        self.crates_io.meta.krate.recent_downloads.unwrap_or(0)
    }
}
