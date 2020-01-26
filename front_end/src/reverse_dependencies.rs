use crate::Page;
use kitchen_sink::CResult;
use kitchen_sink::KitchenSink;
use kitchen_sink::Origin;
use kitchen_sink::RevDependencies;
use kitchen_sink::SemVer;
use locale::Numeric;
use rayon::prelude::*;
use render_readme::Renderer;

use rich_crate::RichCrateVersion;
use semver::VersionReq;
use std::fmt::Display;

pub struct CratePageRevDeps<'a> {
    pub ver: &'a RichCrateVersion,
    pub deps: Vec<RevDepInf<'a>>,
    pub stats: &'a RevDependencies,
}

pub struct RevDepInf<'a> {
    pub origin: Origin,
    pub downloads: usize,
    pub rev_dep: &'a kitchen_sink::Version,
    pub is_optional: bool,
    pub matches_latest: bool,
    pub kind: &'a str,
    pub req: VersionReq,
}

impl<'a> CratePageRevDeps<'a> {
    pub fn new(ver: &'a RichCrateVersion, kitchen_sink: &'a KitchenSink, _markup: &'a Renderer) -> CResult<Self> {
        let deps = kitchen_sink.index.deps_stats()?;
        let own_name = ver.short_name();
        // RichCrateVersion may be unstable
        let latest_stable_semver = kitchen_sink.index.crate_highest_version(&own_name.to_lowercase(), true)?.version().parse()?;
        let stats = deps.counts.get(own_name).ok_or_else(|| failure::err_msg("bad crate name"))?;

        let mut deps: Vec<_> = stats.rev_dep_names.iter().par_bridge().map(|rev_dep| {
            let origin = Origin::from_crates_io_name(rev_dep);
            let downloads = kitchen_sink.downloads_per_month(&origin).ok().and_then(|x| x).unwrap_or(0);
            let rev_dep = kitchen_sink.index.crate_highest_version(&rev_dep.to_lowercase(), true).expect("rev dep integrity");
            let (is_optional, req, kind) = rev_dep.dependencies().iter().filter(|d| {
                own_name == d.crate_name()
            })
            .next()
            .map(|d| {
                (d.is_optional(), d.requirement(), d.kind().unwrap_or_default())
            })
            .unwrap_or_default();

            let req = req.parse().unwrap_or_else(|_| VersionReq::any());
            let matches_latest = req.matches(&latest_stable_semver);

            RevDepInf {
                origin,
                rev_dep, downloads, is_optional, req, kind,
                matches_latest,
            }
        }).collect();

        // sort by downloads if > 100, then by name
        deps.sort_by(|a, b| {
            b.downloads.max(100).cmp(&a.downloads.max(100))
            .then_with(|| {
                a.rev_dep.name().cmp(b.rev_dep.name())
            })
        });
        deps.truncate(1000);

        Ok(Self {
            ver,
            deps,
            stats,
        })
    }

    /// Nicely rounded number of downloads
    ///
    /// To show that these numbers are just approximate.
    pub fn downloads(&self, num: usize) -> (String, &str) {
        match num {
            a @ 0..=99 => (format!("{}", a), ""),
            a @ 0..=500 => (format!("{}", a / 10 * 10), ""),
            a @ 0..=999 => (format!("{}", a / 50 * 50), ""),
            a @ 0..=9999 => (format!("{}.{}", a / 1000, a % 1000 / 100), "K"),
            a @ 0..=999_999 => (format!("{}", a / 1000), "K"),
            a => (format!("{}.{}", a / 1_000_000, a % 1_000_000 / 100_000), "M"),
        }
    }

    pub fn format_number(&self, num: impl Display) -> String {
        Numeric::english().format_int(num)
    }

    // version, deps, normalized popularity 0..100
    pub fn version_breakdown(&self) -> Vec<(SemVer, u16, f32)> {
        let mut ver: Vec<_> = self.stats.versions.iter().map(|(k, v)| {
            (k.to_semver(), *v, 0.)
        }).collect();

        let max = ver.iter().map(|(_, n, _)| *n).max().unwrap_or(1) as f32;
        ver.iter_mut().for_each(|i| i.2 = i.1 as f32 / max * 100.0);
        ver.sort_by(|a, b| b.0.cmp(&a.0));
        ver
    }

    pub fn page(&self) -> Page {
        Page {
            title: format!("Reverse dependencies of {}", self.ver.short_name()),
            item_name: Some(self.ver.short_name().to_string()),
            item_description: self.ver.description().map(|d| d.to_string()),
            noindex: true,
            search_meta: false,
            critical_css_data: Some(include_str!("../../style/public/revdeps.css")),
            critical_css_dev_url: Some("/revdeps.css"),
            ..Default::default()
        }
    }
}
