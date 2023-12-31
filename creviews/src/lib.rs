use crev_data::proof::content::CommonOps;
pub use crev_data::proof::Date;
pub use crev_data::Level;
pub use crev_data::Rating;
pub use crev_data::Version;
use crev_lib::{Error, Local};

pub mod security;

pub struct Creviews {
    crev_local: Local,
}

impl Creviews {
    pub fn new() -> Result<Self, Error> {
        let local = Local::auto_create_or_open()?;
        Ok(Self { crev_local: local })
    }

    pub fn update(&self) -> Result<(), Error> {
        self.crev_local.fetch_all()
    }

    pub fn reviews_for_crate(&self, crate_name: &str) -> Result<Vec<Review>, Error> {
        let db = self.crev_local.load_db()?;

        let mut reviews: Vec<_> = db.get_pkg_reviews_for_name("https://crates.io", crate_name).map(|r| {
            let (thoroughness, understanding, rating) = r
                .review()
                .map(|r| (r.thoroughness, r.understanding, r.rating))
                .unwrap_or_else(|| (Level::None, Level::None, Rating::Neutral));

            let from = r.from();
            let mut issues = Vec::new();
            for a in &r.advisories {
                issues.push(ReviewIssue {
                    ids: a.ids.clone(),
                    comment_markdown: a.comment.clone(),
                    severity: a.severity,
                });
            }
            for a in &r.issues {
                issues.push(ReviewIssue {
                    ids: vec![a.id.clone()],
                    comment_markdown: a.comment.clone(),
                    severity: a.severity,
                });
            }

            Review {
                author_id: from.id.to_string(),
                author_url: db.lookup_url(&from.id).verified().map(|u| u.url.to_string()),
                unmaintained: r.flags.unmaintained,
                version: r.package.id.version.clone(),
                thoroughness,
                understanding,
                rating,
                comment_markdown: r.comment.clone(),
                date: r.common.date,
                issues,
            }
        }).collect();

        reviews.sort_unstable_by(|a, b| b.author_url.is_some().cmp(&a.author_url.is_some())
            .then(b.version.cmp(&a.version))
            .then_with(|| b.date.cmp(&a.date)));

        Ok(reviews)
    }
}

pub struct Review {
    pub author_id: String,
    pub author_url: Option<String>,
    pub unmaintained: bool,
    pub version: Version,
    pub thoroughness: Level,
    pub understanding: Level,
    pub rating: Rating,
    pub comment_markdown: String,
    pub date: Date,
    pub issues: Vec<ReviewIssue>,
}

pub struct ReviewIssue {
    pub ids: Vec<String>,
    pub severity: Level,
    pub comment_markdown: String,
}
