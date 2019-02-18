use fxhash::FxHashSet;
use fxhash::FxHashMap;
use crate::index::*;
use crate::KitchenSinkErr;
use crates_index::Crate;
use rayon::prelude::*;
use std::sync::Mutex;
use string_interner::Sym;

pub struct DepsStats {
    pub total: usize,
    pub counts: FxHashMap<Box<str>, RevDependencies>,
}

#[derive(Debug, Clone, Default)]
pub struct RevDependencies {
    /// Default, optional
    pub runtime: (u16, u16),
    pub build: (u16, u16),
    pub dev: u16,
    pub direct: u16,
    pub versions: FxHashMap<MiniVer, u16>,
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
    node_visited: FxHashSet<(DepInf, *const Mutex<DepSet>)>,
}

impl DepVisitor {
    pub fn new() -> Self {
        Self {
            node_visited: FxHashSet::with_capacity_and_hasher(120, Default::default()),
        }
    }

    pub fn visit(&mut self, depset: &ArcDepSet, depinf: DepInf, mut cb: impl FnMut(&mut Self, &DepName, &Dep)) {
        let target_addr: &Mutex<FxHashMap<DepName, Dep>> = &*depset;
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
        self.recurse_inner(dep, DepInf { direct: true, ..depinf }, cb)
    }

    #[inline]
    pub fn recurse(&mut self, dep: &Dep, depinf: DepInf, cb: impl FnMut(&mut DepVisitor, &ArcDepSet, DepInf)) {
        self.recurse_inner(dep, DepInf { direct: false, ..depinf }, cb)
    }

    #[inline]
    fn recurse_inner(&mut self, dep: &Dep, depinf: DepInf, mut cb: impl FnMut(&mut DepVisitor, &ArcDepSet, DepInf)) {
        cb(self, &dep.runtime, depinf);
        let ty = if depinf.ty == DepTy::Dev { DepTy::Dev } else { DepTy::Build };
        cb(self, &dep.build, DepInf { ty, ..depinf });
    }
}

impl Index {
    pub fn all_dependencies_flattened(&self, c: &Crate) -> Result<FxHashMap<Box<str>, (DepInf, MiniVer)>, KitchenSinkErr> {
        let mut collected = FxHashMap::with_capacity_and_hasher(120, Default::default());
        let mut visitor = DepVisitor::new();

        flatten(&self.deps_of_crate(c, DepQuery {
            default: true,
            all_optional: false,
            dev: false,
        })?, DepInf {
            default: true,
            direct: true,
            ty: DepTy::Runtime,
        }, &mut collected, &mut visitor);

        flatten(&self.deps_of_crate(c, DepQuery {
            default: true,
            all_optional: true,
            dev: false,
        })?, DepInf {
            default: false, // false, because real defaults have already been set
            direct: true,
            ty: DepTy::Runtime,
        }, &mut collected, &mut visitor);

        flatten(&self.deps_of_crate(c, DepQuery {
                default: true,
                all_optional: true,
                dev: true,
            })?, DepInf {
            default: false,  // false, because real defaults have already been set
            direct: true,
            ty: DepTy::Dev,
        }, &mut collected, &mut visitor);

        if collected.is_empty() {
            return Ok(FxHashMap::default());
        }


        let inter = self.inter.read().expect("read lock poison");
        let mut converted = FxHashMap::with_capacity_and_hasher(collected.len(), Default::default());
        converted.extend(collected.into_iter().map(|(k, v)| {
            (inter.resolve(k).expect("resolve").into(), v)
        }));
        Ok(converted)
    }

    pub(crate) fn get_deps_stats(&self) -> DepsStats {
        let crates = self.crates_io_crates();
        let crates: Vec<FxHashMap<_,_>> = crates
        .par_iter()
        .filter_map(|(_, c)| {
            self.all_dependencies_flattened(c)
            .ok()
            .filter(|collected| !collected.is_empty())
        }).collect();

        self.clear_cache();

        let total = crates.len();
        let mut counts = FxHashMap::with_capacity_and_hasher(total, Default::default());
        for deps in crates {
            for (name, (depinf, semver)) in deps {
                let n = counts.entry(name.clone()).or_insert_with(RevDependencies::default);
                let t = n.versions.entry(semver).or_insert(0);
                *t = t.checked_add(1).expect("overflow");
                if depinf.direct {
                    n.direct = n.direct.checked_add(1).expect("overflow");
                }
                match depinf.ty {
                    DepTy::Runtime => {
                        if depinf.default {
                            n.runtime.0 = n.runtime.0.checked_add(1).expect("overflow");
                        } else {
                            n.runtime.1 = n.runtime.1.checked_add(1).expect("overflow");
                        }
                    },
                    DepTy::Build => {
                        if depinf.default {
                            n.build.0 = n.build.0.checked_add(1).expect("overflow");
                        } else {
                            n.build.1 = n.build.1.checked_add(1).expect("overflow");
                        }
                    },
                    DepTy::Dev => {
                        n.dev = n.dev.checked_add(1).expect("overflow");
                    },
                }
            }
        }

        DepsStats { total, counts }
    }
}

fn flatten(dep: &Dep, depinf: DepInf, collected: &mut FxHashMap<Sym, (DepInf, MiniVer)>, visitor: &mut DepVisitor) {
    visitor.start(dep, depinf, |vis, dep, depinf| flatten_set(dep, depinf, collected, vis));
}

fn flatten_set(depset: &ArcDepSet, depinf: DepInf, collected: &mut FxHashMap<Sym, (DepInf, MiniVer)>, visitor: &mut DepVisitor) {
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
