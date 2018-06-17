extern crate cargo_toml;
extern crate libflate;
extern crate render_readme;
extern crate repo_url;
extern crate tar;
extern crate toml;
#[macro_use] extern crate quick_error;

use cargo_toml::TomlManifest;
use cargo_toml::TomlPackage;
use render_readme::Markup;
use render_readme::Readme;
use repo_url::Repo;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;

mod error;
mod tarball;
mod git;

pub use error::*;
pub type Result<T> = std::result::Result<T, UnarchiverError>;

pub struct Unarchiver {
    base_path: PathBuf,
}

impl Unarchiver {
    /// Temporary files will be stored in `cache_base_dir`
    pub fn new(cache_base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_path: cache_base_dir.into(),
        }
    }

    /// Read tarball and get Cargo.toml, etc. out of it.
    pub fn read_archive(&self, archive_tgz: impl Read, name: &str, ver: &str) -> Result<CrateFile> {
        let prefix = format!("{}-{}", name, ver);
        let mut meta = tarball::read_archive(archive_tgz, Path::new(&prefix))?;

        let has_readme = meta.readme.as_ref().ok().and_then(|opt| opt.as_ref()).is_some();
        if !has_readme {
            let maybe_repo = meta.manifest.package.repository.as_ref().and_then(|r| Repo::new(r).ok());
            if let Some(ref repo) = maybe_repo {
                meta.readme = git::checkout(repo, &self.base_path, name)
                .and_then(|checkout| {
                    git::find_readme(&checkout, &meta.manifest.package)
                });
            }
        }

        Ok(meta)
    }
}


#[derive(Debug, Clone)]
pub struct CrateFile {
    pub manifest: TomlManifest,
    pub lib_file: Option<String>,
    pub files: Vec<PathBuf>,
    pub readme: Result<Option<Readme>>,
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


fn readme_from_repo(markup: Markup, repo_url: &Option<String>) -> Readme {
    let repo = repo_url.as_ref().and_then(|url| Repo::new(url).ok());
    let base_url = repo.as_ref().map(|r| r.readme_base_url());
    let base_image_url = repo.map(|r| r.readme_base_image_url());

    Readme::new(markup, base_url, base_image_url)
}

/// Check if given filename is a README. If `package` is missing, guess.
fn is_readme_filename(path: &Path, package: Option<&TomlPackage>) -> bool {
    path.to_str().map_or(false, |s| {
        if let Some(&TomlPackage{readme: Some(ref r),..}) = package {
            r == s
        } else {
            // that's not great; there are readme-windows, readme.ja.md and more
            let readme_filenames = &["readme.md", "readme.markdown", "readme.mdown", "readme", "readme.rst", "readme.adoc", "readme.txt", "readme.rest"];
            readme_filenames.iter().any(|f| f.eq_ignore_ascii_case(s.as_ref()))
        }
    })
}
