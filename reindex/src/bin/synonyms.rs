#![allow(dead_code)]

use ahash::HashMapExt;
use ahash::HashSet;
use ahash::HashMap;
use search_index::CrateSearchIndex;
use kitchen_sink::KitchenSink;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let crates = kitchen_sink::KitchenSink::new_default().await?;

    let index = CrateSearchIndex::new(KitchenSink::data_path()?)?;

    let all_kw = crates.crate_db.all_explicit_keywords().await?;

    for k in &all_kw {
        if let Some((most_related, alt1)) = top_other_keyword(&index, &k, &[&k, "rust", "api", "client", "macro", "proc-macro", "no-std", "cargo"], &["blockchain", "ethereum", "bitcoin", "cryptocurrency", "solana"]) {
            if let Some((first_set, alt2)) = top_other_keyword(&index, &format!("{k} -\"{most_related}\""), &[&k, "no-std", "cli", "rust", "client", "wasm"], &[&most_related, "blockchain", "cryptocurrency"]) {
                if let Some((second_set, alt3)) = top_other_keyword(&index, &format!("{k} -\"{first_set}\""), &[&k, &most_related, "api", "cli", "macro", "macros", "rust"], &[&first_set]) {
                    if let Some((third_set, alt4)) = top_other_keyword(&index, &format!("{k} -\"{first_set}\" -\"{second_set}\""), &[&k, &most_related, "client", "api"], &[&first_set, &second_set]) {
                        eprintln!("{k} => {first_set} | {second_set} | {third_set} # {alt1:?} {alt2:?} {alt3:?} {alt4:?}");
                    } else {
                        eprintln!("{k} => {first_set} | {second_set} # {alt1:?} {alt2:?} {alt3:?}");
                    }
                }
            }
        }

    }

    // let mut rev_match: HashMap<String, String> = HashMap::new();
    // for k in &all_kw {
    //     if let Some(other) = top_other_keyword(&index, &k, &[&k]) {
    //         if let Some(reverse) = rev_match.get(&other) {
    //             if reverse == k {
    //                 println!("{other},{k},2");
    //             }
    //         } else {
    //             rev_match.insert(k.clone(), other);
    //         }
    //     }
    // }

    Ok(())
}

fn normalize_dashes(all_kw: &[String]) {
    let mut most_popular: HashMap<String, &str> = HashMap::new();
    for k in all_kw {
        let nodashed = k.replace('-', "");
        most_popular.entry(nodashed)
            .or_insert(k); // relies on sort order to pick most common

    }
    for k in all_kw {
        if let Some(undashed) = most_popular.get(k) {
            if k != undashed {
                println!("{k},{undashed},4");
            }
        }
    }
}

fn normalize_plural(all_kw: &[String]) {
    let mut most_popular: HashMap<&str, &str> = HashMap::new();
    for k in all_kw {
        let singular = k.strip_suffix('s').unwrap_or(k);
        most_popular.entry(singular)
            .and_modify(|prev| { println!("{k},{prev},5"); })
            .or_insert_with(|| k); // relies on sort order to pick most common

        if let Some(singular) = k.strip_suffix("es") {
            most_popular.entry(singular)
                .and_modify(|prev| { println!("{k},{prev},5"); })
                .or_insert_with(|| k);
        }
    }
}

/// Most common + alterantive unrelated to most common
fn top_other_keyword(index: &CrateSearchIndex, query:&str, skip_words: &[&str], skip_results: &[&str]) -> Option<(String, Vec<String>)> {
    let (res, _) = index.search(query, 300, true).ok()?;
    if res.len() < 25 {
        return None; // noisy data?
    }
    let mut keyword_sets = res.iter().filter_map(|k| {
        if k.keywords.iter().any(|k| skip_results.contains(&k.as_str())) {
            return None;
        }
        let mut k: Vec<_> = k.keywords.iter()
            .map(|k| k.as_str())
            .filter(|k| !skip_words.contains(k)).collect();
        k.sort_unstable();
        Some(k)
    }).collect::<HashSet<_>>();

    let most_common = most_common_in_results(&keyword_sets)?;
    let mut alt = Vec::new();
    let mut last = most_common.as_str();
    for _ in 0..5 {
        keyword_sets.retain(|s| !s.contains(&last));
        if keyword_sets.len() < 50 {
            break;
        }
        if let Some(another) = most_common_in_results(&keyword_sets) {
            alt.push(another);
            last = alt.last().unwrap();
        } else {
            break;
        }
    }
    Some((most_common, alt))
}

fn most_common_in_results(keyword_sets: &HashSet<Vec<&str>>) -> Option<String> {
    let mut counts: HashMap<&str, u32> = HashMap::new();
    for k in keyword_sets {
        for k in k {
            *counts.entry(k).or_default() += 1;
        }
    }
    let most_common = counts.into_iter().max_by_key(|&(_, v)| v).map(|(k, _)| k.to_owned())?;
    Some(most_common)
}
