use cargo_toml;
use render_readme;
use udedokei;
#[macro_use]
extern crate quick_error;
use cargo_toml::{Manifest, Package};
use render_readme::{Markup, Readme};
use repo_url::Repo;
use std::io::Read;
use std::path::{Path, PathBuf};

mod error;
mod tarball;

pub use crate::error::*;
pub type Result<T> = std::result::Result<T, UnarchiverError>;

/// Read tarball and get Cargo.toml, etc. out of it.
pub fn read_archive(archive_tgz: impl Read, name: &str, ver: &str) -> Result<CrateFile> {
    let prefix = format!("{}-{}", name, ver);
    Ok(tarball::read_archive(archive_tgz, Path::new(&prefix))?)
}

#[derive(Debug, Clone)]
pub struct CrateFile {
    pub manifest: Manifest,
    pub lib_file: Option<String>,
    pub files: Vec<PathBuf>,
    pub readme: Result<Option<Readme>>,
    pub language_stats: udedokei::Stats,
    pub decompressed_size: usize,
    pub is_nightly: bool,
}

impl CrateFile {
    /// Checks whether tarball contained given file path,
    /// relative to project root.
    pub fn has(&self, path: impl AsRef<Path>) -> bool {
        let path = path.as_ref();
        self.files.iter().any(|p| p == path)
    }

    /// Find path that matches according to the callback
    pub fn find(&self, mut f: impl FnMut(&Path) -> bool) -> Option<&Path> {
        self.files.iter().map(|p| p.as_path()).find(|p| f(p))
    }
}

fn readme_from_repo(markup: Markup, repo_url: Option<&String>, base_path: &str) -> Readme {
    let repo = repo_url.and_then(|url| Repo::new(url).ok());
    let base_url = repo.as_ref().map(|r| r.readme_base_url(base_path));
    let base_image_url = repo.map(|r| r.readme_base_image_url(base_path));

    Readme::new(markup, base_url, base_image_url)
}

/// Check if given filename is a README. If `package` is missing, guess.
fn is_readme_filename(path: &Path, package: Option<&Package>) -> bool {
    path.to_str().map_or(false, |pathstr| {
        if let Some(&Package { readme: Some(ref r), .. }) = package {
            // packages put ./README which doesn't match README
            r.trim_start_matches('.').trim_start_matches('/') == pathstr
        } else {
            render_readme::is_readme_filename(path)
        }
    })
}
