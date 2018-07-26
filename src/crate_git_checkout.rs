extern crate render_readme;
extern crate cargo_toml;
extern crate repo_url;
extern crate failure;
extern crate git2;
extern crate urlencoding;

use std::process::Command;
use cargo_toml::TomlPackage;
use render_readme::Readme;
use render_readme::Markup;
use cargo_toml::TomlManifest;

use repo_url::Repo;
use std::path::Path;

use git2::{Tree, Repository, build::RepoBuilder, Reference, ObjectType, Blob};

pub fn checkout(repo: &Repo, base_path: &Path, name: &str) -> Result<Repository, git2::Error> {
    let repo = get_repo(repo, base_path, name)?;
    Ok(repo)
}

pub fn iter_blobs<F>(repo: &Repository, treeish: &Reference, mut cb: F) -> Result<(), failure::Error>
    where F: FnMut(&str, &str, Blob) -> Result<(), failure::Error>
{
    let tree = treeish.peel_to_tree()?;
    iter_blobs_recurse(repo, &tree, &mut String::with_capacity(500), &mut cb)?;
    Ok(())
}

fn iter_blobs_recurse<F>(repo: &Repository, tree: &Tree, path: &mut String, cb: &mut F) -> Result<(), failure::Error>
    where F: FnMut(&str, &str, Blob) -> Result<(), failure::Error>
{
    for i in tree.iter() {
        let name = match i.name() {
            Some(n) => n,
            _ => continue,
        };
        match i.kind() {
            Some(ObjectType::Tree) => {
                let sub = repo.find_tree(i.id())?;
                let pre_len = path.len();
                if !path.is_empty() {
                    path.push('/');
                }
                path.push_str(name);
                iter_blobs_recurse(repo, &sub, path, cb)?;
                path.truncate(pre_len);
            },
            Some(ObjectType::Blob) => {
                cb(path, name, repo.find_blob(i.id())?)?;
            },
            _ => {},
        }
    }
    Ok(())
}

fn get_repo(repo: &Repo, base_path: &Path, name: &str) -> Result<Repository, git2::Error> {
    let url = &*repo.canonical_git_url();

    let repo_path = base_path.join(urlencoding::encode(url));
    if !repo_path.exists() {
        let old_path = base_path.join(name);
        if old_path.exists() {
            let _= std::fs::rename(old_path, &repo_path);
        }
    }

    match Repository::open(&repo_path) {
        Ok(repo) => Ok(repo),
        _ => {
            let ok = Command::new("git")
                .arg("clone")
                .arg("--depth=1")
                .arg("--config").arg("core.askPass=true")
                .arg("--")
                .arg(&*url)
                .arg(&repo_path)
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false);
            if !ok {
                let mut ch = RepoBuilder::new();
                ch.bare(true);
                // no support for depth=1!
                ch.clone(&url, &repo_path)
            } else {
                Repository::open(repo_path)
            }
        },
    }
}

pub fn find_manifests(repo: &Repo, base_path: &Path, name: &str) -> Result<Vec<(String, TomlManifest)>, failure::Error> {
    let repo = get_repo(repo, base_path, name)?;
    let head = repo.head()?;
    let mut tomls = Vec::new();
    iter_blobs(&repo, &head, |inner_path, name, blob| {
        if name == "Cargo.toml" {
            match TomlManifest::from_slice(blob.content()) {
                Ok(toml) => tomls.push((inner_path.to_owned(), toml)),
                Err(err) => eprintln!("warning: can't parse {}/{}/{}: {}", repo.path().display(), inner_path, name, err),
            }
        }
        Ok(())
    })?;
    Ok(tomls)
}

pub fn find_readme(repo: &Repository, package: &TomlPackage) -> Result<Option<Readme>, failure::Error> {
    let head = repo.head()?;
    let mut readme = None;
    iter_blobs(&repo, &head, |base, name, blob| {
        let pathname = Path::new(base).join(name);
        if is_readme_filename(&pathname, Some(package)) {
            let text = String::from_utf8_lossy(blob.content()).to_string();
            let markup = if pathname.extension().map_or(false, |e| e == "rst") {
                Markup::Rst(text)
            } else {
                Markup::Markdown(text)
            };
            readme = Some(readme_from_repo(markup, &package.repository));
        }
        Ok(())
    })?;
    Ok(readme)
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
            render_readme::is_readme_filename(path)
        }
    })
}


