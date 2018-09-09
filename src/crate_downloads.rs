use parse_date;
use std::collections::BTreeMap;
use std::collections::HashMap;
use chrono::Duration;
use chrono::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateDownloadsFile {
    pub version_downloads: Vec<CrateVersionDailyDownload>,
    pub meta: CrateDownloadsExtra,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateDownloadsExtra {
    #[serde(default)]
    pub extra_downloads: Vec<CrateExtraDailyDownload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateVersionDailyDownload {
    id: usize,
    pub version: usize,
    pub downloads: usize,
    pub date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateExtraDailyDownload {
    pub downloads: usize,
    pub date: String,
}


#[derive(Debug, Clone)]
pub struct DownloadWeek {
    pub date: Date<Utc>,
    pub total: usize,
    pub downloads: Vec<(Option<usize>, usize)>,
}

impl CrateDownloadsFile {
    pub fn is_stale(&self) -> bool {
        self.version_downloads.iter()
            .max_by_key(|a| &a.date)
            .map(|max| {
                let date = parse_date(&max.date);
                (Utc::today() - date).num_weeks() > 4
            })
            .unwrap_or(true)
    }

    pub fn weekly_downloads(&self) -> Vec<DownloadWeek> {
        let ver_dl = &self.version_downloads;
        let other_dl = &self.meta.extra_downloads;

        let latest_date = parse_date(match (ver_dl.iter().map(|d| &d.date).max(), other_dl.iter().map(|d| &d.date).max()) {
            (Some(a), Some(b)) => a.max(b),
            (Some(any), None) | (None, Some(any)) => any,
            _ => return vec![],
        });

        let mut by_week = BTreeMap::<i64, HashMap<Option<usize>, usize>>::new();
        for ver_day in ver_dl {
            let date = parse_date(&ver_day.date);
            let weeksago = -(latest_date - date).num_weeks(); // negate to sort by oldest first
            *by_week.entry(weeksago).or_insert_with(HashMap::new)
                .entry(Some(ver_day.version)).or_insert(0)
                 += ver_day.downloads;
        }

        for day in other_dl {
            let date = parse_date(&day.date);
            let weeksago = -(latest_date - date).num_weeks(); // negate to sort by oldest first
            *by_week.entry(weeksago).or_insert_with(HashMap::new)
                .entry(None)
                .or_insert(0)
                += day.downloads;
        }

        // Btreemap guarantees sorted order
        by_week.into_iter().map(|(weeksago, downloads)| {
            let date = latest_date - Duration::weeks(-weeksago);
            DownloadWeek {
                date,
                total: downloads.values().sum(),
                downloads: downloads.into_iter().collect(),
            }
        }).collect()
    }
}

