use lab::Lab;
use rich_crate::DownloadWeek;

/// Data for SVG on the crate page showing weekly download counts
///
/// The idea behind the graph is to show popularity of the crate at a first glance.
///
/// That's actually super hard when usage of crates varies by 5 orders of magnitude,
/// so no single scale will work.
///
/// Technically it's an awful chart with unlabelled scale and unlabelled axes.
/// Hopefully that's fine, because actual number is shown rigth below the
/// graph, so more numbers on the graph would just be distracting.
pub struct DownloadsGraph {
    exp: u32,
    is_bin: bool,
    scale: usize,
    pub width: usize,
    pub height: usize,
    pub data: Vec<DownloadWeek>,
}

impl DownloadsGraph {
    pub fn new(data: Vec<DownloadWeek>, is_bin: bool, width: usize, height: usize) -> Self {
        // scale from all data is fine, will demote crates that lost popularity
        let (exp, scale) = Self::downloads_scale(&data);
        Self {
            exp, scale, data, is_bin, width, height,
        }
    }

    /// Lines in the background of the chart
    pub fn ticks(&self) -> Vec<(f32, f32)> {
        let chart_y = 1;
        let chart_height = self.height - 1;
        match self.exp {
            0..=2 => {
                // When values are very low it seems nice to reuse the tick line to show maximum
                let max = self.data.iter().map(|d|d.total).max().unwrap_or(0);
                vec![
                    (chart_y as f32 + chart_height as f32 - (max * chart_height) as f32 / self.scale as f32 - 2., 0.5)
                ]
            },
            x => {
                let num_ticks = (x - 1) as usize;
                let thick = 8. / (7. + num_ticks as f32);
                (0..num_ticks).map(|n| {
                    (chart_y as f32 + ((chart_height * (n+1)) as f32 / (num_ticks+1) as f32),
                        if num_ticks > 8 && (n+100 - num_ticks/2) %4==0 {1.5} else {thick}
                    )
                }).collect()
            },
        }
    }

    /// Red/Blue/Green mix depending on number of downloads
    ///
    /// Binaries are allowed to have fewer downloads, because they're
    /// infrequent installations and not mere uses/cargo update.
    fn color_for_downloads(&self, value: usize) -> Lab {
        let low = Lab::from_rgb(&[255, 0, 0]);
        let hi = Lab::from_rgb(&[40, 220, 50]);
        let mid = Lab::from_rgb(&[20, 125, 250]);
        let max_expected = if self.is_bin { 3.1 } else { 4.0 }; // apps have it harder to get consistent stream of downloads
        let grad = (((value as f32).log10() - 1.0) / max_expected).max(0.).min(1.);

        if grad > 0.5 {
            Lab {
                l: 60.0 + grad * 10.0,
                a: mid.a * (2. - grad*2.) + hi.a * (grad*2.-1.),
                b: mid.b * (2. - grad*2.) + hi.b * (grad*2.-1.),
            }
        } else {
            Lab {
                l: 60.0 + grad * 10.0,
                a: low.a * (1. - grad*2.) + mid.a * grad*2.,
                b: low.b * (1. - grad*2.) + mid.b * grad*2.,
            }
        }
    }

    /// returns (x,y,width,height,color,label)
    /// TODO: make it a struct
    pub fn graph_data(&self) -> Vec<(usize, usize, usize, usize, String, String)> {
        let chart_x = 0;
        let chart_y = 0;
        let chart_width = self.width;
        let chart_height = self.height;

        let scale = self.scale;
        // max half year (but we have only 14 weeks of data anyway)
        let max_time_span = 26;
        let max_item_width = chart_width as f32 / max_time_span as f32 * 4.;

        let time_window = &self.data[self.data.len().saturating_sub(max_time_span)..];
        if time_window.is_empty() {
            return Vec::new();
        }
        let avg_value = time_window.iter().map(|d| d.total).sum::<usize>() / time_window.len();

        // bad rounding error
        let item_width = (chart_width as f32 / time_window.len() as f32).min(max_item_width);
        let left = chart_x + chart_width - (time_window.len() as f32 * item_width).floor() as usize;
        time_window.iter().enumerate()
        .filter(|(_,d)|d.total>0)
        .map(|(i,d)|{
            let blend = self.color_for_downloads((avg_value + d.total)/2);
            let age = i as f32 / time_window.len() as f32;
            let blend = Lab {
                l: blend.l + (1.- age) * 8.,
                a: blend.a * (0.5 + age/2.),
                b: blend.b * (0.5 + age/2.),
            };
            let color = blend.to_rgb();
                let color = format!("#{:02x}{:02x}{:02x}",
                    color[0],
                    color[1],
                    color[2],
                );
            let label = format!("{}/week @ {}", d.total, d.date.format("%Y-%m-%d"));
            let h = (d.total * chart_height + scale - 1) / scale;
            let overdraw = 1; // mix with border for style
            let left_tick = ((i as f32)*item_width).round() as usize;
            let right_tick = (((i+1) as f32)*item_width).round() as usize;
            (left + left_tick, chart_y + chart_height - h, right_tick - left_tick, h + overdraw, color.clone(), label)
        }).collect()
    }

    fn downloads_scale(data: &[DownloadWeek]) -> (u32, usize) {
        // + 100 keeps small values lower on all scales
        nice_round_number(data.iter().map(|d| d.total).max().unwrap_or(0) + 100)
    }
}

fn nice_round_number(n: usize) -> (u32, usize) {
    let defaults = [100, 250, 500, 1000, 1500, 3000, 5000, 10000, 20000];
    for (i, d) in defaults.iter().cloned().enumerate() {
        if n <= d {
            return (i as u32 + 1, d);
        }
    }
    let exp = (n as f64).log2().ceil() as u32;
    let max = (1 << exp) as usize;
    let rounded = if max >= 1_000_000 {
        max / 1_000_000 * 1_000_000
    } else if max >= 100_000 {
        max / 100_000 * 100_000
    } else if max >= 10000 {
        max / 10000 * 10000
    } else {
        max
    };
    if rounded >= n {
        (exp - 5, rounded) // -5 to keep continuity with hardcoded defaults
    } else {
        (exp - 5 + 1, rounded * 2)
    }
}

#[test]
fn dlscale() {
    assert_eq!(100, nice_round_number(11).1);
    assert_eq!(100, nice_round_number(100).1);
    assert_eq!(250, nice_round_number(101).1);
    assert_eq!(500, nice_round_number(333).1);
    assert_eq!(1000, nice_round_number(999).1);
    assert_eq!(1000, nice_round_number(1000).1);
    assert_eq!(1500, nice_round_number(1001).1);
    assert_eq!(16000000, nice_round_number(9999999).1);
    assert_eq!(16000000, nice_round_number(10000000).1);
}
