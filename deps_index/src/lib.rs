mod index;
pub use index::*;
use rich_crate::Origin;
use std::path::PathBuf;

mod deps_stats;
mod git_crates_index;
pub use crates_index::Crate as CratesIndexCrate;
pub use crates_index::Version as CratesIndexVersion;
pub use deps_stats::*;

#[derive(Debug, Clone, thiserror::Error)]
pub enum DepsErr {
    #[error("crate not found: {0:?}")]
    CrateNotFound(Origin),
    #[error("crate {0} not found in repo {1}")]
    CrateNotFoundInRepo(String, String),
    #[error("crate is not a package: {0:?}")]
    NotAPackage(Origin),

    #[error("Error when parsing verison")]
    SemverParsingError,
    #[error("Stopped")]
    Stopped,
    #[error("Deps stats timeout")]
    DepsNotAvailable,
    #[error("Index is empty or parsing failed")]
    IndexBroken,
    #[error("Error in git index file {0:?}: {1}")]
    GitIndexFile(PathBuf, String),
    #[error("Git crate {0:?} can't be indexed, because it's not on the list")]
    GitCrateNotAllowed(Origin),
}
