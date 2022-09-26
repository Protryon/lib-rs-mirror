use categories::CATEGORIES;
use categories::Category;
use categories::CategoryMap;
use chrono::prelude::*;
use futures::future::try_join;
use crate::Page;
use crate::templates;
use crate::Urler;
use kitchen_sink::CompatByCrateVersion;
use kitchen_sink::KitchenSink;
use kitchen_sink::Origin;
use locale::Numeric;
use peeking_take_while::PeekableExt;
use rand::seq::SliceRandom;
use render_readme::Renderer;
use std::collections::HashMap;
use std::io::Write;

#[derive(Debug)]
pub struct GlobalStats {
    pub(crate) total_crate_num: u32,
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
    pub(crate) hs_rev_deps: Histogram,
    pub(crate) hs_maintenance: Histogram,
    pub(crate) hs_age: Histogram,
    pub(crate) hs_languish: Histogram,
    pub(crate) hs_owner_crates: Histogram,

    pub(crate) categories: Vec<TreeBox>,

    pub(crate) rustc_stats_all: Vec<Compat>,
    pub(crate) rustc_stats_recent: Vec<Compat>,
    pub(crate) rustc_stats_recent_num: usize,
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

fn downloads_over_time(start: Date<Utc>, mut day: Date<Utc>, kitchen_sink: &KitchenSink) -> Result<Vec<(u64, u64)>, anyhow::Error> {
    let mut current_year = 0;
    let mut current = [0; 366];
    let mut dl = Vec::new();
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
    Ok(dl)
}

pub async fn render_global_stats(out: &mut impl Write, kitchen_sink: &KitchenSink, _renderer: &Renderer) -> Result<(), anyhow::Error> {
    let (categories, recent_crates) = try_join(
        category_stats(kitchen_sink),
        kitchen_sink.notable_recently_updated_crates(4100)).await?;

    let urler = Urler::new(None);
    let start = Utc.ymd(2015, 5, 15); // Rust 1.0

    let start_week_offset = start.ordinal0()/7;
    let end = Utc::today() - chrono::Duration::days(2);

    let latest_rustc_version = end.signed_duration_since(start).num_weeks()/6;
    let mut compat_data = tokio::task::block_in_place(|| kitchen_sink.all_crate_compat())?;
    let rustc_stats_all = rustc_stats(&compat_data, latest_rustc_version as u16)?;
    let mut recent_compat = HashMap::with_capacity(recent_crates.len());
    let mut rustc_stats_recent_num = 0;
    for (o, _) in recent_crates {
        if let Some(v) = compat_data.remove(&o) {
            recent_compat.insert(o, v);
            rustc_stats_recent_num += 1;
            if rustc_stats_recent_num >= 4000 {
                break;
            }
        }
    }
    let rustc_stats_recent = rustc_stats(&recent_compat, latest_rustc_version as u16)?;

    let dl = downloads_over_time(start, end, kitchen_sink)?;

    let (total_owners_at_month, mut hs_owner_crates) = owner_stats(kitchen_sink, start).await?;
    hs_owner_crates.buckets.iter_mut().take(4).for_each(|c| c.examples.truncate(6)); // normal amount of crates is boring

    assert!(dl.len() >= 52*2);
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

    let rev_deps = kitchen_sink.crates_io_all_rev_deps_counts().await?;
    let mut hs_rev_deps = Histogram::new(rev_deps, true,
        &[0,1,2,5,15,50,100,250,500,750,1000,2500,5000,10000,15000,20000,50000],
        |n| if n > 2 {format!("≥{}", n)} else {n.to_string()});

    hs_rev_deps.buckets.iter_mut().take(5).for_each(|b| b.examples.truncate(5));

    let age_label = |n| match n {
        0..=1 => "≤1 week".to_string(),
        2..=4 => format!("≤{} weeks", n),
        5 => "≤1 month".to_string(),
        6..=51 => format!("≤{} months", (n as f64 / (365./12./7.)).round()),
        52 => "≤1 year".to_string(),
        _ => format!("≤{} years", (n as f64 / 52.).round()),
    };

    let total_crate_num = kitchen_sink.all_crates().count() as u32;
    let stats = GlobalStats {
        total_crate_num,
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
        hs_maintenance: Histogram::new(kitchen_sink.get_stats_histogram("maintenance")?.expect("hs_maintenance"), false, &[0, 1, 5, 26, 52, 52*2, 52*3, 52*5, 52*6, 52*8], |n| match n {
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
        categories,
        rustc_stats_all,
        rustc_stats_recent,
        rustc_stats_recent_num,
        hs_rev_deps,
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

#[derive(Default, Copy, Clone, Debug)]
pub struct Compat {
    pub(crate) bad: u32,
    pub(crate) maybe_bad: u32,
    pub(crate) unknown: u32,
    pub(crate) maybe_ok: u32,
    pub(crate) ok: u32,
}

impl Compat {
    pub fn sum(&self) -> u32 {
        self.bad + self.maybe_bad + self.unknown + self.maybe_ok + self.ok
    }
}

fn rustc_stats(compat: &HashMap<Origin, CompatByCrateVersion>, max_rust_version: u16) -> Result<Vec<Compat>, anyhow::Error> {
    // (ok, maybe, not), [0] is unused
    let mut rustc_versions = vec![Compat::default(); (max_rust_version+1) as usize];

    for (_, c) in compat {
        // can't compile at all
        if !c.iter().any(|(_, c)| c.has_ever_built()) {
            continue;
        }

        // stats for latest crate version only
        let latest_ver = match c.iter().rfind(|(v, _)| v.pre.is_empty()).or_else(|| c.iter().rev().nth(0)) {
            Some((_, c)) => c,
            None => continue,
        };
        let latest_ver_bad = match c.iter().rfind(|(v, c)| v.pre.is_empty() && c.newest_bad_likely().is_some()) {
            Some((_, c)) => c,
            None => latest_ver,
        };
        let newest_bad_raw = latest_ver_bad.newest_bad_likely().unwrap_or(0);
        let newest_bad = latest_ver.newest_bad().unwrap_or(0);
        let oldest_ok = latest_ver.oldest_ok().unwrap_or(999);
        let oldest_ok_raw = latest_ver.oldest_ok_certain().unwrap_or(999);
        for (ver, c) in rustc_versions.iter_mut().enumerate() {
            let ver = ver as u16;
            if ver >= oldest_ok {
                if ver >= oldest_ok_raw {
                    c.ok += 1;
                } else {
                    c.maybe_ok += 1;
                }
            } else if ver <= newest_bad {
                if ver <= newest_bad_raw {
                    c.bad += 1;
                } else {
                    c.maybe_bad += 1;
                }
            } else {
                c.unknown += 1;
            }
        }
    }

    // resize to width
    let width = 330;
    for c in &mut rustc_versions {
        let sum = c.sum();

        c.bad = (c.bad * width + width / 2) / sum;
        c.ok = (c.ok * width + width / 2) / sum;
        c.maybe_bad = (c.maybe_bad * width + width / 2) / sum;
        c.maybe_ok = (c.maybe_ok * width + width / 2) / sum;
        c.unknown = width - c.bad - c.ok - c.maybe_bad - c.maybe_ok;
    }
    Ok(rustc_versions)
}

fn cat_slugs(sub: &'static CategoryMap) -> Vec<TreeBox> {
    let mut out = Vec::with_capacity(sub.len());
    for c in sub.values() {
        if c.slug == "uncategorized" {
            continue;
        }
        out.push(TreeBox {
            cat: c,
            label: c.name.clone(),
            title: c.name.clone(),
            count: 0,
            weight: 0.,
            bounds: treemap::Rect::new(),
            color: String::new(),
            font_size: 12.,
            sub: cat_slugs(&c.sub),
        });
    }
    out
}

#[derive(Debug, Clone)]
pub struct TreeBox {
    pub cat: &'static Category,
    pub title: String,
    pub label: String,
    pub font_size: f64,
    /// SVG fill
    pub color: String,
    pub count: u32,
    pub weight: f64,
    pub bounds: treemap::Rect,
    pub sub: Vec<TreeBox>,
}

impl TreeBox {
    pub fn line_y(&self, nth: usize) -> f64 {
        self.bounds.y + 1. + self.font_size * 1.1 * (nth+1) as f64
    }
    pub fn can_fit_count(&self) -> bool {
        self.line_y(self.label.lines().count()) + 1. - self.bounds.y < self.bounds.h
    }
}

impl treemap::Mappable for TreeBox {
    fn size(&self) -> f64 { self.weight }
    fn bounds(&self) -> &treemap::Rect { &self.bounds }
    fn set_bounds(&mut self, b: treemap::Rect) { self.bounds = b; }
}

async fn category_stats(kitchen_sink: &KitchenSink) -> Result<Vec<TreeBox>, anyhow::Error> {
    use treemap::*;

    let mut roots = cat_slugs(&CATEGORIES.root);
    #[track_caller]
    fn take_cat(slug: &str, items: &mut Vec<TreeBox>) -> TreeBox {
        let pos = items.iter().position(|i| i.cat.slug == slug).unwrap_or_else(|| panic!("{} in {:?}", slug, items));
        items.swap_remove(pos)
    }
    #[track_caller]
    fn get_cat<'a>(slug: &str, items: &'a mut Vec<TreeBox>) -> &'a mut TreeBox {
        let pos = items.iter().position(|i| i.cat.slug == slug).unwrap_or_else(|| panic!("{} in {:?}", slug, items));
        &mut items[pos]
    }
    fn new_cat(sub: Vec<TreeBox>) -> TreeBox {
        TreeBox {
            cat: CATEGORIES.root.values().nth(0).unwrap(),
            title: String::new(),
            label: String::new(),
            font_size: 0.,
            color: String::new(),
            count: 0,
            weight: 0.,
            bounds: Rect::new(),
            sub,
        }
    }

    // names don't fit
    get_cat("database-implementations", &mut roots).label = "Database".into();
    get_cat("simulation", &mut roots).label = "Sim".into();
    get_cat("caching", &mut roots).label = "Cache".into();
    get_cat("config", &mut roots).label = "Config".into();
    get_cat("os", &mut roots).label = "OS".into();
    get_cat("internationalization", &mut roots).label = "i18n".into();
    get_cat("authentication", &mut roots).label = "Auth".into();
    get_cat("visualization", &mut roots).label = "Visualize".into();
    get_cat("accessibility", &mut roots).label = "a11y".into();
    get_cat("compilers", &mut roots).label = "Lang".into();
    get_cat("os::macos-apis", &mut get_cat("os", &mut roots).sub).label = "Apple".into();
    get_cat("rendering::engine", &mut get_cat("rendering", &mut roots).sub).label = "Engine".into();
    get_cat("rendering::data-formats", &mut get_cat("rendering", &mut roots).sub).label = "Formats".into();

    // group them in a more sensible way
    let parsers = vec![take_cat("parsing", &mut roots), take_cat("parser-implementations", &mut roots)];
    roots.push(new_cat(parsers));

    let hw = vec![take_cat("embedded", &mut roots), take_cat("hardware-support", &mut roots), take_cat("no-std", &mut roots)];
    roots.push(new_cat(hw));

    let db = vec![take_cat("database", &mut roots), take_cat("database-implementations", &mut roots)];
    roots.push(new_cat(db));

    let gg = vec![take_cat("game-development", &mut roots), take_cat("games", &mut roots)];
    roots.push(new_cat(gg));

    let int = take_cat("command-line-interface", &mut roots);
    let cli = vec![int, take_cat("command-line-utilities", &mut roots)];
    roots.push(new_cat(cli));

    let mut editors = take_cat("text-editors", &mut roots);
    editors.label = "Editors".into();
    let txt = vec![
        take_cat("text-processing", &mut roots),
        editors,
        take_cat("template-engine", &mut roots),
        take_cat("value-formatting", &mut roots),
    ];
    roots.push(new_cat(txt));

    let wasm = take_cat("wasm", &mut roots);
    get_cat("web-programming", &mut roots).sub.push(wasm);

    let mut asyn = take_cat("asynchronous", &mut roots);
    asyn.label = "Async".into();
    get_cat("network-programming", &mut roots).sub.push(asyn);

    let mut proc = take_cat("development-tools::procedural-macro-helpers", &mut get_cat("development-tools", &mut roots).sub);
    proc.label = "Proc macros".into();
    get_cat("rust-patterns", &mut roots).sub.push(proc);

    let concurrency = take_cat("concurrency", &mut roots);
    get_cat("rust-patterns", &mut roots).sub.push(concurrency);

    let mut cr = get_cat("cryptography", &mut roots).sub.remove(0);
    cr.label = "Crypto Magic Beans".into();
    roots.push(cr);

    // first layout of top-level boxes (won't be used for anything other than second layout)
    for top in roots.iter_mut() {
        let (count, weight) = if top.label == "" { (0, 0.) } else { kitchen_sink.category_crate_count(&top.cat.slug).await? };
        top.count = count;
        top.weight = weight;

        let mut top_copy = top.clone();
        top_copy.sub = Vec::new();

        for i in top.sub.iter_mut() {
            let (count, weight) = kitchen_sink.category_crate_count(&i.cat.slug).await?;
            i.count = count;
            i.weight = weight;
            top.count += i.count;
            top.weight += i.weight;
            assert!(i.sub.is_empty());
        }
        if top_copy.count > 0 {
            top.sub.insert(0, top_copy);
        }
    }

    let mut items_flattened = Vec::new();
    let layout = TreemapLayout::new();
    layout.layout_items(&mut roots, Rect::from_points(0.0, 0.0, 1000., 600.));

    for parent in roots.iter_mut() {
        let layout = TreemapLayout::new();
        layout.layout_items(&mut parent.sub, parent.bounds);
        items_flattened.append(&mut parent.sub);
    }

    postprocess_treebox_items(&mut items_flattened);

    Ok(items_flattened)
}

fn postprocess_treebox_items(items: &mut Vec<TreeBox>) {
    let colors = [
        [0xff, 0xf1, 0xe6],
        [0xe2, 0xec, 0xe9],
        [0xDC, 0xED, 0xC1],
        [0xcd, 0xda, 0xfd],
        [0xbe, 0xe1, 0xe6],
        [0xfd, 0xe2, 0xe4],
        [0xdf, 0xe7, 0xfd],
        [0xFF, 0xD3, 0xB6],
        [0xea, 0xe4, 0xe9],
        [0xd0, 0xd1, 0xff],
        [0xf4, 0xda, 0xe2],
        [0xde, 0xc3, 0xe1],
        [0xd4, 0xe0, 0xf9],
        [0xFF, 0xD3, 0xB6],
        [0xDF, 0xCB, 0xD2],
    ];
    let len = items.len() as f32;
    for (i, item) in &mut items.iter_mut().enumerate() {
        let x = 0.8 + (i as f32 / len) * 0.2;
        let c = colors[i % colors.len()];
        let c = [
            (c[0] as f32 * x + (1. - x) * 200.) as u8,
            (c[1] as f32 * x + (1. - x) * 100.) as u8,
            (c[2] as f32 * x + (1. - x) * 200.) as u8
        ];
        let mut l = lab::Lab::from_rgb(&c);
        l.l = (l.l + 90.) * 0.5; // fix my bad palette
        let c = l.to_rgb();
        item.color = format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]);

        let ideal_max_width = (item.bounds.w * 1.2 / (item.font_size / 1.7)) as usize;
        let maybe_label = textwrap::wrap(&item.label, textwrap::Options::new(ideal_max_width).break_words(false));

        let chars = maybe_label.iter().map(|w| w.len()).max().unwrap_or(1);
        let lines = maybe_label.len();
        let try_font_size = item.font_size
            .min(item.bounds.h / (lines as f64 * 1.05) - 4.)
            .min(item.bounds.w * 1.6 / chars as f64)
            .max(4.);

        let max_width = (item.bounds.w / (try_font_size / 1.7)) as usize;
        let must_break = ideal_max_width < chars * 2 / 3 && item.bounds.h > item.font_size * 2.;

        let label = textwrap::wrap(&item.label, textwrap::Options::new(max_width).break_words(must_break));
        let chars = label.iter().map(|w| w.len()).max().unwrap_or(1);
        let lines = label.len();
        item.label = label.join("\n");
        item.font_size = item.font_size
            .min(item.bounds.h / (lines as f64 * 1.05) - 4.)
            .min(item.bounds.w * 1.6 / chars as f64)
            .max(4.);
    }
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
                if !kitchen_sink.is_crates_io_login_on_shitlist(&login) { // github logins currently equal crates_io_logins
                    examples.push(login);
                    if examples.len() >= 10 {
                        break;
                    }
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

pub fn url_for_rev_deps(url: &Urler, name: &str) -> String {
    url.reverse_deps(&Origin::from_crates_io_name(name)).unwrap()
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
