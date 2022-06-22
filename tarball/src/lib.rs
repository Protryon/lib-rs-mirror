use cargo_toml::Manifest;
use cargo_toml::OptionalFile;
use cargo_toml::Package;
use libflate::gzip::Decoder;
use render_readme::Markup;
use std::collections::HashSet;
use std::io;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use tar::{Archive, Entry, EntryType};
use log::debug;
use udedokei::LanguageExt;

#[derive(Debug, thiserror::Error)]
pub enum UnarchiverError {
    #[error("Cargo.toml not found. Got files: {0}")]
    TomlNotFound(String),
    #[error("I/O error during unarchiving")]
    Io(#[from] #[source] io::Error),
    #[error("Cargo.toml parsing error")]
    Toml(#[from] #[source] cargo_toml::Error),
    #[error("Git checkout failure")]
    Checkout(#[from] #[source] crate_git_checkout::Error),
}

fn read_archive_files<R: Read>(archive: R, mut cb: impl FnMut(Entry<'_, Decoder<R>>) -> Result<(), UnarchiverError>) -> Result<(), UnarchiverError> {
    let mut archive = Archive::new(Decoder::new(archive)?);
    let entries = archive.entries()?;
    for entry in entries {
        cb(entry?)?
    }
    Ok(())
}

#[derive(Eq, PartialEq)]
enum ReadAs {
    Toml,
    ReadmeMarkdown(String),
    ReadmeRst(String),
    Lib,
    Bin,
    GetStatsOfFile(udedokei::Language),
    Skip,
}

const MAX_FILE_SIZE: u64 = 50_000_000;

pub fn read_repo(repo: &crate_git_checkout::Repository, path_in_tree: crate_git_checkout::Oid) -> Result<CrateFile, UnarchiverError> {
    let mut collect = Collector::new(0);
    crate_git_checkout::iter_blobs::<UnarchiverError, _>(repo, Some(path_in_tree), |path, _, name, blob| {
        // FIXME: skip directories that contain other crates
        let mut blob_content = blob.content();
        collect.add(Path::new(path).join(name), blob_content.len() as u64, &mut blob_content)?;
        Ok(())
    })?;
    Ok(collect.finish()?)
}

pub fn read_archive(archive: &[u8], name: &str, ver: &str) -> Result<CrateFile, UnarchiverError> {
    let prefix = PathBuf::from(format!("{}-{}", name, ver));
    let mut collect = Collector::new(archive.len());
    read_archive_files(archive, |mut file| {
        let header = file.header();
        match header.entry_type() {
            EntryType::Regular | EntryType::Char => {
                let path = header.path()?;
                if let Ok(relpath) = path.strip_prefix(&prefix) {
                    return collect.add(relpath.to_path_buf(), header.size()?, &mut file);
                }
            },
            _ => {},
        }
        Ok(())
    })?;
    collect.finish()
}

struct Collector {
    manifest: Option<Manifest>,
    markup: Option<(String, Markup)>,
    files: Vec<PathBuf>,
    lib_file: Option<String>,
    bin_file: Option<String>,
    stats: udedokei::Collect,
    decompressed_size: usize,
    compressed_size: usize,
    is_nightly: bool,
}

impl Collector {
    pub fn new(compressed_size: usize) -> Self {
        Self {
            manifest: None,
            markup: None,
            files: Vec::new(),
            lib_file: None,
            bin_file: None,
            stats: udedokei::Collect::new(),
            decompressed_size: 0,
            compressed_size,
            is_nightly: false,
        }
    }

    pub fn add(&mut self, relpath: PathBuf, size: u64, file_data: &mut dyn Read) -> Result<(), UnarchiverError> {
        let path_match = {
            match &relpath {
                p if p == Path::new("Cargo.toml") || p == Path::new("cargo.toml") => ReadAs::Toml,
                p if is_lib_filename(p, self.manifest.as_ref()) => ReadAs::Lib,
                p if is_bin_filename(p, self.manifest.as_ref()) => ReadAs::Bin,
                p if is_readme_filename(p, self.manifest.as_ref().and_then(|m| m.package.as_ref())) => {
                    let path_prefix = p.parent().unwrap().display().to_string();
                    if p.extension().map_or(false, |e| e == "rst") {
                        ReadAs::ReadmeRst(path_prefix)
                    } else {
                        ReadAs::ReadmeMarkdown(path_prefix)
                    }
                },
                p => {
                    if let Some(lang) = is_source_code_file(p) {
                        if lang.is_code() {
                            ReadAs::GetStatsOfFile(lang)
                        } else {
                            ReadAs::Skip
                        }
                    } else {
                        ReadAs::Skip
                    }
                },
            }
        };

        self.files.push(relpath);
        if path_match == ReadAs::Skip {
            return Ok(());
        }

        let mut data = Vec::with_capacity(size.min(MAX_FILE_SIZE) as usize);
        file_data.take(MAX_FILE_SIZE).read_to_end(&mut data)?;
        self.decompressed_size += data.len();

        let data = String::from_utf8(data).unwrap_or_else(|e| String::from_utf8_lossy(&e.into_bytes()).into_owned());
        if data.trim_start().is_empty() {
            debug!("Skipping empty tarball entry file");
            return Ok(());
        }

        match path_match {
            ReadAs::Lib => {
                self.stats.add_to_stats(udedokei::from_path("lib.rs").unwrap(), &data);
                if check_if_uses_nightly_features(&data) {
                    self.is_nightly = true;
                }
                self.lib_file = Some(data);
            },
            ReadAs::Bin => {
                self.stats.add_to_stats(udedokei::from_path("main.rs").unwrap(), &data);
                if check_if_uses_nightly_features(&data) {
                    self.is_nightly = true;
                }
                self.bin_file = Some(data);
            },
            ReadAs::Toml => {
                self.manifest = Some(Manifest::from_str(&data)?);
            },
            ReadAs::ReadmeMarkdown(path_prefix) => {
                self.markup = Some((path_prefix, Markup::Markdown(data)));
            },
            ReadAs::ReadmeRst(path_prefix) => {
                self.markup = Some((path_prefix, Markup::Rst(data)));
            },
            ReadAs::GetStatsOfFile(lang) => {
                self.stats.add_to_stats(lang, &data);
            },
            ReadAs::Skip => unreachable!(),
        }
        Ok(())
    }

    fn finish(self) -> Result<CrateFile, UnarchiverError> {
        let mut manifest = match self.manifest {
            Some(m) => m,
            None => return Err(UnarchiverError::TomlNotFound(self.files.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", "))),
        };

        manifest.complete_from_abstract_filesystem(FilesFs(&self.files))?;

        Ok(CrateFile {
            decompressed_size: self.decompressed_size,
            compressed_size: self.compressed_size,
            readme: self.markup,
            manifest,
            files: self.files,
            lib_file: self.lib_file,
            bin_file: self.bin_file,
            language_stats: self.stats.finish(),
            is_nightly: self.is_nightly,
        })
    }
}

struct FilesFs<'a>(&'a [PathBuf]);

impl cargo_toml::AbstractFilesystem for FilesFs<'_> {
    fn file_names_in(&self, dir: &str) -> io::Result<HashSet<Box<str>>> {
        Ok(self.0.iter().filter_map(|p| {
            p.strip_prefix(dir).ok()
        })
        .filter_map(|p| p.to_str())
        .map(From::from)
        .collect())
    }
}

fn check_if_uses_nightly_features(lib_source: &str) -> bool {
    lib_source.lines()
        .take(1000)
        .map(|line| line.find('"').map(|pos| &line[0..pos]).unwrap_or(line)) // half-assed effort to ignore feature in strings
        .map(|line| line.find("//").map(|pos| &line[0..pos]).unwrap_or(line)) // half-assed effort to remove comments
        .any(|line| line.contains("#![feature("))
}

fn is_source_code_file(path: &Path) -> Option<udedokei::Language> {
    use std::os::unix::ffi::OsStrExt;

    if path.starts_with("tests") || path.starts_with("benches") || path.starts_with("docs") || path.starts_with("examples") {
        return None;
    }
    if let Some(name) = path.file_name() {
        if name.as_bytes().starts_with(b".") {
            return None;
        }
    } else {
        return None;
    }
    udedokei::from_path(path)
}

#[derive(Debug, Clone)]
pub struct CrateFile {
    pub manifest: Manifest,
    pub lib_file: Option<String>,
    pub bin_file: Option<String>,
    pub files: Vec<PathBuf>,
    // relative path and markdown
    pub readme: Option<(String, Markup)>,
    pub language_stats: udedokei::Stats,
    pub compressed_size: usize,
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
}

fn is_lib_filename(path: &Path, manifest: Option<&Manifest>) -> bool {
    let expected_path = if let Some(lib_path) = manifest.and_then(|m| m.lib.as_ref()).and_then(|l| l.path.as_ref()) {
        Path::new(lib_path)
    } else {
        Path::new("src/lib.rs")
    };
    path == expected_path

}

fn is_bin_filename(path: &Path, manifest: Option<&Manifest>) -> bool {
    if let Some(bins) = manifest.map(|m| &m.bin) {
        if bins.iter().filter_map(|p| p.path.as_ref()).any(|bin_path| Path::new(bin_path) == path) {
            return true;
        }
    }
    path == Path::new("src/main.rs")
}

/// Check if given filename is a README. If `package` is missing, guess.
fn is_readme_filename(path: &Path, package: Option<&Package>) -> bool {
    path.to_str().map_or(false, |pathstr| {
        if let Some(&Package { readme: OptionalFile::Path(ref r), .. }) = package {
            // packages put ./README which doesn't match README
            r.trim_start_matches('.').trim_start_matches('/') == pathstr
        } else {
            render_readme::is_readme_filename(path)
        }
    })
}

#[test]
fn unpack_crate() {
    let k = include_bytes!("../test.crate");
    let d = read_archive(&k[..], "testing", "1.0.0").unwrap();
    assert_eq!(d.manifest.package.as_ref().unwrap().name, "crates-server");
    assert_eq!(d.manifest.package.as_ref().unwrap().version, "0.5.1");
    assert!(d.lib_file.unwrap().contains("fn nothing"));
    assert_eq!(d.files.len(), 5);
    assert!(match d.readme.unwrap().1 {
        Markup::Rst(a) => a == "o hi\n",
        _ => false,
    });
    assert_eq!(d.language_stats.langs.get(&udedokei::Language::Rust).unwrap().code, 1);
    assert_eq!(d.language_stats.langs.get(&udedokei::Language::C).unwrap().code, 1);
    assert_eq!(d.language_stats.langs.get(&udedokei::Language::JavaScript).unwrap().code, 0);
    assert!(d.language_stats.langs.get(&udedokei::Language::Bash).is_none());
    assert_eq!(d.decompressed_size, 161);
}

#[test]
fn unpack_repo() {
    use repo_url::Repo;
    let test_repo_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("test.repo");
    let repo = Repo::new("http://example.invalid/foo.git").unwrap();
    let checkout = crate_git_checkout::checkout(&repo, &test_repo_path).unwrap();
    let f = crate_git_checkout::path_in_repo(&checkout, "crates-server").unwrap().unwrap();
    let tree_id = f.tree;
    let manifest = f.manifest;

    let d = read_repo(&checkout, tree_id).unwrap();
    assert_eq!(d.manifest.package, manifest.package);

    assert_eq!(d.manifest.package.as_ref().unwrap().name, "crates-server");
    assert_eq!(d.manifest.package.as_ref().unwrap().version, "0.5.1");
    assert!(d.lib_file.unwrap().contains("fn nothing"));
    assert_eq!(d.files.len(), 5);
    assert!(match d.readme.unwrap().1 {
        Markup::Rst(a) => a == "o hi\n",
        _ => false,
    });
    assert_eq!(d.language_stats.langs.get(&udedokei::Language::Rust).unwrap().code, 1);
    assert_eq!(d.language_stats.langs.get(&udedokei::Language::C).unwrap().code, 1);
    assert_eq!(d.language_stats.langs.get(&udedokei::Language::JavaScript).unwrap().code, 0);
    assert!(d.language_stats.langs.get(&udedokei::Language::Bash).is_none());
    assert_eq!(d.decompressed_size, 161);
}
