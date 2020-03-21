use chrono::prelude::*;
use crate::Page;
use crate::templates;
use crate::url_domain;
use futures::stream::StreamExt;
use kitchen_sink::CrateOwnerRow;
use kitchen_sink::CResult;
use kitchen_sink::KitchenSink;
use kitchen_sink::Org;
use kitchen_sink::OwnerKind;
use kitchen_sink::RichAuthor;
use kitchen_sink::RichCrateVersion;
use kitchen_sink::User;
use kitchen_sink::UserType;
use render_readme::Renderer;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::Arc;

pub struct OtherOwner {
    github_id: u32,
    login: String,
    invited_by_github_id: Option<u32>,
    invited_at: DateTime<Utc>,
    kind: OwnerKind,
}

/// Data sources used in `author.rs.html`
pub struct AuthorPage<'a> {
    pub(crate) aut: &'a RichAuthor,
    pub(crate) markup: &'a Renderer,
    pub(crate) founder_crates: Vec<(Arc<RichCrateVersion>, u32, CrateOwnerRow, Vec<OtherOwner>)>,
    pub(crate) member_crates: Vec<(Arc<RichCrateVersion>, u32, CrateOwnerRow, Vec<OtherOwner>)>,
    pub(crate) orgs: Vec<Org>,
    pub(crate) joined: DateTime<Utc>,
    pub(crate) founder_total: usize,
    pub(crate) member_total: usize,
    pub(crate) keywords: Vec<String>,
    pub(crate) collab: Vec<User>,
}

impl<'a> AuthorPage<'a> {
    pub async fn new(aut: &'a RichAuthor, kitchen_sink: &'a KitchenSink, markup: &'a Renderer) -> CResult<AuthorPage<'a>> {
        let orgs = kitchen_sink.user_github_orgs(&aut.github.login).await?.unwrap_or_default();
        let orgs = futures::stream::iter(orgs).filter_map(|org| async move {
            kitchen_sink.github_org(&org.login).await
                .map_err(|e| eprintln!("org: {} {}", &org.login, e))
                .ok().and_then(|x| x)
        })
        .collect().await;
        let rows = kitchen_sink.crates_of_author(aut).await?;
        let joined = rows.iter().filter_map(|row| row.invited_at).min().expect("no crates?");

        let (mut founder, mut member): (Vec<_>, Vec<_>) = rows.into_iter().partition(|c| c.invited_by_github_id.is_none());
        let founder_total = founder.len();
        let member_total = member.len();
        founder.sort_by(|a,b| b.latest_release.cmp(&a.latest_release));
        founder.truncate(200);

        member.sort_by(|a,b| b.crate_ranking.partial_cmp(&a.crate_ranking).unwrap_or(Ordering::Equal));
        member.truncate(200);

        let founder_crates = Self::look_up(kitchen_sink, founder).await;
        let member_crates = Self::look_up(kitchen_sink, member).await;

        // Most common keywords
        let mut keywords = HashMap::new();
        let now = Utc::now();
        // Most collaborated with
        let mut collab = HashMap::new();
        for (c, _, row, all_owners) in founder_crates.iter().chain(member_crates.iter()) {
            for (i, k) in c.keywords().iter().enumerate() {
                // take first(-ish) keyword from each crate to avoid one crate taking most
                *keywords.entry(k).or_insert(0.) += (row.crate_ranking + 0.5) / (i+2) as f32;
            }

            if let Some(own) = all_owners.iter().find(|o| o.github_id == aut.github.id) {
                let oldest = all_owners.iter().map(|o| o.invited_at).min().unwrap();
                // max discounts young crates
                let max_days = now.signed_duration_since(oldest).num_days().max(30*6) as f32;
                let own_days = now.signed_duration_since(own.invited_at).num_days();
                for o in all_owners {
                    if o.github_id == aut.github.id || o.kind != OwnerKind::User {
                        continue;
                    }
                    // How long co-owned together, relative to crate's age
                    let overlap = now.signed_duration_since(o.invited_at).num_days().min(own_days) as f32 / max_days;
                    let relationship = if own.invited_by_github_id == Some(o.github_id) {4.}
                        else if o.invited_by_github_id == Some(own.github_id) {2.} else {1.};
                    collab.entry(o.github_id).or_insert((0., &o.login)).0 += (row.crate_ranking + 0.5) * overlap * relationship;
                }
            }
        }
        let mut keywords: Vec<_> = keywords.into_iter().collect();
        keywords.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
        let keywords: Vec<_> = keywords.into_iter().take(7).map(|(k, _)| k.to_owned()).collect();

        let mut collab: Vec<_> = collab.into_iter().map(|(_, v)| v).collect();
        collab.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(Ordering::Equal));
        let collab: Vec<_> = futures::stream::iter(collab.into_iter().take(100)).filter_map(|(_, login)| async move {
            kitchen_sink.user_by_github_login(login).await.map_err(|e| eprintln!("{}: {}", login, e)).ok().and_then(|x| x)
        }).collect().await;

        Ok(Self {
            founder_crates, member_crates,
            founder_total, member_total,
            aut,
            markup,
            orgs,
            joined,
            keywords,
            collab,
        })
    }

    async fn look_up(kitchen_sink: &KitchenSink, rows: Vec<CrateOwnerRow>) -> Vec<(Arc<RichCrateVersion>, u32, CrateOwnerRow, Vec<OtherOwner>)> {
        futures::stream::iter(rows.into_iter())
            .filter_map(|row| async move {
                let c = kitchen_sink.rich_crate_version_async(&row.origin).await.map_err(|e| eprintln!("{}", e)).ok()?;
                let dl = kitchen_sink.downloads_per_month(&row.origin).await.map_err(|e| eprintln!("{}", e)).ok()?.unwrap_or(0) as u32;
                let owners = kitchen_sink.crate_owners(&row.origin).await.map_err(|e| eprintln!("o: {}", e)).ok()?.into_iter().filter_map(|o| {
                    Some(OtherOwner {
                        invited_at: o.invited_at()?,
                        github_id: o.github_id?,
                        invited_by_github_id: o.invited_by_github_id,
                        login: o.login,
                        kind: o.kind,
                    })
                }).collect();
                Some((c, dl, row, owners))
            })
            .collect().await
    }

    /// `(url, label)`
    pub fn homepage_link(&self) -> Option<(&str, Cow<'_, str>)> {
        if let Some(url) = self.aut.github.blog.as_deref() {
            if url.starts_with("https://") || url.starts_with("http://") {
                let label = url_domain(url)
                    .map(|host| {
                        format!("Home ({})", host).into()
                    })
                    .unwrap_or_else(|| "Homepage".into());
                return Some((url, label));
            }
        }
        None
    }

    pub fn joined_github(&self) -> Option<DateTime<FixedOffset>> {
        if let Some(d) = &self.aut.github.created_at {
            DateTime::parse_from_rfc3339(d).ok()
        } else {
            None
        }
    }

    pub fn org_name(org: &Org) -> &str {
        if let Some(name) = &org.name {
            if name.eq_ignore_ascii_case(&org.login) {
                return &name;
            }
        }
        &org.login
    }

    pub fn format_month(date: &DateTime<Utc>) -> String {
        date.format("%b %Y").to_string()
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
            title: format!("@{}'s Rust crates", self.login()),
            critical_css_data: Some(include_str!("../../style/public/author.css")),
            critical_css_dev_url: Some("/author.css"),
            ..Default::default()
        }
    }

    pub fn render_markdown_str(&self, s: &str) -> templates::Html<String> {
        templates::Html(self.markup.markdown_str(s, true, None))
    }
}
