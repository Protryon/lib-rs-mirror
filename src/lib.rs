extern crate cargo_toml;
extern crate libflate;
extern crate render_readme;
extern crate repo_url;
extern crate tar;
extern crate udedokei;

#[macro_use] extern crate quick_error;

use cargo_toml::TomlPackage;
use cargo_toml::TomlManifest;
use render_readme::Markup;
use render_readme::Readme;
use repo_url::Repo;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;

mod error;
mod tarball;

pub use error::*;
pub type Result<T> = std::result::Result<T, UnarchiverError>;

/// Read tarball and get Cargo.toml, etc. out of it.
pub fn read_archive(archive_tgz: impl Read, name: &str, ver: &str) -> Result<CrateFile> {
    let prefix = format!("{}-{}", name, ver);
    Ok(tarball::read_archive(archive_tgz, Path::new(&prefix))?)
}

#[derive(Debug, Clone)]
pub struct CrateFile {
    pub manifest: TomlManifest,
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

fn readme_from_repo(markup: Markup, repo_url: &Option<String>, base_path: &str) -> Readme {
    let repo = repo_url.as_ref().and_then(|url| Repo::new(url).ok());
    let base_url = repo.as_ref().map(|r| r.readme_base_url(base_path));
    let base_image_url = repo.map(|r| r.readme_base_image_url(base_path));

    Readme::new(markup, base_url, base_image_url)
}

/// Check if given filename is a README. If `package` is missing, guess.
fn is_readme_filename(path: &Path, package: Option<&TomlPackage>) -> bool {
    path.to_str().map_or(false, |s| {
        if let Some(&TomlPackage{readme: Some(ref r),..}) = package {
            r == s
        } else {
            render_readme::is_readme_filename(path)
        }
    })
}
