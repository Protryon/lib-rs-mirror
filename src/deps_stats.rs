use std::sync::Mutex;
use std::sync::Arc;
use std::collections::HashMap;
use std::collections::HashSet;
use index::*;
use rayon::prelude::*;
use semver::Version as SemVer;

pub struct DepsStats {
    pub total: usize,
    pub counts: HashMap<Arc<str>, Counts>,
}

#[derive(Debug, Clone, Default)]
pub struct Counts {
    /// Default, optional
    pub runtime: (usize, usize),
    pub build: (usize, usize),
    pub dev: usize,
    pub direct: usize,
    pub versions: HashMap<SemVer, u32>,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash)]
enum DepTy {
    Runtime,
    Build,
    Dev,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash)]
struct DepInf {
    direct: bool,
    default: bool,
    ty: DepTy,
}

impl Index {
    pub(crate) fn get_deps_stats(&self) -> DepsStats {
        let crates = self.crates();
        let crates: Vec<_> = crates
        .par_iter()
        .filter_map(|(_, c)| {
            let mut collected = HashMap::with_capacity(120);
            let mut node_visited = HashSet::with_capacity(120);

            flatten(&self.deps_of_crate(c, DepQuery {
                default: true,
                all_optional: false,
                dev: false,
            }).ok()?, DepInf {
                default: true,
                direct: true,
                ty: DepTy::Runtime,
            }, &mut collected, &mut node_visited);

            flatten(&self.deps_of_crate(c, DepQuery {
                default: true,
                all_optional: true,
                dev: false,
            }).ok()?, DepInf {
                default: false, // false, because real defaults have already been set
                direct: true,
                ty: DepTy::Runtime,
            }, &mut collected, &mut node_visited);

            flatten(&self.deps_of_crate(c, DepQuery {
                    default: true,
                    all_optional: true,
                    dev: true,
                }).ok()?, DepInf {
                default: false,  // false, because real defaults have already been set
                direct: true,
                ty: DepTy::Dev,
            }, &mut collected, &mut node_visited);

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
                let n = counts.entry(name.clone()).or_insert(Counts::default());
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

fn flatten(dep: &Dep, depinf: DepInf, collected: &mut HashMap<Arc<str>, (DepInf, SemVer)>, node_visited: &mut HashSet<(DepInf, *const Mutex<DepSet>)>) {
    flatten_set(&dep.runtime, depinf, collected, node_visited);
    let ty = if depinf.ty == DepTy::Dev {DepTy::Dev} else {DepTy::Build};
    flatten_set(&dep.build, DepInf {ty, ..depinf}, collected, node_visited);
}

fn flatten_set(depset: &ArcDepSet, depinf: DepInf, collected: &mut HashMap<Arc<str>, (DepInf, SemVer)>, node_visited: &mut HashSet<(DepInf, *const Mutex<DepSet>)>) {
    let target_addr: &Mutex<HashMap<DepName, Dep>> = &*depset;
    if node_visited.insert((depinf, target_addr as *const _)) {
        if let Ok(depset) = depset.try_lock() {
            for ((name, _), dep) in depset.iter() {
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
                flatten(dep, DepInf {direct: false, ..depinf}, collected, node_visited);
            }
        }
    }
}
