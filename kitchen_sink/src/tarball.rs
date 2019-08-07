use cargo_toml::Manifest;
use cargo_toml::Package;
use crate_files::read_archive_files;
use render_readme::Markup;
use render_readme::Readme;
use repo_url::Repo;
use std::collections::HashSet;
use std::io::Read;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use udedokei::LanguageExt;
use udedokei;

enum ReadAs {
    Toml,
    ReadmeMarkdown(String),
    ReadmeRst(String),
    Lib,
    GetStatsOfFile(udedokei::Language),
}

struct FileEntry<'a> {
    relpath: PathBuf,
    size: usize,
    data: Box<dyn Read + 'a>,
}

const MAX_FILE_SIZE: u64 = 50_000_000;

pub fn read_archive(archive: impl Read, name: &str, ver: &str) -> crate_files::Result<CrateFile> {
    let prefix = PathBuf::from(format!("{}-{}", name, ver));
    read_archive_from(&mut read_archive_files(archive)?.into_iter().filter_map(|f| {
        f.map(|file| {
            let header = file.header();
            let path = header.path();
            let relpath = match path {
                Ok(ref p) => match p.strip_prefix(&prefix) {
                    Ok(relpath) => relpath,
                    _ => return None,
                },
                _ => return None,
            };
            Some(FileEntry {
                relpath: relpath.to_owned(),
                size: header.size().ok()? as usize,
                data: Box::new(file),
            })
        }).transpose()
    }))
}

fn read_archive_from<'a>(src: &mut dyn Iterator<Item=io::Result<FileEntry<'a>>>) -> crate_files::Result<CrateFile> {
    let mut manifest: Option<Manifest> = None;
    let mut markup = None;
    let mut files = Vec::new();
    let mut lib_file = None;
    let mut stats = udedokei::Collect::new();
    let mut decompressed_size = 0;
    let mut is_nightly = false;

    for file in src {
        let file = file?;

        let path_match = {

            files.push(file.relpath.clone());

            match file.relpath.as_path() {
                p if p == Path::new("Cargo.toml") || p == Path::new("cargo.toml") => ReadAs::Toml,
                p if p == Path::new("src/lib.rs") => ReadAs::Lib,
                p if is_readme_filename(p, manifest.as_ref().and_then(|m| m.package.as_ref())) => {
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
                            continue;
                        }
                    } else {
                        continue;
                    }
                },
            }
        };

        let mut data = Vec::with_capacity(file.size.min(MAX_FILE_SIZE as usize));
        file.data.take(MAX_FILE_SIZE).read_to_end(&mut data)?;
        decompressed_size += data.len();
        let data = String::from_utf8_lossy(&data);

        match path_match {
            ReadAs::Lib => {
                stats.add_to_stats(udedokei::from_path("lib.rs").unwrap(), &data);
                if check_if_uses_nightly_features(&data) {
                    is_nightly = true;
                }
                lib_file = Some(data.to_string());
            },
            ReadAs::Toml => {
                manifest = Some(Manifest::from_slice(data.as_bytes())?);
            },
            ReadAs::ReadmeMarkdown(path_prefix) => {
                markup = Some((path_prefix, Markup::Markdown(data.to_string())));
            },
            ReadAs::ReadmeRst(path_prefix) => {
                markup = Some((path_prefix, Markup::Rst(data.to_string())));
            },
            ReadAs::GetStatsOfFile(lang) => {
                stats.add_to_stats(lang, &data);
            },
        }
    }

    let mut manifest = manifest.ok_or_else(|| crate_files::UnarchiverError::TomlNotFound(
        files.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", "),
    ))?;

    manifest.complete_from_abstract_filesystem(FilesFs(&files))?;

    Ok(CrateFile {
        decompressed_size,
        readme: markup.map(|(path, m)| readme_from_repo(m, manifest.package.as_ref().and_then(|r| r.repository.as_ref()), &path)),
        manifest,
        files,
        lib_file,
        language_stats: stats.finish(),
        is_nightly,
    })
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
    pub files: Vec<PathBuf>,
    pub readme: Option<Readme>,
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

    // /// Find path that matches according to the callback
    // pub fn find(&self, mut f: impl FnMut(&Path) -> bool) -> Option<&Path> {
    //     self.files.iter().map(|p| p.as_path()).find(|p| f(p))
    // }
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
