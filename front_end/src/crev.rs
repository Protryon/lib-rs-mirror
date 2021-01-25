use crate::templates;
use crate::Page;
use kitchen_sink::KitchenSink;
use kitchen_sink::Level;
use kitchen_sink::Origin;
use kitchen_sink::Rating;
use kitchen_sink::Review;
use kitchen_sink::SemVer;
use render_readme::Links;
use render_readme::Markup;
use render_readme::Renderer;
use rich_crate::RichCrateVersion;
use std::borrow::Cow;
use std::collections::HashSet;

pub struct ReviewsPage<'a> {
    pub(crate) ver: &'a RichCrateVersion,
    pub(crate) markup: &'a Renderer,
    /// hide text, show non-latest warning, review
    pub(crate) reviews: Vec<(bool, bool, &'a Review)>,
    pub(crate) version: SemVer,
    pub(crate) cargo_crev_origin: Origin,
}

impl<'a> ReviewsPage<'a> {
    pub(crate) async fn new(reviews: &'a [Review], ver: &'a RichCrateVersion, _k: &'a KitchenSink, markup: &'a Renderer) -> ReviewsPage<'a> {
        let version: SemVer = ver.version().parse().expect("semver");
        let cargo_crev_origin = Origin::from_crates_io_name("cargo-crev");
        let mut author_seen = HashSet::new();
        let mut non_latest_shown = false;
        let reviews = reviews.iter().map(|r| {
            let non_latest = if !non_latest_shown && r.version < version {
                non_latest_shown = true;
                true
            } else {
                false
            };
            (!author_seen.insert(&r.author_url), non_latest, r)
        }).collect();
        Self {
            reviews,
            version,
            ver,
            markup,
            cargo_crev_origin,
        }
    }

    pub fn issue_url<'b>(&self, id: &'b str) -> Option<Cow<'b, str>> {
        if id.starts_with("https://") {
            return Some(id.into());
        }
        if id.starts_with("RUSTSEC-") {
            return Some(format!("https://github.com/RustSec/advisory-db/blob/HEAD/crates/{}/{}.toml", self.ver.short_name(), id).into());
        }
        None
    }

    pub fn issue_id<'b>(&self, id: &'b str) -> &'b str {
        id.trim_start_matches("https://")
    }

    pub fn rating_class(&self, rating: Rating) -> &str {
        match rating {
            Rating::Negative => "negative",
            Rating::Neutral => "neutral",
            Rating::Positive => "positive",
            Rating::Strong => "strong",
        }
    }

    pub fn rating_label(&self, rating: Rating) -> &str {
        match rating {
            Rating::Negative => "Negative",
            Rating::Neutral => "Neutral",
            Rating::Positive => "Positive",
            Rating::Strong => "Strong Positive",
        }
    }

    pub fn level_class(&self, level: Level) -> &str {
        match level {
            Level::None => "none",
            Level::Low => "low",
            Level::Medium => "medium",
            Level::High => "high",
        }
    }

    pub fn level_label(&self, level: Level) -> &str {
        match level {
            Level::None => "None",
            Level::Low => "Low",
            Level::Medium => "Medium",
            Level::High => "High",
        }
    }

    /// class, label
    pub fn version_compare(&self, other: &SemVer) -> (&str, &str) {
        if &self.version <= other {
            ("current", "(current)")
        } else if self.version.major > 0 && self.version.major == other.major && self.version.minor == other.minor {
            ("old", "")
        } else {
            ("outdated", "(older version)")
        }
    }

    pub fn page(&self) -> Page {
        Page {
            title: format!("Review{} of the {} crate", if self.reviews.len() == 1 { "" } else { "s" }, self.ver.capitalized_name()),
            item_name: Some(self.ver.short_name().to_string()),
            item_description: self.ver.description().map(|d| d.to_string()),
            noindex: self.reviews.is_empty(),
            search_meta: false,
            critical_css_data: Some(include_str!("../../style/public/crev.css")),
            critical_css_dev_url: Some("/crev.css"),
            ..Default::default()
        }
    }

    pub fn author_name<'b>(&self, review: &'b Review) -> Cow<'b, str> {
        if let Some(url) = review.author_url.as_ref() {
            let url = url.trim_start_matches("https://");
            if url.starts_with("github.com/") {
                let mut parts = url.split('/');
                if let Some(name) = parts.nth(1) {
                    return name.into();
                }
            }
            let url = url.trim_end_matches("/crev-proofs");
            url.into()
        } else {
            format!("Untrusted source ({})", review.author_id).into()
        }
    }

    pub fn render_comment(&self, markdown: &str) -> templates::Html<String> {
        let (html, warnings) = self.markup.page(&Markup::Markdown(markdown.to_string()), None, Links::Ugc, None);
        if !warnings.is_empty() {
            eprintln!("{} creview: {:?}", self.ver.short_name(), warnings);
        }
        templates::Html(html)
    }
}
