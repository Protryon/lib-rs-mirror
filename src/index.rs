use std::collections::HashMap;
use lazyonce::LazyOnce;
use crates_index::Crate;
use rich_crate::Origin;
use std::path::PathBuf;
use crates_index;
use KitchenSinkErr;

pub struct Index {
    index: crates_index::Index,
    crate_path_index: LazyOnce<HashMap<Origin, PathBuf>>,
}

impl Index {
    pub fn new(path: PathBuf) -> Self {
        Self {
            index: crates_index::Index::new(path),
            crate_path_index: LazyOnce::new(),
        }
    }

    /// Iterator over all crates available in the index
    ///
    /// It returns only a thin and mostly useless data from the index itself,
    /// so `rich_crate`/`rich_crate_version` is needed to do more.
    pub fn crates(&self) -> crates_index::Crates {
        self.index.crates()
    }


    pub fn crate_by_name(&self, name: &Origin) -> Result<Crate, KitchenSinkErr> {
        self.crate_path_index.get(|| {
            self.index.crate_index_paths()
                .filter_map(|p| {
                    let f = p.file_name().and_then(|f| f.to_str()).map(|s| s.to_lowercase());
                    f.map(|f| (Origin::from_crates_io_name(&f), p))
                })
                .collect()
        })
        .get(name)
        .map(Crate::new)
        .ok_or_else(|| KitchenSinkErr::CrateNotFound(name.clone()))
    }

}
