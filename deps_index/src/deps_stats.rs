use crate::index::*;
use crate::DepsErr;
use crate::Origin;
use rayon::prelude::*;
use string_interner::symbol::SymbolU32 as Sym;
use smartstring::alias::String as SmolStr;

type FxHashMap<K, V> = std::collections::HashMap<K, V, ahash::RandomState>;
type FxHashSet<V> = std::collections::HashSet<V, ahash::RandomState>;

pub type DepInfMap = FxHashMap<SmolStr, (DepInf, MiniVer)>;

pub struct DepsStats {
    pub total: usize,
    pub counts: FxHashMap<SmolStr, RevDependencies>,
}

#[derive(Debug, Clone, Default)]
pub struct RevDepCount {
    pub def: u32,
    pub opt: u32,
}

impl RevDepCount {
    pub fn all(&self) -> u32 {
        self.def + self.opt
    }
}

#[derive(Debug, Clone, Default)]
pub struct DirectDepCount {
    pub runtime: u16,
    pub build: u16,
    pub dev: u16,
}

impl DirectDepCount {
    pub fn all(&self) -> u32 {
        self.runtime as u32 + self.build as u32 + self.dev as u32
    }
}

#[derive(Debug, Clone, Default)]
pub struct RevDependencies {
    /// Default, optional
    pub runtime: RevDepCount,
    pub build: RevDepCount,
    pub dev: u16,
    pub direct: DirectDepCount,
    pub versions: FxHashMap<MiniVer, u16>,
    pub rev_dep_names_default: CompactStringSet,
    pub rev_dep_names_optional: CompactStringSet,
    pub rev_dep_names_dev: CompactStringSet,
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
    node_visited: FxHashSet<(DepInf, *const DepSet)>,
}

impl DepVisitor {
    pub(crate) fn new() -> Self {
        Self {
            node_visited: FxHashSet::with_capacity_and_hasher(120, Default::default()),
        }
    }

    pub(crate) fn visit(&mut self, depset: &ArcDepSet, depinf: DepInf, mut cb: impl FnMut(&mut Self, &DepName, &Dep)) {
        let target_addr: &FxHashMap<DepName, Dep> = depset;
        if self.node_visited.insert((depinf, target_addr as *const _)) {
            for (name, dep) in depset.iter() {
                cb(self, name, dep);
            }
        }
    }

    #[inline]
    pub(crate) fn start(&mut self, dep: &Dep, depinf: DepInf, cb: impl FnMut(&mut DepVisitor, &ArcDepSet, DepInf)) {
        self.recurse_inner(dep, DepInf { direct: true, ..depinf }, cb)
    }

    #[inline]
    pub(crate) fn recurse(&mut self, dep: &Dep, depinf: DepInf, cb: impl FnMut(&mut DepVisitor, &ArcDepSet, DepInf)) {
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
    pub fn all_dependencies_flattened(&self, c: &impl ICrate) -> Result<DepInfMap, DepsErr> {
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


        let inter = self.inter.read();
        let mut converted = FxHashMap::with_capacity_and_hasher(collected.len(), Default::default());
        converted.extend(collected.into_iter().map(|(k, v)| {
            let name = inter.resolve(k).expect("resolve");
            debug_assert_eq!(name, name.to_ascii_lowercase());
            debug_assert!(!name.is_empty());
            (name.into(), v)
        }));
        Ok(converted)
    }

    pub(crate) fn get_deps_stats(&self) -> DepsStats {
        let crates = self.crates_io_crates();
        let crates: Vec<(SmolStr, FxHashMap<_,_>)> = crates
            .par_iter()
            .filter_map(|(name_lowercase, c)| {
                self.all_dependencies_flattened(c)
                .ok()
                .filter(|collected| !collected.is_empty())
                .map(|deps| {
                    (name_lowercase.clone(), deps)
                })
            }).collect();

        self.clear_cache();

        let total = crates.len();
        let mut counts = FxHashMap::with_capacity_and_hasher(total, Default::default());
        for (parent_name, deps) in crates {
            for (name, (depinf, semver)) in deps {
                let n = counts.entry(name).or_insert_with(RevDependencies::default);
                let t = n.versions.entry(semver).or_insert(0);
                *t = t.checked_add(1).expect("overflow");
                debug_assert!(Origin::is_valid_crate_name(&parent_name));
                match depinf.ty {
                    DepTy::Runtime => {
                        if depinf.direct {
                            if depinf.default { &mut n.rev_dep_names_default } else { &mut n.rev_dep_names_optional }.push(&parent_name);
                            n.direct.runtime = n.direct.runtime.checked_add(1).expect("overflow");
                        }
                        if depinf.default {
                            n.runtime.def += 1;
                        } else {
                            n.runtime.opt += 1;
                        }
                    },
                    DepTy::Build => {
                        if depinf.direct {
                            if depinf.default { &mut n.rev_dep_names_default } else { &mut n.rev_dep_names_optional }.push(&parent_name);
                            n.direct.build = n.direct.build.checked_add(1).expect("overflow");
                        }
                        if depinf.default {
                            n.build.def += 1;
                        } else {
                            n.build.opt += 1;
                        }
                    },
                    DepTy::Dev => {
                        if depinf.direct {
                            n.rev_dep_names_dev.push(&parent_name);
                            n.direct.dev = n.direct.dev.checked_add(1).expect("overflow");
                        }
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
        collected.entry(*name)
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

#[derive(Debug, Clone, Default)]
pub struct CompactStringSet(String);

impl CompactStringSet {
    pub fn push(&mut self, s: &str) {
        debug_assert!(s.bytes().all(|c| c > 0));

        if !self.0.is_empty() {
            self.0.reserve(1 + s.len());
            self.0.push('\0');
        }
        self.0.push_str(s);
    }

    pub fn iter(&self) -> impl Iterator<Item = &str> {
        if !self.0.is_empty() { Some(self.0.as_str()) } else { None }
            .into_iter().flat_map(|s| s.split('\0'))
    }
}


#[test]
fn compact_str() {
    let mut c = CompactStringSet::default();
    assert_eq!(0, c.iter().count());
    c.push("aaa");
    assert_eq!(1, c.iter().count());
    c.push("bb");
    assert_eq!(2, c.iter().count());
}
