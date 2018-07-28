use cargo_toml::TomlManifest;
use libflate::gzip::Decoder;
use render_readme::Markup;
use std::io::Read;
use std::path::Path;
use tar::Archive;
use readme_from_repo;
use is_readme_filename;
use Result;
use {UnarchiverError, CrateFile};

enum ReadAs {
    Toml,
    ReadmeMarkdown(String),
    ReadmeRst(String),
    Lib,
}

pub fn read_archive(archive: impl Read, prefix: &Path) -> Result<CrateFile> {
    let decoder = Decoder::new(archive)?;
    let mut a = Archive::new(decoder);

    let mut manifest: Option<TomlManifest> = None;
    let mut markup = None;
    let mut files = Vec::new();
    let mut lib_file = None;

    for file in a.entries()? {
        let mut file = file?;

        let path_match = match file.header().path() {
            Ok(ref p) => if let Ok(relpath) = p.strip_prefix(prefix) {
                files.push(relpath.to_owned());
                match relpath {
                    p if p == Path::new("Cargo.toml") || p == Path::new("cargo.toml") => ReadAs::Toml,
                    p if p == Path::new("src/lib.rs") => ReadAs::Lib,
                    p => if is_readme_filename(p, manifest.as_ref().map(|m| &m.package)) {
                        let path_prefix = p.parent().unwrap().display().to_string();
                        if p.extension().map_or(false, |e| e == "rst") {
                            ReadAs::ReadmeRst(path_prefix)
                        } else {
                            ReadAs::ReadmeMarkdown(path_prefix)
                        }
                    } else {
                        continue;
                    },
                }
            } else {
                eprintln!("warning: bad prefix {} in {}", prefix.display(), p.display());
                continue
            },
            _ => continue,
        };

        let mut data = String::with_capacity(file.header().size()? as usize);
        file.read_to_string(&mut data)?;

        match path_match {
            ReadAs::Lib => {
                lib_file = Some(data);
            },
            ReadAs::Toml => {
                manifest = Some(TomlManifest::from_slice(data.as_bytes())?);
            },
            ReadAs::ReadmeMarkdown(path_prefix) => {
                markup = Some((path_prefix, Markup::Markdown(data)));
            },
            ReadAs::ReadmeRst(path_prefix) => {
                markup = Some((path_prefix, Markup::Rst(data)));
            },
        }
    }

    let manifest = manifest.ok_or_else(|| UnarchiverError::TomlNotFound(
        files.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", "),
    ))?;

    Ok(CrateFile {
        readme: Ok(markup.map(|(path, m)| readme_from_repo(m, &manifest.package.repository, &path))),
        manifest,
        files,
        lib_file,
    })
}
