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
use std::collections::{HashMap, HashSet};
use std::collections::hash_map::Entry::Vacant;

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
                .arg("--depth=64")
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

pub fn find_manifests(repo: &Repository) -> Result<(Vec<(String, TomlManifest)>, HashSet<String>), failure::Error> {
    let head = repo.head()?;
    let tree = head.peel_to_tree()?;
    find_manifests_in_tree(&repo, &tree)
}

fn find_manifests_in_tree(repo: &Repository, tree: &Tree) -> Result<(Vec<(String, TomlManifest)>, HashSet<String>), failure::Error> {
    let mut tomls = Vec::with_capacity(8);
    let mut warnings = HashSet::new();
    iter_blobs(repo, tree, |inner_path, name, blob| {
        if name == "Cargo.toml" {
            match TomlManifest::from_slice(blob.content()) {
                Ok(toml) => tomls.push((inner_path.to_owned(), toml)),
                Err(err) => {
                    warnings.insert(format!("warning: can't parse {}/{}/{}: {}", repo.path().display(), inner_path, name, err));
                },
            }
        }
        Ok(())
    })?;
    Ok((tomls, warnings))
}

fn path_in_repo(repo: &Repository, tree: &Tree, crate_name: &str) -> Result<Option<String>, failure::Error> {
    Ok(find_manifests_in_tree(repo, tree)?.0
        .into_iter()
        .find(|(_, manifest)| manifest.package.name == crate_name)
        .map(|(path, _)| path))
}

#[derive(Debug, Copy, Clone, Default)]
struct State {
    since: Option<usize>,
    until: Option<usize>,
}


/// Callback gets added, removed, number of commits ago.
pub fn find_dependency_changes(repo: &Repository, mut cb: impl FnMut(HashSet<String>, HashSet<String>, usize)) -> Result<(), failure::Error> {
    let head = repo.head()?;

    let mut newer_deps: HashMap<String, State> = HashMap::with_capacity(100);

    // iterates from the latest!
    // The generation number here is not quite accurate (due to diamond-shaped histories),
    // but I need the fiction of it being linerar for this implementation.
    // A recursive implementation could do it better, maybe.
    let commits = commit_history_iter(&repo, &head)?.filter(|c| !c.is_merge).map(|c| c.commit);
    for (age, commit) in commits.enumerate() {
        // All deps in a repo, because we traverse history once per repo, not once per crate,
        // and because moving of deps between internal crates doesn't count.
        let mut older_deps = HashSet::with_capacity(100);
        for (_, mut manifest) in find_manifests_in_tree(&repo, &commit.tree()?)?.0 {
            older_deps.extend(manifest.dependencies.into_iter().map(|(k,_)| k));
            older_deps.extend(manifest.dev_dependencies.into_iter().map(|(k,_)| k));
            older_deps.extend(manifest.build_dependencies.into_iter().map(|(k,_)| k));
        }

        let mut added = HashSet::with_capacity(10);
        let mut removed = HashSet::with_capacity(10);

        for (dep, state) in &mut newer_deps {
            // if it's Some(), it's going to be added in the future! so it's not there now
            // (as a side effect if dependency is added, removed, then re-added, it tracks only the most recent add/remove)
            if state.since.is_none() {
                if !older_deps.contains(dep) {
                    added.insert(dep.clone());
                    state.since = Some(age);
                }
            }
        }

        for dep in older_deps {
            match newer_deps.entry(dep) {
                Vacant(e) => {
                    if age > 0 {
                        removed.insert(e.key().clone());
                        e.insert(State {since: None, until: Some(age)});
                    } else {
                        e.insert(State::default());
                    }
                },
                _ => {},
            }
        }

        cb(added, removed, age);
    }
    Ok(())
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


