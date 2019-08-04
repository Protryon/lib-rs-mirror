use rich_crate::RichCrate;
use std::collections::HashMap;
use kitchen_sink::{KitchenSink, CratesIndexCrate, Origin,, stopped};
use fxhash::FxHashMap;
use fxhash::FxHashSet;
use rayon::prelude::*;
use std::sync::Mutex;
use chrono::prelude::*;

#[derive(Default, Copy, Clone, Debug)]
pub struct CrateScore {
    /// Estimate of how much crate's author cares about the crate.
    /// This is just from author's perspective - doesn't need external validation.
    pub authors_perspective: f64,
    /// Crate's importance from users' perspective.
    /// Mainly based on external factors, which ideally
    /// should be somewhat harder to game by the author.
    pub users_perspective: f64,
}


pub fn do_author_pr(crates: &KitchenSink) -> Option<FxHashMap<String, f64>> {
    let crates_io_crates = crates.all_crates_io_crates();

    let (crate_pr, crate_ownership) = rayon::join(
        || get_crate_pr(&crates, crates_io_crates),
        || get_crate_ownership(&crates, crates_io_crates));

    let mut all_authors = FxHashMap::default();
    for (crate_name, crate_importance) in &crate_pr {
        if stopped() {return None;}

        if let Some(authors) = crate_ownership.get(crate_name) {
            for (author_name, &UserContrib {is_owner, contrib}) in authors {
                let author_info = all_authors.entry(author_name).or_insert_with(AuthorInfo::default);
                author_info.crates.insert(crate_name);

                // This is supposed to be author's total external validation
                // (author is important if users think their crates are important).
                // Weighed by ownership, so that minor contributors won't get all the glory for
                // someone else's crate.
                //
                // This can be computed for non-owning contributors too, because owners control who's in
                // (owner -> contributor relationship).
                // Spammy crates will have low scores, so even fake contributors won't add much noise.
                author_info.total_importance += crate_importance.users_perspective * contrib;

                // Passing trust to co-authors.
                // Non-owners can't pass trust to others (contributor -> owner),
                // because they could have been faked (e.g. added to git history against their will)
                if !is_owner {
                    continue;
                }

                // This is how much this author cares about this crate. The more they care, the more
                // trust it requires to collaborate with someone.
                let authors_perspective_relevance = crate_importance.authors_perspective * contrib;

                // Including self in coauthors to see how often author collaborates.
                // If they mostly own code, they don't trust anyone else!
                for (coauthor, UserContrib {contrib, ..}) in authors.iter() {
                    // contrib is already artificially decreased for non-owners.
                    // All values here will be normalized later.
                    let corelevance = authors_perspective_relevance * contrib;
                    *author_info.coauthors.entry(coauthor).or_insert(0.) += corelevance;
                }
            }
        }
    }

    all_authors.par_iter_mut().for_each(|(login, author_info)| {
        if stopped() {return;}

        // this magic number is here only to reduce useless traffic to github
        // (assuming minor authors aren't part of major orgs anyway)
        if author_info.total_importance > 4. {
            // We trust some github orgs, so we trust their members
            if let Ok(Some(orgs)) = crates.user_github_orgs(login) {
                for org in orgs {
                    author_info.total_importance += org_trust(&org.login);
                }
            }
        }

        // normalize weights, since its using authors_perspective scores
        // to divide trust among co-authors
        let coauthors = &mut author_info.coauthors;
        let to_give_sum = coauthors.values().cloned().sum::<f64>().max(0.001); // div/0
        for (_, w) in coauthors {
            *w /= to_give_sum;
        }
    });

    // and now spread users_perspective score based on authors_perspective trust
    let mut final_score: FxHashMap<String, f64> = FxHashMap::with_capacity_and_hasher(all_authors.len(), Default::default());
    for (author_name, author_info) in all_authors {
        *final_score.entry(author_name.to_string()).or_insert(0.) += author_info.total_importance;
        for (coauthor, trust_weight) in author_info.coauthors {
            *final_score.entry(coauthor.to_string()).or_insert(0.) += trust_weight * author_info.total_importance;
        }
    }
    Some(final_score)
}

struct UserContrib {
    contrib: f64,
    is_owner: bool,
}

/// For each crate list of all users (by their github login) who contributed to that crate
/// along with percentage of how much each user "owns" the crate, based on crates.io ownership,
/// weighed by approximate code contribution (derived from number of commits)
fn get_crate_ownership<'a>(crates: &KitchenSink, crates_io_crates: &'a FxHashMap<Origin, CratesIndexCrate>) -> FxHashMap<&'a str, FxHashMap<String, UserContrib>> {
    let crate_ownership = Mutex::new(FxHashMap::<&str, FxHashMap<String, UserContrib>>::default());
    crates_io_crates.par_iter().for_each(|(origin, k1)| {
        if stopped() {return;}

        let name = k1.name();
        let k = match crates.rich_crate_version(origin, CrateData::Minimal) {
            Ok(k) => k,
            Err(e) => {
                for c in e.iter_chain() {
                    eprintln!("• error: -- {}", c);
                }
                return;
            },
        };

        if let Ok((authors, owners, ..)) = crates.all_contributors(&k) {
            let mut user_contributions: FxHashMap<_,_> = owners.into_iter().chain(authors)
                .filter_map(|a| {
                    let is_owner = a.owner;
                    let contrib = a.contribution;
                    a.github.map(|gh_user| (gh_user.login.to_ascii_lowercase(), UserContrib {is_owner, contrib}))
                })
                .filter(|(login, _)| {
                    // ignore rust-bus, nursery, and other orgs that are ownership backups, not contributors
                    is_a_real_author(login)
                })
                .collect();
            // contribution value based on commits is rather arbitrary, so normalize its range,
            // so that we can reliably mix it with values based on ownership.
            normalize_contribution(&mut user_contributions);

            for (_, c) in &mut user_contributions {
                c.contrib =
                        // non-owner contributors get very little trust,
                        // since accepting someone's PR is not as serious as giving access
                        c.contrib * if c.is_owner {1.} else {0.05}
                        // even non-contributing owners should get some trust,
                        // since they still have write access
                        + if c.is_owner {0.2} else {0.};
            }
            normalize_contribution(&mut user_contributions);
            crate_ownership.lock().unwrap().insert(name, user_contributions);
        }
    });

    crate_ownership.into_inner().unwrap()
}

fn normalize_contribution(contrib: &mut FxHashMap<String, UserContrib>) {
    let total_contrib = contrib.values().map(|o| o.contrib).sum::<f64>();
    if total_contrib > 0. {
        for c in contrib.values_mut() {
            c.contrib /= total_contrib;
        }
    }
}

#[derive(Default, Debug)]
struct AuthorInfo<'a> {
    total_importance: f64,
    coauthors: HashMap<&'a str, f64>, // github login name => degree of cooperation
    crates: FxHashSet<&'a str>,
}

// there are orgs which are collections of crates, but aren't actual contributors
fn is_a_real_author(login: &str) -> bool {
    match login {
        "rust-bus" | "rust-bus-owner" | "rust-lang-nursery" | "rust-lang-deprecated" => false,
        _ => true,
    }
}

// We trust members in some github orgs (there's some vetting required to get in these orgs)
fn org_trust(login: &str) -> f64 {
    match login {
        "maintainers" => 5.0, // general github
        "rust-lang-deprecated" | "rust-lang-nursery" |
        "rust-community" | "rust-embedded" | "mozilla-standards" => 10.0,
        "google" | "rustwasm" | "integer32llc" | "rustbridge" => 50.0,
        "mozilla" | "servo" => 200.0,
        "rust-lang" => 1000.0,
        _ => 0.,
    }
}

/// Score how many owners is right (on 0..=1 scale)
fn bus_factor_score(k: &RichCrate) -> f64 {
    let num_owners = k.owners().len();
    match num_owners {
        1 => 0.1, // meh
        n @ 2..=5 => n as f64 * 0.2, // good
        n => (1. - (n-5) as f64 * 0.05).max(0.) // suspicious
    }
}

fn time_between_first_and_last_version(k: &RichCrate) -> chrono::Duration {
    let mut max = parse_date("1970-01-01");
    let mut min = parse_date("2222-01-01");
    for v in k.versions() {
        let created = parse_date(&v.created_at);
        if created < min {
            min = created;
        }
        if created > max {
            max = created;
        }
    }
    max - min
}

/// This is very rough
fn initial_crate_score(k: &RichCrate, downloads_or_equivalent: usize) -> CrateScore {
    // Start with rank based on log2 of 3-month downloads
    let dl = downloads_or_equivalent;
    let dl = ((dl*3 + 1) as f64).log2();

    // If there are co-owners, that's a better crate
    // and shows more involvement from the author
    let owners = bus_factor_score(k);

    let age = time_between_first_and_last_version(k).num_days() as f64 / 30.5;
    let num_versions = k.versions().count();

    // this score is a mix of how much users thing the crate is relevant
    // (since we don't want chaff/spam to influence trust scores)
    // and how much author thinks this crate is important to them
    // (since that is more relevant to authors_perspective trust)
    CrateScore {
        // age used as a metric can't be easily faked/spammed
        users_perspective: dl + owners * 3. + age.min(24.) * 0.25,
        // assume more versions = more work put into the crate
        authors_perspective: 5. + dl / 2. + owners * 2. + age.min(5.*24.) * 0.25 + (num_versions as f64).sqrt().min(5.),
    }
}

/// sum of all scores will be 1
fn normalize(scores: &mut FxHashMap<&str, CrateScore>) {
    let mut total_users_perspective = 0.;
    let mut total_authors_perspective = 0.;
    for v in scores.values() {
        total_users_perspective += v.users_perspective;
        total_authors_perspective += v.authors_perspective;
    }
    assert!(total_users_perspective > 0. && total_authors_perspective > 0.);
    for v in scores.values_mut() {
        v.users_perspective /= total_users_perspective;
        v.authors_perspective /= total_authors_perspective;
    }
}

fn get_crate_pr<'a>(crates: &KitchenSink, crates_io_crates: &'a FxHashMap<Origin, CratesIndexCrate>) -> FxHashMap<&'a str, CrateScore> {
    let mut initial_scores = FxHashMap::<&str, CrateScore>::with_capacity_and_hasher(crates_io_crates.len(), Default::default());
    initial_scores.extend(crates_io_crates.iter()
        .filter_map(|(o, k1)| {
            let dl = crates.downloads_per_month_or_equivalent(o).ok()?.unwrap_or(0);
            crates.rich_crate(o)
                .map_err(|e| eprintln!("{}: {}", k1.name(), e)).ok()
                .map(|k| (k1.name(), k, dl))
        })
        .map(|(name, k, dl)| {
            (name, initial_crate_score(&k, dl))
        }));
    normalize(&mut initial_scores);

    let mut prev_pass = initial_scores.clone();

    // And then on each run pass some rank to deps
    let damping_factor = 0.85;
    for _pass in 0..10 {
        let mut pool = CrateScore::default();
        let mut next_pass = FxHashMap::with_capacity_and_hasher(prev_pass.len(), Default::default());
        for k in crates_io_crates.values() {
            if let Some(this_crate_pr) = prev_pass.get(k.name()).cloned() {
                let deps = k.latest_version().direct_dependencies();
                if deps.is_empty() {
                    // this is a sink, so pretend it has *all* other crates as deps
                    pool.authors_perspective += this_crate_pr.authors_perspective * damping_factor;
                    pool.users_perspective += this_crate_pr.users_perspective * damping_factor;
                } else {
                    // the score to give away
                    let per_dep = (1. / deps.len() as f64) * damping_factor;
                    for dep in deps {
                        let t = next_pass.entry(dep.name()).or_insert_with(CrateScore::default);
                        t.authors_perspective += this_crate_pr.authors_perspective * per_dep;
                        t.users_perspective += this_crate_pr.users_perspective * per_dep;
                    }
                }
                // copy remainder of own's score to the next stage
                let stays = next_pass.entry(k.name()).or_insert_with(CrateScore::default);
                stays.authors_perspective += this_crate_pr.authors_perspective * (1.-damping_factor);
                stays.users_perspective += this_crate_pr.users_perspective * (1.-damping_factor);
            }
        }
        for (name, score) in &mut next_pass {
            if let Some(init) = initial_scores.get(name) {
                score.users_perspective += pool.users_perspective * init.users_perspective;
                score.authors_perspective += pool.authors_perspective * init.authors_perspective;
            }
        }
        prev_pass = next_pass;
    }
    prev_pass
}


pub(crate) fn parse_date(date: &str) -> Date<Utc> {
    let y = date[0..4].parse().expect("dl date parse");
    let m = date[5..7].parse().expect("dl date parse");
    let d = date[8..10].parse().expect("dl date parse");
    Utc.ymd(y, m, d)
}
