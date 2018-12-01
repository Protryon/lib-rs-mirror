use cargo_toml::TomlManifest;
use libflate::gzip::Decoder;
use render_readme::Markup;
use std::io::Read;
use std::path::Path;
use tar::Archive;
use crate::readme_from_repo;
use crate::is_readme_filename;
use crate::{Result, UnarchiverError, CrateFile};
use udedokei;

enum ReadAs {
    Toml,
    ReadmeMarkdown(String),
    ReadmeRst(String),
    Lib,
    GetStatsOfFile(udedokei::Language)
}

pub fn read_archive(archive: impl Read, prefix: &Path) -> Result<CrateFile> {
    let decoder = Decoder::new(archive)?;
    let mut a = Archive::new(decoder);

    let mut manifest: Option<TomlManifest> = None;
    let mut markup = None;
    let mut files = Vec::new();
    let mut lib_file = None;
    let mut stats = udedokei::Collect::new();
    let mut decompressed_size = 0;
    let mut is_nightly = false;

    for file in a.entries()? {
        let mut file = file?;

        let path_match = {
            let path = file.header().path();
            let relpath = match path {
                Ok(ref p) => match p.strip_prefix(prefix) {
                    Ok(relpath) => relpath,
                    _ => continue,
                },
                _ => continue,
            };
            files.push(relpath.to_owned());

            match relpath {
                p if p == Path::new("Cargo.toml") || p == Path::new("cargo.toml") => ReadAs::Toml,
                p if p == Path::new("src/lib.rs") => ReadAs::Lib,
                p if is_readme_filename(p, manifest.as_ref().map(|m| &m.package)) => {
                    let path_prefix = p.parent().unwrap().display().to_string();
                    if p.extension().map_or(false, |e| e == "rst") {
                        ReadAs::ReadmeRst(path_prefix)
                    } else {
                        ReadAs::ReadmeMarkdown(path_prefix)
                    }
                },
                p => if let Some(lang) = is_source_code_file(p) {
                    if lang.is_code() {
                        ReadAs::GetStatsOfFile(lang)
                    } else {
                        continue;
                    }
                } else {
                    continue;
                },
            }
        };

        let mut data = Vec::with_capacity(file.header().size()? as usize);
        file.read_to_end(&mut data)?;
        decompressed_size += data.len();
        let data = String::from_utf8_lossy(&data);

        match path_match {
            ReadAs::Lib => {
                stats.add_to_stats(udedokei::Language::from_path("lib.rs").unwrap(), &data);
                if check_if_uses_nightly_features(&data) {
                    is_nightly = true;
                }
                lib_file = Some(data.to_string());
            },
            ReadAs::Toml => {
                manifest = Some(TomlManifest::from_slice(data.as_bytes())?);
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

    let manifest = manifest.ok_or_else(|| UnarchiverError::TomlNotFound(
        files.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", "),
    ))?;

    Ok(CrateFile {
        decompressed_size,
        readme: Ok(markup.map(|(path, m)| readme_from_repo(m, &manifest.package.repository, &path))),
        manifest,
        files,
        lib_file,
        language_stats: stats.finish(),
        is_nightly,
    })
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

    if path.starts_with("tests") || path.starts_with("benches") || path.starts_with("examples") {
        return None;
    }
    if let Some(name) = path.file_name() {
        if name.as_bytes().starts_with(b".") {
            return None;
        }
    } else {
        return None;
    }
    udedokei::Language::from_path(path)
}
