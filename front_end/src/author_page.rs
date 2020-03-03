use crate::Page;
use crate::templates;
use futures::stream::StreamExt;
use kitchen_sink::CrateOwnerRow;
use kitchen_sink::CResult;
use kitchen_sink::KitchenSink;
use kitchen_sink::RichAuthor;
use kitchen_sink::RichCrateVersion;
use kitchen_sink::UserOrg;
use kitchen_sink::UserType;
use render_readme::Renderer;
use std::sync::Arc;

// pub struct User {
//     pub id: u32,
//     pub login: String,
//     pub name: Option<String>,
//     pub avatar_url: Option<String>,  // "https://avatars0.githubusercontent.com/u/1111?v=4",
//     pub gravatar_id: Option<String>, // "",
//     pub html_url: String,            // "https://github.com/zzzz",
//     pub blog: Option<String>,        // "https://example.com
//     #[serde(rename = "type")]
//     pub user_type: UserType,
// }

/// Data sources used in `author.rs.html`
pub struct AuthorPage<'a> {
    pub aut: &'a RichAuthor,
    pub kitchen_sink: &'a KitchenSink,
    pub markup: &'a Renderer,
    pub crates: Vec<(Arc<RichCrateVersion>, CrateOwnerRow)>,
    pub orgs: Vec<UserOrg>,
}

impl<'a> AuthorPage<'a> {
    pub async fn new(aut: &'a RichAuthor, kitchen_sink: &'a KitchenSink, markup: &'a Renderer) -> CResult<AuthorPage<'a>> {
        dbg!(&aut);
        let orgs = kitchen_sink.user_github_orgs(&aut.github.login).await?.unwrap_or_default();
        let mut rows = kitchen_sink.crates_of_author(aut).await?;
        rows.sort_by(|a,b| b.latest_release.cmp(&a.latest_release));
        rows.truncate(200);
        dbg!(&rows);

        let crates = futures::stream::iter(rows.into_iter())
            .filter_map(|row| async move {
                let c = kitchen_sink.rich_crate_version_async(&row.origin).await.map_err(|e| eprintln!("{}", e)).ok()?;
                Some((c, row))
            })
            .collect().await;

        Ok(Self {
            crates,
            aut,
            kitchen_sink,
            markup,
            orgs,
        })
    }

    pub fn is_org(&self) -> bool {
        self.aut.github.user_type == UserType::Org
    }

    pub fn login(&self) -> &str {
        &self.aut.github.login
    }

    pub fn github_url(&self) -> String {
        format!("https://github.com/{}", self.aut.github.login)
    }

    pub fn page(&self) -> Page {
        Page {
            title: format!("Rust crates by @{}", self.login()),
            ..Default::default()
        }
    }

    pub fn render_markdown_str(&self, s: &str) -> templates::Html<String> {
        templates::Html(self.markup.markdown_str(s, true, None))
    }

    // fn block<O>(&self, f: impl Future<Output = O>) -> O {
    //     self.handle.enter(|| futures::executor::block_on(f))
    // }

    // pub fn format_number(&self, num: impl Display) -> String {
    //     Numeric::english().format_int(num)
    // }

    // pub fn format_knumber(&self, num: usize) -> (String, &'static str) {
    //     let (num, unit) = match num {
    //         0..=899 => (num, ""),
    //         0..=8000 => return (format!("{}", ((num + 250) / 500) as f64 * 0.5), "K"), // 3.5K
    //         0..=899_999 => ((num + 500) / 1000, "K"),
    //         0..=9_999_999 => return (format!("{}", ((num + 250_000) / 500_000) as f64 * 0.5), "M"), // 3.5M
    //         _ => ((num + 500_000) / 1_000_000, "M"),                                                // 10M
    //     };
    //     (Numeric::english().format_int(num), unit)
    // }

    // pub fn format_kbytes(&self, bytes: usize) -> String {
    //     let (num, unit) = match bytes {
    //         0..=100_000 => ((bytes + 999) / 1000, "KB"),
    //         0..=800_000 => ((bytes + 3999) / 5000 * 5, "KB"),
    //         0..=9_999_999 => return format!("{}MB", ((bytes + 250_000) / 500_000) as f64 * 0.5),
    //         _ => ((bytes + 500_000) / 1_000_000, "MB"),
    //     };
    //     format!("{}{}", Numeric::english().format_int(num), unit)
    // }

    // fn format_number_frac(num: f64) -> String {
    //     if num > 0.05 && num < 10. && num.fract() > 0.09 && num.fract() < 0.9 {
    //         if num < 3. {
    //             format!("{:.1}", num)
    //         } else {
    //             format!("{}", (num * 2.).round() / 2.)
    //         }
    //     } else {
    //         Numeric::english().format_int(if num > 500. {
    //             (num / 10.).round() * 10.
    //         } else if num > 100. {
    //             (num / 5.).round() * 5.
    //         } else {
    //             num.round()
    //         })
    //     }
    // }

    // pub fn format_kbytes_range(&self, a: usize, b: usize) -> String {
    //     let min_bytes = a.min(b);
    //     let max_bytes = a.max(b);

    //     // if the range is small, just display the upper number
    //     if min_bytes * 4 > max_bytes * 3 || max_bytes < 250_000 {
    //         return self.format_kbytes(max_bytes);
    //     }

    //     let (denom, unit) = match max_bytes {
    //         0..=800_000 => (1000., "KB"),
    //         _ => (1_000_000., "MB"),
    //     };
    //     let mut low_val = min_bytes as f64 / denom;
    //     let high_val = max_bytes as f64 / denom;
    //     if low_val > 1. && high_val > 10. {
    //         low_val = low_val.round(); // spread is so high that precision of low end isn't relevant
    //     }
    //     format!("{}–{}{}", Self::format_number_frac(low_val), Self::format_number_frac(high_val), unit)
    // }

    // /// Display number 0..1 as percent
    // pub fn format_fraction(&self, num: f64) -> String {
    //     if num < 1.9 {
    //         format!("{:0.1}%", num)
    //     } else {
    //         format!("{}%", Numeric::english().format_int(num.round() as usize))
    //     }
    // }

    // pub fn format(date: &DateTime<FixedOffset>) -> String {
    //     date.format("%b %e, %Y").to_string()
    // }

    // pub fn format_month(date: &DateTime<FixedOffset>) -> String {
    //     date.format("%b %Y").to_string()
    // }
}
