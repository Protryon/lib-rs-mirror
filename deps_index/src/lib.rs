mod index;
use failure::Fail;
pub use index::*;
use rich_crate::Origin;
use std::path::PathBuf;

mod deps_stats;
mod git_crates_index;
pub use crates_index::Crate as CratesIndexCrate;
pub use crates_index::Version as CratesIndexVersion;
pub use deps_stats::*;

#[derive(Debug, Clone, Fail)]
pub enum DepsErr {
    #[fail(display = "crate not found: {:?}", _0)]
    CrateNotFound(Origin),
    #[fail(display = "crate {} not found in repo {}", _0, _1)]
    CrateNotFoundInRepo(String, String),
    #[fail(display = "crate is not a package: {:?}", _0)]
    NotAPackage(Origin),

    #[fail(display = "Error when parsing verison")]
    SemverParsingError,
    #[fail(display = "Stopped")]
    Stopped,
    #[fail(display = "Deps stats timeout")]
    DepsNotAvailable,
    #[fail(display = "Index is empty or parsing failed")]
    IndexBroken,
    #[fail(display = "Crate timeout")]
    GitIndexFile(PathBuf, String),
    #[fail(display = "Git crate '{:?}' can't be indexed, because it's not on the list", _0)]
    GitCrateNotAllowed(Origin),
}
