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
mod iter;
use iter::HistoryIter;


fn commit_history_iter<'a>(repo: &Repository, commit: &Reference<'a>) -> Result<HistoryIter<'a>, git2::Error> {
    if repo.is_shallow() {
        repo.find_remote("origin")?.fetch(&["master"], None, None)?;
    }
    Ok(HistoryIter::new(commit.peel_to_commit()?))
}

pub fn checkout(repo: &Repo, base_path: &Path, name: &str) -> Result<Repository, git2::Error> {
    let repo = get_repo(repo, base_path, name)?;
    Ok(repo)
}

#[inline]
pub fn iter_blobs<F>(repo: &Repository, tree: &Tree, mut cb: F) -> Result<(), failure::Error>
    where F: FnMut(&str, &str, Blob) -> Result<(), failure::Error>
{
    iter_blobs_recurse(repo, tree, &mut String::with_capacity(500), &mut cb)?;
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

pub fn find_manifests(repo: &Repository) -> Result<Vec<(String, TomlManifest)>, failure::Error> {
    let head = repo.head()?;
    let tree = head.peel_to_tree()?;
    find_manifests_in_tree(&repo, &tree)
}

fn find_manifests_in_tree(repo: &Repository, tree: &Tree) -> Result<Vec<(String, TomlManifest)>, failure::Error> {
    let mut tomls = Vec::new();
    iter_blobs(repo, tree, |inner_path, name, blob| {
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

fn path_in_repo(repo: &Repository, tree: &Tree, crate_name: &str) -> Result<Option<String>, failure::Error> {
    Ok(find_manifests_in_tree(repo, tree)?
        .into_iter()
        .find(|(_, manifest)| manifest.package.name == crate_name)
        .map(|(path, _)| path))
}

pub fn find_readme(repo: &Repository, package: &TomlPackage) -> Result<Option<Readme>, failure::Error> {
    let head = repo.head()?;
    let tree = head.peel_to_tree()?;
    let mut readme = None;

    let prefix = path_in_repo(&repo, &tree, &package.name)?;
    let prefix = prefix.as_ref().map(|s| s.as_str()).unwrap_or("");

    iter_blobs(&repo, &tree, |base, name, blob| {
        let rel_path = if base.starts_with(prefix) {&base[prefix.len()..]} else {base};
        let rel_path_name = Path::new(rel_path).join(name);
        if is_readme_filename(&rel_path_name, Some(package)) {
            let text = String::from_utf8_lossy(blob.content()).to_string();
            let markup = if rel_path_name.extension().map_or(false, |e| e == "rst") {
                Markup::Rst(text)
            } else {
                Markup::Markdown(text)
            };
            readme = Some(readme_from_repo(markup, &package.repository, base));
        }
        Ok(())
    })?;
    Ok(readme)
}


fn readme_from_repo(markup: Markup, repo_url: &Option<String>, base_dir_in_repo: &str) -> Readme {
    let repo = repo_url.as_ref().and_then(|url| Repo::new(url).ok());
    let base_url = repo.as_ref().map(|r| r.readme_base_url(base_dir_in_repo));
    let base_image_url = repo.map(|r| r.readme_base_image_url(base_dir_in_repo));

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


