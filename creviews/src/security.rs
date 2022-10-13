pub use rustsec::{Advisory, Error, advisory::Severity};
use rustsec::database::Query;
use rustsec::repository::git::DEFAULT_URL;
use rustsec::{Database, Repository, Collection};
use std::path::{Path, PathBuf};

pub struct RustSec {
    path: PathBuf,
    db: Database,
}

impl RustSec {
    pub fn new(root: &Path) -> Result<Self, Error> {
        let path = root.join("rustsec");
        let db = Self::db_at_path(&path)?;
        Ok(Self { db, path })
    }

    pub fn update(&mut self) ->  Result<(), Error> {
        self.db = Self::db_at_path(&self.path)?;
        Ok(())
    }

    fn db_at_path(path: &Path) -> Result<Database, Error> {
        let r = Repository::fetch(DEFAULT_URL, path, true)?;
        Database::load_from_repo(&r)
    }

    pub fn advisories_for_crate(&self, crate_name: &str) -> Vec<&Advisory> {
        self.db.query(&Query::new()
            .collection(Collection::Crates)
            .package_source(Default::default())
            .package_name(crate_name.parse().unwrap())
        )
    }
}


#[test]
fn rustsec_test() {
    let path = Path::new("../data");
    assert!(path.exists());
    let d = RustSec::new(path).unwrap();
    let a = d.advisories_for_crate("rgb");
    assert_eq!(1, a.len());
}

#[test]
fn severity_sort() {
    assert!(Severity::High > Severity::Low);
}
