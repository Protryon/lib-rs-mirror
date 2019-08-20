use crate::KitchenSinkErr;
use crate::Origin;
use std::fs;
use std::path::Path;

type FxHashSet<V> = std::collections::HashSet<V, ahash::ABuildHasher>;

pub struct GitIndex {
    index: FxHashSet<Origin>,
}

impl GitIndex {
    pub fn new(dir: &Path) -> Result<Self, KitchenSinkErr> {
        let path = dir.join("git_crates.txt");
        let index = if path.exists() {
            match fs::read_to_string(&path) {
                Ok(file) => file.split('\n').map(|s| s.trim()).filter(|s| !s.is_empty()).map(Origin::from_str).collect(),
                Err(e) => return Err(KitchenSinkErr::GitIndexFile(path, e.to_string())),
            }
        } else {
            Default::default()
        };
        Ok(Self {
            index,
        })
    }

    pub fn has(&self, origin: &Origin) -> bool {
        self.index.get(origin).is_some()
    }

    pub fn crates(&self) -> impl Iterator<Item=&Origin> {
        self.index.iter()
    }
}
