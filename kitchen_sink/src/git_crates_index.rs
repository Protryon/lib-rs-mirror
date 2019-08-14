use crate::KitchenSinkErr;
use crate::Origin;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::path::PathBuf;

type FxHashSet<V> = std::collections::HashSet<V, ahash::ABuildHasher>;

pub struct GitIndex {
    #[allow(unused)]
    path: PathBuf,
    index: FxHashSet<Origin>,
}

impl GitIndex {
    pub fn new(dir: &Path) -> Result<Self, KitchenSinkErr> {
        let path = dir.join("git_crates.json");
        let crates: Vec<String> = if path.exists() {
            match File::open(&path) {
                Ok(file) => serde_json::from_reader(BufReader::new(file)).map_err(|e| KitchenSinkErr::GitIndexParse(e.to_string()))?,
                Err(e) => return Err(KitchenSinkErr::GitIndexFile(path, e.to_string())),
            }
        } else {
            Vec::new()
        };
        Ok(Self {
            path,
            index: crates.into_iter().map(Origin::from_str).collect()
        })
    }

    pub fn has(&self, origin: &Origin) -> bool {
        self.index.get(origin).is_some()
    }

    pub fn crates(&self) -> impl Iterator<Item=&Origin> {
        self.index.iter()
    }
}
