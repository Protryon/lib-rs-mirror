use crate::reverse_dependencies::DownloadsBar;
use chrono::DateTime;
use std::collections::HashMap;
use std::collections::HashSet;
use crate::Page;
use kitchen_sink::KitchenSink;
use kitchen_sink::KitchenSinkErr;

use rich_crate::{RichCrate, RichCrateVersion};
use semver::Version as SemVer;
use std::mem;

pub struct AllVersions<'a> {
    pub(crate) all: &'a RichCrate,
    pub(crate) version_history: Vec<VerRow>,
    pub(crate) changelog_url: Option<String>,
    pub(crate) capitalized_name: String,
    pub(crate) has_feat_changes: bool,
    pub(crate) has_deps_changes: bool,
}


#[derive(Debug)]
pub(crate) struct VerRow {
    pub yanked: bool,
    pub is_semver_major_change: bool,
    pub version: SemVer,
    pub release_date: String,
    pub git_rev: Option<String>,
    pub deps_added: Vec<String>,
    pub deps_removed: Vec<String>,
    pub deps_upgraded: Vec<(String, String)>,
    pub feat_added: Vec<String>,
    pub feat_removed: Vec<String>,
    pub dl: DownloadsBar,
}

impl<'a> AllVersions<'a> {
    pub(crate) async fn new(all: &'a RichCrate, ver: &RichCrateVersion, kitchen_sink: &KitchenSink) -> Result<AllVersions<'a>, KitchenSinkErr> {

        let changelog_url = kitchen_sink.changelog_url(ver).await;
        let downloads = kitchen_sink.recent_downloads_by_version(ver.origin()).await.map_err(|e| log::error!("d/l: {}", e)).unwrap_or_default();
        let capitalized_name = ver.capitalized_name().to_string();

        let ver_dates = all.versions();
        let ver_dates: HashMap<_, _> = ver_dates.iter().map(|v| (v.num.as_str(), v)).collect();
        let ver = match kitchen_sink.all_crates_io_versions(all.origin()) {
            Ok(v) => v,
            Err(KitchenSinkErr::NoVersions) => Vec::new(),
            Err(e) => return Err(e),
        };

        let mut combined_meta: Vec<(SemVer, _, _, _)> = ver.into_iter().filter_map(|version_meta| {
            let num = version_meta.version();
            let sem = num.parse().ok()?;
            let release_date = DateTime::parse_from_rfc3339(&ver_dates.get(num)?.created_at)
                .map_err(|e| log::error!("bad date {}: {}", all.name(), e)).ok()?;

            let mut required_deps = HashMap::with_capacity(version_meta.dependencies().len());

            for req in version_meta.dependencies() {
                if req.kind() == kitchen_sink::DependencyKind::Dev {
                    continue;
                }

                let dep_name = req.crate_name().to_ascii_lowercase();
                let ver_req = req.requirement();
                let (actual_version, _) = match kitchen_sink.crates_io_version_matching_requirement_by_lowercase_name(&dep_name, ver_req) {
                    Ok(d) => d,
                    Err(e) => {
                        log::warn!("{} requires broken {} {}: {}", all.name(), dep_name, ver_req, e);
                        continue;
                    },
                };

                let r_dep = required_deps.entry(dep_name).or_insert_with(HashMap::new);
                // TODO: track changes to req.is_optional()?
                r_dep.insert(map_to_major(&actual_version), actual_version);
            }

            Some((sem, version_meta, release_date, required_deps))
        }).collect();
        combined_meta.sort_by(|(a, ..), (b, ..)| a.cmp(b));

        let mut prev_required_deps = None::<HashMap<String, HashMap<_, _>>>;
        let mut prev_features = None::<HashSet<_>>;
        let mut prev_semver = None::<SemVer>;
        let mut version_history: Vec<_> = combined_meta.into_iter().map(|(version, version_meta, release_date, required_deps)| {
            let yanked = version_meta.is_yanked();
            let release_date = release_date.format("%b %e, %Y").to_string();

            let dl = {
                let num = downloads.get(&version.clone().into()).copied().unwrap_or(0);
                DownloadsBar {
                    num,
                    str: crate::format_downloads(num),
                    perc: 0., // fixed later
                    num_width: 0.,
                }
            };

            let git_rev = None;
            let mut feat_added = Vec::new();
            let mut feat_removed = Vec::new();
            let mut deps_added = Vec::new();
            let mut deps_removed = Vec::new();
            let mut deps_upgraded = Vec::new();

            let is_semver_major_change = match &prev_semver {
                Some(prev) => semver_major_differs(prev, &version),
                None => false,
            };
            prev_semver = Some(version.clone());

            if yanked {
                // everything intentionally left empty, don't update prev deps, so
                // that only stable compares with stable
                return VerRow {
                    yanked,
                    version,
                    release_date,
                    is_semver_major_change,
                    git_rev,
                    deps_added,
                    deps_removed,
                    deps_upgraded,
                    feat_added,
                    feat_removed,
                    dl,
                }
            }

            match mem::take(&mut prev_required_deps) {
                Some(mut prev) => {
                    for (new_k, new_v) in &required_deps {
                        match prev.remove(new_k) {
                            Some(prev_v) => {
                                // both versions have the same crate
                                for (k, new) in new_v {
                                    if prev_v.get(k).is_none() {
                                        deps_upgraded.push((new_k.clone(), new.to_string()))
                                    }
                                }
                            },
                            None => {
                                deps_added.push(new_k.clone());
                            }
                        }
                    }
                    deps_removed.extend(prev.into_iter()
                        .map(|(k,_)| k)
                        .filter(|k| required_deps.get(k).is_none()));
                },
                None => {}
            };
            prev_required_deps = Some(required_deps);
            deps_added.sort();
            deps_upgraded.sort();
            deps_removed.sort();

            let features: HashSet<_> = version_meta.features().keys()
                .filter(|k| !k.starts_with('_') && *k != "default")
                .cloned().collect();
            if let Some(prev) = &prev_features {
                feat_added.extend(features.difference(prev).cloned());
                feat_removed.extend(prev.difference(&features).cloned());
            }
            prev_features = Some(features);
            feat_added.sort();
            feat_removed.sort();

            VerRow {
                yanked,
                version,
                release_date,
                is_semver_major_change,
                git_rev,
                deps_removed,
                deps_added,
                deps_upgraded,
                feat_added,
                feat_removed,
                dl,
            }
        }).collect();

        // make max artificially higher, so that small number of downloads looks small
        let dl_max = version_history.iter().map(|v| v.dl.num).max().unwrap_or(0).max(100) as f32 + 100.0;
        for i in &mut version_history {
            i.dl.perc = i.dl.num as f32 / dl_max * 100.0;
            i.dl.str = crate::format_downloads(i.dl.num);
            i.dl.num_width = 4. + 7. * (i.dl.str.0.len() + i.dl.str.1.len()) as f32; // approx visual width of the number
        }

        Ok(Self {
            has_feat_changes: version_history.iter().any(|v| !v.feat_added.is_empty() || !v.feat_removed.is_empty()),
            has_deps_changes: version_history.iter().any(|v| !v.deps_added.is_empty() || !v.deps_removed.is_empty() || !v.deps_upgraded.is_empty()),
            changelog_url,
            all,
            version_history,
            capitalized_name,
        })
    }

    pub fn page(&self) -> Page {
        Page {
            title: format!("All releases of {}", self.capitalized_name),
            item_name: None,
            item_description: None,
            noindex: true,
            search_meta: false,
            critical_css_data: Some(include_str!("../../style/public/all_versions.css")),
            critical_css_dev_url: Some("/all_versions.css"),
            ..Default::default()
        }
    }
}

fn map_to_major(v: &SemVer) -> (bool, bool, u64) {
    let pre = v.is_prerelease();
    if v.major == 0 {
        (pre, false, v.minor)
    } else {
        (pre, true, v.major)
    }
}

fn semver_major_differs(a: &SemVer, b: &SemVer) -> bool {
    a.major != b.major || (a.major == 0 && a.minor != b.minor) || a.is_prerelease() != b.is_prerelease()
}

