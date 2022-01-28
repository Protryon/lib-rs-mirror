use crate::Page;
use kitchen_sink::CResult;
use kitchen_sink::DependencyKind;
use kitchen_sink::DependerChangesMonthly;
use kitchen_sink::KitchenSink;
use kitchen_sink::Origin;
use kitchen_sink::RevDependencies;
use kitchen_sink::SemVer;
use locale::Numeric;
use render_readme::Renderer;

use rich_crate::RichCrateVersion;
use semver::VersionReq;

use std::fmt::Display;

pub struct CratePageRevDeps<'a> {
    pub ver: &'a RichCrateVersion,
    pub deps: Vec<RevDepInf<'a>>,
    pub stats: Option<&'a RevDependencies>,
    pub has_download_columns: bool,
    downloads_by_ver: Vec<(SemVer, u32)>,
    changes: Vec<DependerChangesMonthly>,
}

#[derive(Debug, Default)]
pub(crate) struct DownloadsBar {
    pub num: u32,
    pub str: (String, &'static str),
    pub perc: f32,
    pub num_width: f32,
}

pub struct RevDepInf<'a> {
    pub origin: Origin,
    pub downloads: u32,
    pub depender: &'a kitchen_sink::Version,
    pub is_optional: bool,
    pub matches_latest: bool,
    pub kind: DependencyKind,
    pub req: VersionReq,
    pub rev_dep_count: u32,
}

pub(crate) struct DlRow {
    pub ver: SemVer,
    pub num: u16,
    pub num_str: String,
    pub perc: f32,
    pub num_width: f32,

    pub dl: DownloadsBar,
}

pub struct ChangesEntry {
    pub running_total: u32,
    pub added: u32,
    pub removed: u32,

    pub width: u16,
    pub running_totals_height: u16,
    pub added_height: u16,
    pub removed_height: u16,
    pub year: u16,

    pub label_inside: bool,
}

impl<'a> CratePageRevDeps<'a> {
    pub async fn new(ver: &'a RichCrateVersion, kitchen_sink: &'a KitchenSink, _markup: &'a Renderer) -> CResult<CratePageRevDeps<'a>> {
        let all_deps_stats = kitchen_sink.index.deps_stats().await?;
        let own_name = &ver.short_name().to_ascii_lowercase();
        let latest_stable_semver = &kitchen_sink.index.crate_highest_version(own_name, true)?.version().parse()?;
        let latest_unstable_semver = &kitchen_sink.index.crate_highest_version(own_name, false)?.version().parse()?;
        let stats = all_deps_stats.counts.get(own_name.as_str());

        let mut downloads_by_ver: Vec<_> = kitchen_sink.recent_downloads_by_version(ver.origin()).await?.into_iter().map(|(v, d)| (v.to_semver(), d)).collect();
        downloads_by_ver.sort_unstable_by(|a, b| b.0.cmp(&a.0));

        let mut deps: Vec<_> = match stats {
            Some(s) => futures::future::join_all(s.rev_dep_names.iter().map(|rev_dep| async move {
                let origin = Origin::from_crates_io_name(rev_dep);
                let downloads = kitchen_sink.downloads_per_month(&origin).await.ok().and_then(|x| x).unwrap_or(0) as u32;
                let depender = kitchen_sink.index.crate_highest_version(&rev_dep.to_ascii_lowercase(), true).expect("rev dep integrity");
                let (is_optional, req, kind) = depender.dependencies().iter().find(|d| {
                    own_name.eq_ignore_ascii_case(d.crate_name())
                })
                .map(|d| {
                    (d.is_optional(), d.requirement(), d.kind())
                })
                .unwrap_or_default();

                let req = req.parse().unwrap_or_else(|_| VersionReq::STAR);
                let matches_latest = req.matches(latest_stable_semver) || req.matches(latest_unstable_semver);

                RevDepInf {
                    origin,
                    depender, downloads, is_optional, req, kind,
                    matches_latest,
                    rev_dep_count: 0,
                }
            })).await,
            None => Vec::new(),
        };

        // sort by downloads if > 100, then by name
        deps.sort_by(|a, b| {
            b.downloads.max(100).cmp(&a.downloads.max(100))
            .then_with(|| {
                a.depender.name().cmp(b.depender.name())
            })
        });
        deps.truncate(1000);
        for d in deps.iter_mut() {
            d.rev_dep_count = all_deps_stats.counts.get(d.depender.name()).map(|s| s.direct.all()).unwrap_or(0);
        }
        let has_download_columns = deps.iter().any(|d| d.rev_dep_count > 0 || d.downloads > 100);

        let changes = kitchen_sink.depender_changes(ver.origin())?;
        Ok(Self {
            ver,
            deps,
            stats,
            downloads_by_ver,
            has_download_columns,
            changes,
        })
    }

    pub fn kind(&self, k: DependencyKind) -> &'static str {
        match k {
            DependencyKind::Normal => "normal",
            DependencyKind::Dev => "dev",
            DependencyKind::Build => "build",
        }
    }

    /// Nicely rounded number of downloads
    ///
    /// To show that these numbers are just approximate.
    pub fn downloads(&self, num: u32) -> (String, &'static str) {
        crate::format_downloads(num as _)
    }

    pub fn format_number(&self, num: impl Display) -> String {
        Numeric::english().format_int(num)
    }

    // version, deps, normalized popularity 0..100
    pub(crate) fn version_breakdown(&self) -> Vec<DlRow> {
        let stats = match self.stats {
            None => return Vec::new(),
            Some(s) => s,
        };

        let mut ver: Vec<_> = stats.versions.iter().map(|(k, v)| {
            DlRow {
                ver: k.to_semver(),
                num: *v,
                perc: 0.,
                num_width: 0.,
                num_str: String::new(),
                dl: DownloadsBar::default(),
            }
        }).collect();

        // Ensure the (latest) version is always included
        let own_ver_semver: SemVer = self.ver.version().parse().expect("semver2");
        if !ver.iter().any(|v| v.ver == own_ver_semver) {
            ver.push(DlRow {
                ver: own_ver_semver,
                num: 0,
                perc: 0.,
                num_width: 0.,
                num_str: String::new(),
                dl: DownloadsBar::default(),
            });
        }

        // Download data may be older and not match exactly, so at least avoid
        // accidentally omitting the most popular version
        if let Some(biggest) = self.downloads_by_ver.iter().max_by_key(|v| v.1) {
            if !ver.iter().any(|v| v.ver == biggest.0) {
                ver.push(DlRow {
                    ver: biggest.0.clone(),
                    num: 0,
                    perc: 0.,
                    num_width: 0.,
                    num_str: String::new(),
                    dl: DownloadsBar::default(),
                });
            }
        }

        // align selected versions and their (or older) downloads
        let mut dl_vers = self.downloads_by_ver.iter().rev().peekable();
        ver.sort_by(|a, b| b.ver.cmp(&a.ver));
        for curr in ver.iter_mut().rev() {
            let mut sum = 0;
            while let Some((next_ver, dl)) = dl_vers.peek() {
                if next_ver > &curr.ver {
                    break;
                }
                if next_ver.major == curr.ver.major &&
                (next_ver.major != 0 || next_ver.minor == curr.ver.minor) {
                    sum += dl;
                }
                dl_vers.next();
            }
            curr.dl.num = sum;
        }

        let max = ver.iter().map(|v| v.num).max().unwrap_or(1) as f32;
        let dl_max = ver.iter().map(|v| v.dl.num).max().unwrap_or(1) as f32;
        for i in ver.iter_mut() {
            i.perc = i.num as f32 / max * 100.0;
            i.num_str = self.format_number(i.num);
            i.num_width = 4. + 7. * i.num_str.len() as f32; // approx visual width of the number

            i.dl.perc = i.dl.num as f32 / dl_max * 100.0;
            i.dl.str = self.downloads(i.dl.num as u32);
            i.dl.num_width = 4. + 7. * (i.dl.str.0.len() + i.dl.str.1.len()) as f32; // approx visual width of the number
        }
        ver
    }

    /// entries + year and colspan
    pub fn changes_graph(&self) -> Option<(Vec<ChangesEntry>, Vec<(u16, u16)>)> {
        let total_max = self.changes.iter().map(|c| c.running_total()).max().unwrap_or(0).max(1);

        let good_data = (self.changes.len() >= 12 && total_max >= 15) || (self.changes.len() >= 6 && total_max >= 100);
        if !good_data {
            return None;
        }

        let added_removed_max = self.changes.iter().map(|c| c.added.max(c.removed + c.expired)).max().unwrap_or(0).max(18);

        let adds_chart_height = 20;
        let totals_chart_height = 80;
        let width = (800 / self.changes.len()).max(1).min(30) as _;

        let entries: Vec<_> = self.changes.iter().map(|ch| {
            let removed = ch.removed + ch.expired;
            let running_totals_height = (ch.running_total() * totals_chart_height) as f64 / total_max.max(50) as f64;
            ChangesEntry {
                running_total: ch.running_total(),
                added: ch.added,
                removed,
                width,
                running_totals_height: running_totals_height.round() as _,
                added_height: (ch.added * adds_chart_height / added_removed_max) as _,
                removed_height: (removed * adds_chart_height / added_removed_max) as _,
                year: ch.year,

                label_inside: ((ch.running_total() as f64).log10().ceil() * 5.8 + 7.) < running_totals_height,
            }
        }).collect();

        let mut years = Vec::with_capacity(entries.len() / 12 + 1);
        let mut curr_year = entries[0].year;
        let mut curr_year_months = 0;
        for e in &entries {
            if curr_year != e.year {
                years.push((curr_year, curr_year_months));
                curr_year = e.year;
                curr_year_months = 0;
            }
            curr_year_months += 1;
        }
        years.push((curr_year, curr_year_months));
        Some((entries, years))
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
