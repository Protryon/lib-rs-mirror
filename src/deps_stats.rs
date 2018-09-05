use std::sync::Mutex;
use std::sync::Arc;
use std::collections::HashMap;
use std::collections::HashSet;
use index::*;
use rayon::prelude::*;
use semver::Version as SemVer;

pub struct DepsStats {
    pub total: usize,
    pub counts: HashMap<Arc<str>, RevDependencies>,
}

#[derive(Debug, Clone, Default)]
pub struct RevDependencies {
    /// Default, optional
    pub runtime: (usize, usize),
    pub build: (usize, usize),
    pub dev: usize,
    pub direct: usize,
    pub versions: HashMap<SemVer, u32>,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum DepTy {
    Runtime,
    Build,
    Dev,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct DepInf {
    pub direct: bool,
    pub default: bool,
    pub ty: DepTy,
}

pub struct DepVisitor {
    node_visited: HashSet<(DepInf, *const Mutex<DepSet>)>,
}

impl DepVisitor {
    pub fn new() -> Self {
        Self {
            node_visited: HashSet::with_capacity(120),
        }
    }

    pub fn visit(&mut self, depset: &ArcDepSet, depinf: DepInf, mut cb: impl FnMut(&mut Self, &DepName, &Dep)) {
        let target_addr: &Mutex<HashMap<DepName, Dep>> = &*depset;
        if self.node_visited.insert((depinf, target_addr as *const _)) {
            if let Ok(depset) = depset.try_lock() {
                for (name, dep) in depset.iter() {
                    cb(self, name, dep);
                }
            }
        }
    }

    #[inline]
    pub fn start(&mut self, dep: &Dep, depinf: DepInf, cb: impl FnMut(&mut DepVisitor, &ArcDepSet, DepInf)) {
        self.recurse_inner(dep, DepInf {direct: true, ..depinf}, cb)
    }

    #[inline]
    pub fn recurse(&mut self, dep: &Dep, depinf: DepInf, cb: impl FnMut(&mut DepVisitor, &ArcDepSet, DepInf)) {
        self.recurse_inner(dep, DepInf {direct: false, ..depinf}, cb)
    }

    #[inline]
    fn recurse_inner(&mut self, dep: &Dep, depinf: DepInf, mut cb: impl FnMut(&mut DepVisitor, &ArcDepSet, DepInf)) {
        cb(self, &dep.runtime, depinf);
        let ty = if depinf.ty == DepTy::Dev {DepTy::Dev} else {DepTy::Build};
        cb(self, &dep.build, DepInf {ty, ..depinf});
    }
}

impl Index {
    pub(crate) fn get_deps_stats(&self) -> DepsStats {
        let crates = self.crates();
        let crates: Vec<_> = crates
        .par_iter()
        .filter_map(|(_, c)| {
            let mut collected = HashMap::with_capacity(120);
            let mut visitor = DepVisitor::new();

            flatten(&self.deps_of_crate(c, DepQuery {
                default: true,
                all_optional: false,
                dev: false,
            }).ok()?, DepInf {
                default: true,
                direct: true,
                ty: DepTy::Runtime,
            }, &mut collected, &mut visitor);

            flatten(&self.deps_of_crate(c, DepQuery {
                default: true,
                all_optional: true,
                dev: false,
            }).ok()?, DepInf {
                default: false, // false, because real defaults have already been set
                direct: true,
                ty: DepTy::Runtime,
            }, &mut collected, &mut visitor);

            flatten(&self.deps_of_crate(c, DepQuery {
                    default: true,
                    all_optional: true,
                    dev: true,
                }).ok()?, DepInf {
                default: false,  // false, because real defaults have already been set
                direct: true,
                ty: DepTy::Dev,
            }, &mut collected, &mut visitor);

            if collected.is_empty() {
                None
            } else {
                Some(collected)
            }
        }).collect();

        let total = crates.len();
        let mut counts = HashMap::with_capacity(total);
        for deps in crates {
            for (name, (depinf, semver)) in deps {
                let n = counts.entry(name.clone()).or_insert(RevDependencies::default());
                *n.versions.entry(semver).or_insert(0) += 1;
                if depinf.direct {
                    n.direct += 1;
                }
                match depinf.ty {
                    DepTy::Runtime => {
                        if depinf.default {
                            n.runtime.0 += 1;
                        } else {
                            n.runtime.1 += 1;
                        }
                    },
                    DepTy::Build => {
                        if depinf.default {
                            n.build.0 += 1;
                        } else {
                            n.build.1 += 1;
                        }
                    },
                    DepTy::Dev => {
                        n.dev += 1;
                    }
                }
            }
        }

        DepsStats {
            total,
            counts,
        }
    }
}

fn flatten(dep: &Dep, depinf: DepInf, collected: &mut HashMap<Arc<str>, (DepInf, SemVer)>, visitor: &mut DepVisitor) {
    visitor.start(dep, depinf, |vis, dep, depinf| flatten_set(dep, depinf, collected, vis));
}

fn flatten_set(depset: &ArcDepSet, depinf: DepInf, collected: &mut HashMap<Arc<str>, (DepInf, SemVer)>, visitor: &mut DepVisitor) {
    visitor.visit(depset, depinf, |vis, (name, _), dep| {
        collected.entry(name.clone())
            .and_modify(|(old, semver)| {
                if depinf.default {old.default = true;}
                if depinf.direct {
                    old.direct = true;
                    *semver = dep.semver.clone(); // direct version is most important; used for estimating out-of-date versions
                }
                match (old.ty, depinf.ty) {
                    (_, DepTy::Runtime) => {old.ty = DepTy::Runtime;},
                    (DepTy::Dev, DepTy::Build) => {old.ty = DepTy::Build;},
                    _ => {},
                }
            })
            .or_insert((depinf, dep.semver.clone()));
        vis.recurse(dep, depinf, |vis, dep, depinf| flatten_set(dep, depinf, collected, vis));
    })
}
