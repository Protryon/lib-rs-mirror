use itertools::Itertools;
use std::collections::HashSet;
use std::collections::HashMap;
use rich_crate::Origin;
use tantivy::TantivyError;
use std::{fs, path::Path};
use tantivy::{self, collector::TopDocs, query::QueryParser, schema::*, Index, IndexWriter};

const CRATE_SCORE_MAX: f64 = 1_000_000.;

pub struct CrateSearchIndex {
    /// Origin.to_str
    origin_pkey: Field,
    /// as-is
    crate_name_field: Field,
    /// ", "-separated
    keywords_field: Field,
    description_field: Field,
    readme_field: Field,
    /// raw number
    monthly_downloads: Field,
    /// semver string
    crate_version: Field,
    /// number in range 0..=SCORE_MAX denoting desirability of the crate
    crate_score: Field,

    tantivy_index: Index,
}

#[derive(Debug, Clone)]
pub struct CrateFound {
    pub origin: Origin,
    pub crate_name: String,
    pub description: String,
    pub keywords: String,
    pub score: f32,
    pub relevance_score: f32,
    pub crate_base_score: f32,
    pub version: String,
    pub monthly_downloads: u64,
}

pub struct Indexer {
    index: CrateSearchIndex,
    writer: IndexWriter,
}

impl CrateSearchIndex {
    pub fn new(index_dir: impl AsRef<Path>) -> tantivy::Result<Self> {
        let index_dir = index_dir.as_ref().join("tantivy18");
        if !index_dir.exists() {
            return Self::reset_db_to_empty(&index_dir);
        }
        let tantivy_index = Index::open_in_dir(index_dir)?;
        let schema = tantivy_index.schema();

        Ok(Self {
            tantivy_index,
            origin_pkey: schema.get_field("origin").expect("schema"),
            crate_name_field: schema.get_field("crate_name").expect("schema"),
            keywords_field: schema.get_field("keywords").expect("schema"),
            description_field: schema.get_field("description").expect("schema"),
            readme_field: schema.get_field("readme").expect("schema"),
            monthly_downloads: schema.get_field("monthly_downloads").expect("schema"),
            crate_version: schema.get_field("crate_version").expect("schema"),
            crate_score: schema.get_field("crate_score").expect("schema"),
        })
    }

    fn reset_db_to_empty(index_dir: &Path) -> tantivy::Result<Self> {
        let _ = fs::create_dir_all(index_dir);

        let mut schema_builder = SchemaBuilder::default();

        let origin_pkey = schema_builder.add_text_field("origin", STRING | STORED);
        let crate_name_field = schema_builder.add_text_field("crate_name", TEXT | STORED);
        let keywords_field = schema_builder.add_text_field("keywords", TEXT | STORED);
        let description_field = schema_builder.add_text_field("description", TEXT | STORED);
        let text_field_indexing = TextFieldIndexing::default().set_tokenizer("en_stem").set_index_option(IndexRecordOption::WithFreqs);
        let text_options = TextOptions::default().set_indexing_options(text_field_indexing);
        let readme_field = schema_builder.add_text_field("readme", text_options);
        let crate_version = schema_builder.add_text_field("crate_version", STRING | STORED);
        let monthly_downloads = schema_builder.add_u64_field("monthly_downloads", STORED);
        let crate_score = schema_builder.add_u64_field("crate_score", STORED);

        let schema = schema_builder.build();
        let tantivy_index = Index::create_in_dir(index_dir, schema)?;

        Ok(Self { tantivy_index, origin_pkey, crate_name_field, keywords_field, description_field, readme_field, monthly_downloads, crate_version, crate_score })
    }

    /// if sort_by_query_relevance is false, sorts by internal crate score (relevance is still used to select the crates)
    /// Second arg is distinctive keywords
    pub fn search(&self, query_text: &str, limit: usize, sort_by_query_relevance: bool) -> tantivy::Result<(Vec<CrateFound>, Vec<String>)> {
        let query_text = query_text.trim();
        let mut query_parser = QueryParser::for_index(&self.tantivy_index, vec![
            self.crate_name_field, self.keywords_field, self.description_field, self.readme_field,
        ]);
        query_parser.set_conjunction_by_default();
        query_parser.set_field_boost(self.crate_name_field, 2.0);
        query_parser.set_field_boost(self.keywords_field, 1.5);
        query_parser.set_field_boost(self.readme_field, 0.5);

        let query = query_parser.parse_query(query_text)
            .or_else(|_| {
                let mangled_query: String = query_text.chars().map(|ch| {
                    if ch.is_alphanumeric() {ch.to_ascii_lowercase()} else {' '}
                }).collect();
                query_parser.parse_query(mangled_query.trim())
            })?;

        let reader = self.tantivy_index.reader()?;
        let searcher = reader.searcher();
        let top_docs = searcher.search(&*query, &TopDocs::with_limit((limit + 50 + limit / 2).max(250)))?; // 250 is a hack for https://github.com/tantivy-search/tantivy/issues/700

        let mut docs = top_docs.into_iter().map(|(relevance_score, doc_address)| {
            let retrieved_doc = searcher.doc(doc_address)?;
            let mut doc = self.tantivy_index.schema().to_named_doc(&retrieved_doc).0;
            let crate_base_score = take_int(doc.get("crate_score")) as f64 / CRATE_SCORE_MAX;
            let crate_name = take_string(doc.remove("crate_name"));
            let origin = Origin::from_str(take_string(doc.remove("origin")));
            Ok(CrateFound {
                crate_base_score: crate_base_score as f32,
                relevance_score: relevance_score as f32,
                score: 0.,
                crate_name,
                description: take_string(doc.remove("description")),
                keywords: take_string(doc.remove("keywords")),
                version: take_string(doc.remove("crate_version")),
                monthly_downloads: if origin.is_crates_io() { take_int(doc.get("monthly_downloads")) } else { 0 },
                origin,
            })
        })
        .collect::<tantivy::Result<Vec<_>>>()?;

        let max_relevance = docs.iter().map(|v| v.relevance_score).max_by(|a,b| a.partial_cmp(b).unwrap()).unwrap_or(0.);
        for doc in &mut docs {
            doc.score = if sort_by_query_relevance {
                // bonus for exact match
                doc.crate_base_score * if doc.crate_name.eq_ignore_ascii_case(query_text) {
                    max_relevance * 1.10
                } else {
                    doc.relevance_score
                }
            } else {
                doc.crate_base_score
            };
        }

        // workaround for bug or corrupted index that caused dupes
        docs.dedup_by(|a, b| a.crate_name == b.crate_name);

        // re-sort using our base score
        docs.sort_unstable_by(|a, b| b.score.total_cmp(&a.score));

        // Pick a few representative keywords that are for different meanings of the query (e.g. oracle db vs padding oracle)
        let query_text = query_text.to_ascii_lowercase();
        let query_as_keyword = query_text.replace(|c:char| { c == '_' || c == ' ' }, "-"); // "foo bar" == "foo-bar"
        let query_as_keyword2 = query_text.split_whitespace().rev().join("-");
        let query_keywords: Vec<_> = query_text.split(|c: char| !c.is_alphanumeric()).filter(|k| !k.is_empty())
            .chain([query_text.as_str(), query_as_keyword.as_str(), query_as_keyword2.as_str()])
            .collect();
        let dividing_keywords = Self::dividing_keywords(&docs, limit/2, &query_keywords,  &["blockchain", "solana", "ethereum", "bitcoin", "cryptocurrency"]).unwrap_or_default();
        docs.truncate(limit); // truncate after getting all interesting keywords

        // Make sure that there's at least one crate ranked high for each of the interesting keywords
        let mut interesting_crate_indices = dividing_keywords.iter().take(6).filter_map(|dk| {
            docs.iter().position(|k| k.keywords.split(", ").any(|k| k == dk))
        }).collect::<Vec<_>>();
        interesting_crate_indices.extend([0,1,2]); // but keep top 3 results as they are
        interesting_crate_indices.sort_unstable();
        interesting_crate_indices.dedup();
        let mut interesting = Vec::with_capacity(interesting_crate_indices.len());
        for idx in interesting_crate_indices.into_iter().rev() {
            let tmp = docs.remove(idx);
            interesting.push(tmp);
        }
        interesting.sort_unstable_by(|a, b| b.score.total_cmp(&a.score));

        docs.truncate(limit.saturating_sub(interesting.len())); // search picked a few more results to cut out chaff using crate_score
        let min_score = docs.first().map(|f| f.score).unwrap_or_default();
        // keep score monotonic
        interesting.iter_mut().for_each(|f| if f.score < min_score { f.score = min_score; });
        interesting.append(&mut docs);
        Ok((interesting, dividing_keywords))
    }



    fn dividing_keywords(results: &[CrateFound], limit: usize, query_keywords: &[&str], skip_entire_results: &[&str]) -> Option<Vec<String>> {
        let mut dupes = HashSet::new();
        let mut keyword_sets = results.iter().enumerate().filter_map(|(i, found)| {
                if !dupes.insert(&found.keywords) { // some crate families have all the same keywords, which is spammy and biases the results
                    return None;
                }
                let k_set: HashSet<_> = found.keywords.split(", ")
                    .filter(|k| !query_keywords.contains(k))
                    .collect();
                if skip_entire_results.iter().any(|&dealbreaker| k_set.contains(dealbreaker)) {
                    return None;
                }
                Some((k_set, if i < limit { 2048 } else { 1024 } + (128. * found.score) as u32)) // integer for ease of sorting, unique for sort stability
            }).collect::<Vec<_>>();
        drop(dupes);


        // api/client/cli are boring and generic
        let most_common = Self::popular_dividing_keyword(&keyword_sets, &["api", "client", "cli"])?;
        // The most common co-occurrence may be a synonym, so skip it for now
        let second_most_common = Self::popular_dividing_keyword(&keyword_sets, &[most_common])?;

        let mut dividing_keywords = Vec::with_capacity(10);
        let mut next_keyword = second_most_common;
        for _ in 0..10 {
            keyword_sets.retain(|(k_set, _)| !k_set.contains(&next_keyword));
            dividing_keywords.push(next_keyword.to_string());
            next_keyword = match Self::popular_dividing_keyword(&keyword_sets, &["reserved"]) {
                None => break,
                Some(another) => another,
            };
        }
        Some(dividing_keywords)
    }

    /// Find a keyword that splits the set into two distinctive groups
    fn popular_dividing_keyword<'a>(keyword_sets: &[(HashSet<&'a str>, u32)], ignore_keywords: &[&str]) -> Option<&'a str> {
        if keyword_sets.len() < 25 {
            return None; // too few results will give odd niche keywords
        }
        let mut counts: HashMap<&str, (u32, u32)> = HashMap::with_capacity(keyword_sets.len());
        for (k_set, w) in keyword_sets {
            for k in k_set {
                let mut n = counts.entry(k).or_default();
                n.0 += 1;
                n.1 += *w;
            }
        }
        let good_pop = (keyword_sets.len() / 3) as u32;
        let too_common = (keyword_sets.len() * 3 / 4) as u32;
        counts.into_iter()
            .filter(|&(_, (pop, _))| pop > 2 && pop <= too_common)
            .filter(|&(k, _)| !ignore_keywords.iter().any(|&ignore| ignore == k))
            .max_by_key(|&(_, (pop, weight))| if pop >= 10 && pop <= good_pop { weight * 2 } else { weight })
            .map(|(k, _)| k)
    }

}

#[track_caller]
fn take_string(val: Option<Vec<Value>>) -> String {
    match val {
        Some(mut val) => match val.remove(0) {
            Value::Str(s) => s,
            _ => panic!("invalid value type"),
        },
        _ => String::new(),
    }
}

#[track_caller]
fn take_int(val: Option<&Vec<Value>>) -> u64 {
    match val {
        Some(val) => match val.get(0) {
            Some(Value::U64(s)) => *s,
            Some(Value::I64(s)) => *s as u64,
            _ => panic!("invalid value type"),
        },
        _ => 0,
    }
}

impl Indexer {
    pub fn new(index: CrateSearchIndex) -> tantivy::Result<Self> {
        Ok(Self { writer: index.tantivy_index.writer(250_000_000)?, index })
    }

    /// score is float 0..=1 range
    pub fn add(&mut self, origin: &Origin, crate_name: &str, version: &str, description: &str, keywords: &[&str], readme: Option<&str>, monthly_downloads: u64, score: f64) -> Result<(), TantivyError> {
        let origin = origin.to_str();
        // delete old doc if any
        let pkey = Term::from_field_text(self.index.origin_pkey, &origin);
        self.writer.delete_term(pkey);

        // index new one
        let mut doc = Document::default();
        doc.add_text(self.index.origin_pkey, &origin);
        doc.add_text(self.index.crate_name_field, crate_name);
        doc.add_text(self.index.keywords_field, &keywords.join(", "));
        doc.add_text(self.index.description_field, description);
        if let Some(readme) = readme {
            doc.add_text(self.index.readme_field, readme);
        }
        doc.add_text(self.index.crate_version, version);
        doc.add_u64(self.index.monthly_downloads, monthly_downloads);
        doc.add_u64(self.index.crate_score, (score * CRATE_SCORE_MAX).ceil() as u64);
        self.writer.add_document(doc)?;
        Ok(())
    }

    pub fn commit(&mut self) -> tantivy::Result<()> {
        self.writer.commit()?;
        Ok(())
    }

    pub fn bye(self) -> tantivy::Result<CrateSearchIndex> {
        self.writer.wait_merging_threads()?;
        Ok(self.index)
    }
}
