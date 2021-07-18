use kitchen_sink::Origin;
use locale::Numeric;
use chrono::prelude::*;
use crate::Page;
use crate::templates;
use crate::Urler;
use kitchen_sink::KitchenSink;
use render_readme::Renderer;
use std::collections::HashMap;
use std::io::Write;
use peeking_take_while::PeekableExt;
use rand::seq::SliceRandom;

#[derive(Debug)]
pub struct GlobalStats {
    pub(crate) total_owners_at_month: Vec<u32>,
    pub(crate) max_total_owners: u32,
    pub(crate) max_daily_downloads_rate: u32,
    pub(crate) max_downloads_per_week: u64,
    pub(crate) start_week_offset: u32,
    pub(crate) dl_grid_line_every: u64,
    pub(crate) weeks_to_reach_max_downloads: u32,
    pub(crate) dl_per_day_this_year: (u64, u64),
    pub(crate) dl_per_day_last_year: (u64, u64),


    pub(crate) hs_releases: Histogram,
    pub(crate) hs_sizes: Histogram,
    pub(crate) hs_deps1: Histogram,
    pub(crate) hs_deps2: Histogram,
    pub(crate) hs_maintenance: Histogram,
    pub(crate) hs_age: Histogram,
    pub(crate) hs_languish: Histogram,
    pub(crate) hs_owner_crates: Histogram,
}

pub type CallbackFn = fn(&Urler, &str) -> String;

impl GlobalStats {
    pub fn relative_increase(val: (u64, u64)) -> String {
        format!("{:.1}×", val.0 as f64 / val.1 as f64)
    }

    pub fn dl_ratio_up(&self) -> bool {
        let r1 = self.dl_per_day_this_year.0 as f64 / self.dl_per_day_this_year.1 as f64;
        let r2 = self.dl_per_day_last_year.0 as f64 / self.dl_per_day_last_year.1 as f64;
        r1 > r2
    }
}

pub async fn render_global_stats(out: &mut impl Write, kitchen_sink: &KitchenSink, _renderer: &Renderer) -> Result<(), anyhow::Error> {
    let urler = Urler::new(None);
    let start = Utc.ymd(2015, 5, 15); // Rust 1.0

    let start_week_offset = start.ordinal0()/7;
    let mut day = Utc::today() - chrono::Duration::days(2);

    let mut current_year = 0;
    let mut current = [0; 366];

    let mut dl = Vec::new();

    // skip over potentially missing data
    while day > start {
        let year = day.year() as u16;
        if year != current_year {
            current_year = year;
            current = kitchen_sink.total_year_downloads(current_year)?;
        }
        let n = current[day.ordinal0() as usize];
        if n > 0 {
            break;
        }
        day = day - chrono::Duration::days(1);
    }

    // going from the end ensures last data point always has a full week
    while day > start {
        let mut weekday_sum = 0;
        let mut weekend_sum = 0;
        for _ in 0..7 {
            let year = day.year() as u16;
            if year != current_year {
                current_year = year;
                current = kitchen_sink.total_year_downloads(current_year)?;
            }
            let n = current[day.ordinal0() as usize];
            match day.weekday() {
                // this sucks a bit due to mon/fri being UTC, and overlapping with the weekend
                // in the rest of the world.
                Weekday::Sat | Weekday::Sun => weekend_sum += n,
                _ => weekday_sum += n,
            };
            day = day - chrono::Duration::days(1);
        }
        dl.push((weekday_sum, weekend_sum));
    }
    dl.reverse();

    let (total_owners_at_month, mut hs_owner_crates) = owner_stats(kitchen_sink, start).await?;
    hs_owner_crates.buckets.iter_mut().take(4).for_each(|c| c.examples.truncate(6)); // normal amount of crates is boring

    eprintln!("{:?} {:?}", total_owners_at_month, hs_owner_crates);

    let this_year = &dl[dl.len()-52..];
    let last_year = &dl[dl.len()-52*2..dl.len()-52];

    fn sum2(s: &[(u64, u64)]) -> (u64, u64) {
        let mut a_sum = 0;
        let mut b_sum = 0;
        s.iter().for_each(|&(a, b)| { a_sum += a; b_sum += b; });
        (a_sum, b_sum)
    }
    let max_daily_downloads_rate = this_year.iter().map(move |(d, e)| (d/5).max(e/2)).max().unwrap_or(0) as u32;
    let mut tmp_sum = 0;
    let downloads_this_year = sum2(this_year);
    let downloads_last_year = sum2(last_year);
    let max_downloads_per_week = dl.iter().map(|(a, b)| a + b).max().unwrap_or(0);
    let max_total_owners = total_owners_at_month.iter().copied().max().unwrap_or(0);
    let dl_grid_line_every = (max_downloads_per_week / 6_000_000) * 1_000_000;
    let mut hs_deps1 = Histogram::new(kitchen_sink.get_stats_histogram("deps")?.expect("hs_deps"), true,
        &[0,1,2,3,4,5,6,7,8,9,10,11,12,14,16,18,20,25,30,40,60,80,100,120,150],
        |n| if n > 11 {format!("≥{}", n)} else {n.to_string()});
    let hs_deps2 = Histogram {
        max: hs_deps1.max,
        buckets: hs_deps1.buckets.split_off(10),
        bucket_labels: hs_deps1.bucket_labels.split_off(10),
    };

    let age_label = |n| match n {
        0..=1 => "≤1 week".to_string(),
        2..=4 => format!("≤{} weeks", n),
        5 => "≤1 month".to_string(),
        6..=51 => format!("≤{} months", (n as f64 / (365./12./7.)).round()),
        52 => "≤1 year".to_string(),
        _ => format!("≤{} years", (n as f64 / 52.).round()),
    };

    let stats = GlobalStats {
        total_owners_at_month,
        max_total_owners,
        max_daily_downloads_rate,
        start_week_offset,
        weeks_to_reach_max_downloads: dl.iter().copied().take_while(move |(d, e)| { tmp_sum += (d + e) as u32; tmp_sum < max_daily_downloads_rate }).count() as u32,
        dl_per_day_this_year: (downloads_this_year.0 / 5, downloads_this_year.1 / 2),
        dl_per_day_last_year: (downloads_last_year.0 / 5, downloads_last_year.1 / 2),
        max_downloads_per_week,
        dl_grid_line_every,

        hs_releases: Histogram::new(kitchen_sink.get_stats_histogram("releases")?.expect("hs_releases"), true, &[1,2,4,8,16,32,50,100,500], |n| if n > 2 {format!("≥{}", n)} else {n.to_string()}),
        hs_sizes: Histogram::new(kitchen_sink.get_stats_histogram("sizes")?.expect("hs_sizes"), true, &[1,10,50,100,500,1_000,5_000,10_000,20_000], |n| {
            let mut t = format_bytes(n*1024);
            t.insert(0, '≤'); t
        }),
        hs_deps1, hs_deps2,
        hs_maintenance: Histogram::new(kitchen_sink.get_stats_histogram("maintenance")?.expect("hs_maintenance"), false, &[0, 1, 5, 26, 52, 52*2, 52*3, 52*5, 52*7, 52*9], |n| match n {
            0 => "one-off".to_string(),
            1 => "≤1 week".to_string(),
            2..=4 => format!("≤{} weeks", n),
            5 => "≤1 month".to_string(),
            6..=51 => format!("≤{} months", (n as f64 / (365./12./7.)).round()),
            52 => "≤1 year".to_string(),
            _ => format!("≤{} years", (n as f64 / 52.).round()),
        }),
        hs_age: Histogram::new(kitchen_sink.get_stats_histogram("age")?.expect("hs_age"), false, &[5, 13, 26, 52, 52*2, 52*3, 52*4, 52*5, 52*6, 52*8], age_label),
        hs_languish: Histogram::new(kitchen_sink.get_stats_histogram("languish")?.expect("hs_languish"), false, &[5, 13, 26, 52, 52*2, 52*3, 52*4, 52*5, 52*6, 52*8], age_label),
        hs_owner_crates,
    };

    templates::global_stats(out, &Page {
        title: "State of the Rust/Cargo crates ecosystem".to_owned(),
        description: Some("How many packages there are? How many dependencies they have? Which crate is the oldest or biggest? Is Rust usage growing?".to_owned()),
        noindex: false,
        search_meta: true,
        critical_css_data: Some(include_str!("../../style/public/home.css")),
        critical_css_dev_url: Some("/home.css"),
        ..Default::default()
    }, &dl, &stats, &urler)?;
    Ok(())
}

async fn owner_stats(kitchen_sink: &KitchenSink, start: Date<Utc>) -> Result<(Vec<u32>, Histogram), anyhow::Error> {
    let all_owners = kitchen_sink.crate_all_owners().await?;
    eprintln!("got {} owners", all_owners.len());
    assert!(all_owners.len() > 1000);
    let mut owner_crates_with_ids = HashMap::new();
    let mut total_owners_at_month = vec![0u32; (Utc::today().signed_duration_since(start).num_days() as usize + 29) / 30];
    let mut sum = 0;
    for o in &all_owners {
        // account creation history
        let (y,m,_d) = o.created_at;
        if y < 2015 || (y == 2015 && m < 5) {
            sum += 1;
            continue;
        }
        let mon_num = (y as usize - 2015) * 12 + m as usize - 5;
        if mon_num < total_owners_at_month.len() {
            total_owners_at_month[mon_num as usize] += 1;
        }
        // update histogram
        let t = owner_crates_with_ids.entry(o.num_crates).or_insert((0, Vec::<u64>::new()));
        t.0 += 1;
        if t.1.len() < 1000 {
            t.1.push(o.github_id);
        }
    }

    // convert IDs to logins
    let owner_crates = owner_crates_with_ids.into_iter().map(|(k, (pop, mut id_examples))| {
        let mut examples = Vec::with_capacity(id_examples.len().min(10));
        if k <= 50 {
            id_examples.sort_unstable(); // promote low-id users for normal amount of crates
        } else {
            id_examples.sort_by_key(|v| !v); // show newest users for potentially-spammy crate sets
        }
        // but include one counter-example just to make things more interesting
        if let Some(tmp) = id_examples.pop() {
            id_examples.insert(0, tmp);
        }
        for id in id_examples {
            if let Ok(login) = kitchen_sink.login_by_github_id(id) {
                examples.push(login);
                if examples.len() > 8 {
                    break;
                }
            }
        }
        (k, (pop, examples))
    }).collect();

    // trim empty end
    while total_owners_at_month.last().map_or(false, |&l| l == 0) {
        total_owners_at_month.pop();
    }
    total_owners_at_month.iter_mut().for_each(|n| {
        sum += *n;
        *n = sum;
    });
    let hs_owner_crates = Histogram::new(owner_crates, true, &[1,2,3,6,25,50,75,100,150,200,500,750,2000], |n| if n > 3 {format!("≥{}", n)} else {n.to_string()});
    Ok((total_owners_at_month, hs_owner_crates))
}

#[derive(Debug)]
pub struct Histogram {
    pub max: u32,
    pub buckets: Vec<Bucket>,
    pub bucket_labels: Vec<String>,
}

#[derive(Debug)]
pub struct Bucket {
    /// population
    pub count: u32,
    pub threshold: u32,
    pub examples: Vec<String>,
}

impl Bucket {
    pub fn new(threshold: u32) -> Self {
        Self { threshold, count: 0, examples: Vec::with_capacity(BUCKET_MAX_EXAMPLES) }
    }
}

const BUCKET_MAX_EXAMPLES: usize = 25;

impl Histogram {
    pub fn perc(&self, val: u32) -> f32 {
        val as f32 / (self.max as f32 / 100.)
    }

    /// greater_mode - bucket means this many or more, otherwise it's <=
    ///
    pub fn new(data: kitchen_sink::StatsHistogram, greater_mode: bool, bucket_thresholds: &[u32], label: fn(u32) -> String) -> Self {
        let mut data: Vec<_> = data.into_iter().collect();
        data.sort_unstable_by_key(|d| d.0);
        let mut data = data.drain(..).fuse().peekable();

        fn make_bucket(mut b: Bucket, (key, (size, mut val)): (u32, (u32, Vec<String>))) -> Bucket {
            debug_assert!(size as usize >= val.len());
            b.count += size;
            if b.examples.len() < BUCKET_MAX_EXAMPLES {
                b.examples.append(&mut val);
            }
            if key > b.threshold {
                b.threshold = key;
            }
            b
        }

        let mut buckets: Vec<_> = bucket_thresholds.windows(2)
            .map(|thr_pair| (thr_pair[0], thr_pair[1]))
            .chain(std::iter::once((bucket_thresholds.last().copied().unwrap(), !0)))
            .map(|(threshold, next_thr)| {
            let mut b = data.by_ref()
                .peeking_take_while(|d| if greater_mode {
                    d.0 < next_thr
                } else {
                    d.0 <= threshold
                })
                .fold(Bucket::new(0), make_bucket);
            if greater_mode {
                b.threshold = threshold;
            } else {
                // round threshold to max if close, otherwise show actual
                if b.threshold / 9 > threshold / 10 {
                    b.threshold = threshold;
                }
            }
            b.examples.shuffle(&mut rand::thread_rng());
            b
        })
        .filter(|bucket| bucket.count > 0)
        .collect();

        let other = data.fold(Bucket::new(0), make_bucket);
        if other.count > 0 {
            buckets.push(other);
        }

        Self {
            max: buckets.iter().map(|b| b.count).max().unwrap_or(0),
            bucket_labels: buckets.iter().map(|b| label(b.threshold)).collect(),
            buckets,
        }
    }
}

pub fn url_for_crate_name(url: &Urler, name: &str) -> String {
    url.crate_by_origin(&Origin::from_crates_io_name(name))
}

pub fn versions_for_crate_name(url: &Urler, name: &str) -> String {
    url.all_versions(&Origin::from_crates_io_name(name)).unwrap()
}

pub fn format_number(num: u32) -> String {
    Numeric::english().format_int(num)
}

pub fn format_bytes(bytes: u32) -> String {
    let (num, unit) = match bytes {
        0..=1_000_000 => ((bytes + 999) / 1024, "KB"),
        0..=9_999_999 => return format!("{}MB", ((bytes + 250_000) / 500_000) as f64 * 0.5),
        _ => ((bytes + 500_000) / 1_000_000, "MB"),
    };
    format!("{}{}", Numeric::english().format_int(num), unit)
}
