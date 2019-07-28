use crate::iter::HistoryIter;
use cargo_toml::{Manifest, Package};
use failure;
use git2;
use git2::build::RepoBuilder;
use git2::{Blob, ObjectType, Reference, Repository, Tree};
use lazy_static::lazy_static;
use render_readme;
use render_readme::{Markup, Readme};
use repo_url::Repo;
use std::collections::hash_map::Entry::Vacant;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::sync::Mutex;
use urlencoding;

mod iter;

lazy_static! {
    static ref GLOBAL_LOCK: Mutex<HashMap<String, Arc<Mutex<()>>>> = Mutex::new(HashMap::new());
}

#[derive(Debug, Clone)]
pub struct ParseError(pub String);

fn commit_history_iter<'a>(repo: &Repository, commit: &Reference<'a>) -> Result<HistoryIter<'a>, git2::Error> {
    if repo.is_shallow() {
        repo.find_remote("origin")?.fetch(&["master"], None, None)?;
    }
    Ok(HistoryIter::new(commit.peel_to_commit()?))
}

pub fn checkout(repo: &Repo, base_path: &Path) -> Result<Repository, git2::Error> {
    let repo = get_repo(repo, base_path)?;
    Ok(repo)
}

#[inline]
pub fn iter_blobs<F>(repo: &Repository, tree: &Tree<'_>, mut cb: F) -> Result<(), failure::Error>
    where F: FnMut(&str, &str, Blob<'_>) -> Result<(), failure::Error>
{
    iter_blobs_recurse(repo, tree, &mut String::with_capacity(500), &mut cb)?;
    Ok(())
}

fn iter_blobs_recurse<F>(repo: &Repository, tree: &Tree<'_>, path: &mut String, cb: &mut F) -> Result<(), failure::Error>
    where F: FnMut(&str, &str, Blob<'_>) -> Result<(), failure::Error>
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

fn get_repo(repo: &Repo, base_path: &Path) -> Result<Repository, git2::Error> {
    // ensure one clone per dir at a time
    let lock = GLOBAL_LOCK.lock().unwrap().entry(repo.canonical_git_url().to_string()).or_insert_with(|| Arc::new(Mutex::new(()))).clone();
    let _lock = lock.lock().unwrap();

    let shallow = false;
    let url = &*repo.canonical_git_url();

    let repo_path = base_path.join(urlencoding::encode(url));

    match Repository::open(&repo_path) {
        Ok(repo) => Ok(repo),
        Err(err) => {
            if !url.starts_with("http://") && !url.starts_with("https://") && !url.starts_with("git@github.com:") {
                eprintln!("Rejecting non-HTTP git URL: {}", url);
                return Err(err);
            }
            if err.code() == git2::ErrorCode::Exists {
                if let Ok(repo) = Repository::open(&repo_path) {
                    return Ok(repo)
                }
                let _ = fs::remove_dir_all(&repo_path);
            }
            if shallow {
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
                if ok {
                    return Repository::open(repo_path);
                }
            }
            let mut ch = RepoBuilder::new();
            ch.bare(true);
            // no support for depth=1!
            ch.clone(&url, &repo_path)
        },
    }
}

pub fn find_manifests(repo: &Repository) -> Result<(Vec<(String, Manifest)>, Vec<ParseError>), failure::Error> {
    let head = repo.head()?;
    let tree = head.peel_to_tree()?;
    find_manifests_in_tree(&repo, &tree)
}

struct GitFS<'a, 'b> {
    repo: &'b Repository,
    tree: &'b Tree<'a>,
}

impl cargo_toml::AbstractFilesystem for GitFS<'_, '_> {
    fn file_names_in(&self, dir_path: &str) -> Result<HashSet<Box<str>>, io::Error> {
        self.file_names_in_tree(&self.tree, Some(dir_path))
    }
}

impl GitFS<'_, '_> {
    fn file_names_in_tree(&self, curr_dir: &Tree<'_>, dir_path: Option<&str>) -> Result<HashSet<Box<str>>, io::Error> {
        if let Some(dir_path) = dir_path {
            let mut parts = dir_path.splitn(2, '/');
            let subdir_name = parts.next().unwrap();
            let rest = parts.next();
            for item in curr_dir.iter() {
                if item.name() == Some(subdir_name) {
                    if let Ok(tree) = self.repo.find_tree(item.id()) {
                        return self.file_names_in_tree(&tree, rest);
                    }
                }
            }
            Ok(HashSet::new()) // dir not found
        } else {
            let mut res = HashSet::new();
            for item in curr_dir.iter() {
                if let Some(n) = item.name() {
                    res.insert(n.into());
                }
            }
            Ok(res)
        }
    }
}

fn find_manifests_in_tree(repo: &Repository, tree: &Tree<'_>) -> Result<(Vec<(String, Manifest)>, Vec<ParseError>), failure::Error> {
    let mut tomls = Vec::with_capacity(8);
    let mut warnings = Vec::new();
    iter_blobs(repo, tree, |inner_path, name, blob| {
        if name == "Cargo.toml" {
            match Manifest::from_slice(blob.content()) {
                Ok(mut toml) => {
                    toml.complete_from_abstract_filesystem(GitFS {repo, tree})?;
                    if toml.package.is_some() {
                        tomls.push((inner_path.to_owned(), toml))
                    }
                },
                Err(err) => {
                    warnings.push(ParseError(format!("Can't parse {}/{}/{}: {}", repo.path().display(), inner_path, name, err)));
                },
            }
        }
        Ok(())
    })?;
    Ok((tomls, warnings))
}

fn path_in_repo(repo: &Repository, tree: &Tree<'_>, crate_name: &str) -> Result<Option<String>, failure::Error> {
    Ok(find_manifests_in_tree(repo, tree)?.0
        .into_iter()
        .find(|(_, manifest)| manifest.package.as_ref().map_or(false, |p| p.name == crate_name))
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
    for (age, commit) in commits.enumerate().take(1000) {
        // All deps in a repo, because we traverse history once per repo, not once per crate,
        // and because moving of deps between internal crates doesn't count.
        let mut older_deps = HashSet::with_capacity(100);
        for (_, manifest) in find_manifests_in_tree(&repo, &commit.tree()?)?.0 {
            older_deps.extend(manifest.dependencies.into_iter().map(|(k, _)| k));
            older_deps.extend(manifest.dev_dependencies.into_iter().map(|(k, _)| k));
            older_deps.extend(manifest.build_dependencies.into_iter().map(|(k, _)| k));
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
            if let Vacant(e) = newer_deps.entry(dep) {
                if age > 0 {
                    removed.insert(e.key().clone());
                    e.insert(State { since: None, until: Some(age) });
                } else {
                    e.insert(State::default());
                }
            }
        }

        cb(added, removed, age);
    }
    Ok(())
}



pub fn find_readme(repo: &Repository, package: &Package) -> Result<Option<Readme>, failure::Error> {
    let head = repo.head()?;
    let tree = head.peel_to_tree()?;
    let mut readme = None;
    let mut found_best = false; // it'll find many readmes, including fallbacks

    let prefix = path_in_repo(&repo, &tree, &package.name)?;
    let prefix = prefix.as_ref().map(|s| s.as_str()).unwrap_or("");

    iter_blobs(&repo, &tree, |base, name, blob| {
        if found_best {
            return Ok(()); // done
        }
        let is_correct_dir = base.starts_with(prefix);
        let rel_path = if is_correct_dir {
            &base[prefix.len()..]
        } else if readme.is_none() {
            base
        } else {
            return Ok(()); // don't search bad dirs if there's some readme already
        };
        let rel_path_name = Path::new(rel_path).join(name);
        if is_readme_filename(&rel_path_name, Some(package)) {
            let text = String::from_utf8_lossy(blob.content()).to_string();
            let markup = if rel_path_name.extension().map_or(false, |e| e == "rst") {
                Markup::Rst(text)
            } else {
                Markup::Markdown(text)
            };
            readme = Some(readme_from_repo(markup, &package.repository, base));
            found_best = is_correct_dir;
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
fn is_readme_filename(path: &Path, package: Option<&Package>) -> bool {
    path.to_str().map_or(false, |s| {
        if let Some(&Package{readme: Some(ref r),..}) = package {
            r.eq_ignore_ascii_case(s) // crates published on Mac have this
        } else {
            render_readme::is_readme_filename(path)
        }
    })
}

#[test]
fn git_fs() {
    let repo = Repository::open(".git").expect("own git repo");
    let (m, w) = find_manifests(&repo).expect("has manifests");
    assert_eq!(1, m.len());
    assert_eq!(0, w.len());
    assert_eq!("", &m[0].0);
    let manif = &m[0].1;
    let pkg = manif.package.as_ref().expect("package");
    assert_eq!("crate_git_checkout", &pkg.name);
    assert!(manif.lib.is_some());
    assert_eq!(0, manif.bin.len());
}

