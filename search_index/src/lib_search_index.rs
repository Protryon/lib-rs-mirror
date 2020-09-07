use rich_crate::Origin;
use std::cmp::Ordering;
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
    pub version: String,
    pub monthly_downloads: u64,
}

pub struct Indexer {
    index: CrateSearchIndex,
    writer: IndexWriter,
}

impl CrateSearchIndex {
    pub fn new(index_dir: impl AsRef<Path>) -> tantivy::Result<Self> {
        let index_dir = index_dir.as_ref().join("tantivy13");
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
    pub fn search(&self, query_text: &str, limit: usize, sort_by_query_relevance: bool) -> tantivy::Result<Vec<CrateFound>> {
        let query_text = query_text.trim();
        let mut query_parser = QueryParser::for_index(&self.tantivy_index, vec![
            self.crate_name_field, self.keywords_field, self.description_field, self.readme_field,
        ]);
        query_parser.set_conjunction_by_default();

        let query = query_parser.parse_query(query_text)
            .or_else(|_| {
                let mangled_query: String = query_text.chars().map(|ch| {
                    if ch.is_alphanumeric() {ch.to_ascii_lowercase()} else {' '}
                }).collect();
                query_parser.parse_query(mangled_query.trim())
            })?;

        let reader = self.tantivy_index.reader()?;
        let searcher = reader.searcher();
        let top_docs = searcher.search(&*query, &TopDocs::with_limit((limit+50+limit/2).max(250)))?; // 250 is a hack for https://github.com/tantivy-search/tantivy/issues/700

        let mut docs = top_docs.into_iter().map(|(relevance_score, doc_address)| {
            let retrieved_doc = searcher.doc(doc_address)?;
            let mut doc = self.tantivy_index.schema().to_named_doc(&retrieved_doc).0;
            let mut crate_base_score = take_int(doc.get("crate_score")) as f64;
            let crate_name = take_string(doc.remove("crate_name"));
            let origin = Origin::from_str(take_string(doc.remove("origin")));
            Ok(CrateFound {
                score: if sort_by_query_relevance {
                    // bonus for exact match
                    if crate_name == query_text {
                        crate_base_score += CRATE_SCORE_MAX / 8.;
                    }
                    (relevance_score as f64 * crate_base_score) as f32
                } else {
                    crate_base_score as f32
                },
                crate_name,
                description: take_string(doc.remove("description")),
                keywords: take_string(doc.remove("keywords")),
                version: take_string(doc.remove("crate_version")),
                monthly_downloads: if let Origin::CratesIo(_) = &origin {take_int(doc.get("monthly_downloads"))} else {0},
                origin,
            })
        })
        .collect::<tantivy::Result<Vec<_>>>()?;

        // workaround for bug or corrupted index that caused dupes
        docs.dedup_by(|a, b| a.crate_name == b.crate_name);

        // re-sort using our base score
        docs.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));
        docs.truncate(limit); // search picked a few more results to cut out chaff using crate_score
        Ok(docs)
    }
}

fn take_string(val: Option<Vec<Value>>) -> String {
    match val {
        Some(mut val) => match val.remove(0) {
            Value::Str(s) => s,
            _ => panic!("invalid value type"),
        },
        _ => String::new(),
    }
}

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
    pub fn add(&mut self, origin: &Origin, crate_name: &str, version: &str, description: &str, keywords: &[&str], readme: Option<&str>, monthly_downloads: u64, score: f64) {
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
        self.writer.add_document(doc);
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
