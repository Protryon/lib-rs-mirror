#![allow(unused)]
use git2::Repository;
use git2::RepositoryInitOptions;
use quick_error::quick_error;
use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::RwLock;

const DEBCARGO_CONF_REPO_URL: &str = "https://salsa.debian.org/rust-team/debcargo-conf.git";

quick_error! {
    #[derive(Debug)]
    pub enum Error {
        Git(err: git2::Error) {
            display("{}", err)
            from()
        }
        Other(s: &'static str) {
            display("{}", s)
            from()
        }
    }
}

pub struct DebcargoList {
    repo: Repository,
    list: RwLock<HashSet<Box<str>>>,
}

// *Hopefully* `git2::Repository` is safe to use?
unsafe impl Sync for DebcargoList {}
unsafe impl Send for DebcargoList {}

impl DebcargoList {
    pub fn new(cache_dir: &Path) -> Result<Self, Error> {
        let repo_path = cache_dir.join("debcargo-conf");

        let (needs_update, repo) = match Repository::open(&repo_path) {
            Ok(repo) => (false, repo),
            Err(_) => {
                let mut opts = RepositoryInitOptions::new();
                opts.external_template(false);
                opts.bare(true);
                (true, Repository::init_opts(&repo_path, &opts)?)
            }
        };
        let list = Self { repo, list: RwLock::new(HashSet::new()) };
        if needs_update {
            list.update()?;
        }
        Ok(list)
    }

    pub fn update(&self) -> Result<(), Error> {
        let mut remote = self.repo.remote_anonymous(DEBCARGO_CONF_REPO_URL)?;
        remote.fetch(&["HEAD:refs/remotes/origin/HEAD"], None, None)?;
        drop(remote);
        self.read_list()?;
        Ok(())
    }

    fn read_list(&self) -> Result<(), Error> {
        // Lock early just in case libgit2 isn't thread-safe
        let mut locked_list = self.list.write().unwrap();

        let head = self.repo.refname_to_id("FETCH_HEAD")?;
        let commit = self.repo.find_commit(head)?;
        let tree = commit.tree()?;
        let src = tree.get_name("src").ok_or("oops, borked repo")?.to_object(&self.repo)?.peel_to_tree()?;
        let data: HashSet<_, _> = src.iter().filter_map(|e| e.name().map(Box::from)).collect();
        if data.is_empty() {
            return Err("unexpectedly empty")?;
        }
        *locked_list = data;
        Ok(())
    }

    pub fn has(&self, crate_name: &str) -> Result<bool, Error> {
        // there are packages in format crate-version, but they can be ignored,
        // because Debian has a rule that there's always also a package without the version suffix
        let mut l = self.list.read().unwrap();
        if l.is_empty() {
            drop(l);
            self.read_list()?;
            l = self.list.read().unwrap();
        }
        Ok(l.contains(crate_name))
    }
}

#[test]
fn is_send() {
    fn check<T: Send + Sync>() {};
    check::<DebcargoList>();
}

#[test]
fn debcargo_test() {
    let l = DebcargoList::new(Path::new("/tmp/debcargo-conf-test")).unwrap();
    assert!(l.has("rand").unwrap());
    assert!(!l.has("").unwrap());
    assert!(!l.has("/").unwrap());
    assert!(!l.has("..").unwrap());
    assert!(!l.has("borkedcratespam").unwrap());
    assert!(l.has("rgb").unwrap());
}
