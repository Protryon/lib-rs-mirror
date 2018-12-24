use std::{fs, path::Path};
use tantivy::{self, collector::TopCollector, query::QueryParser, schema::*, Index, IndexWriter};

pub struct CrateSearchIndex {
    crate_name_field: Field,
    keywords_field: Field,
    description_field: Field,
    readme_field: Field,
    tantivy_index: Index,
}

#[derive(Debug, Clone)]
pub struct CrateFound {
    pub crate_name: String,
    pub description: String,
    pub score: f32,
}

pub struct Indexer {
    index: CrateSearchIndex,
    writer: IndexWriter,
}

impl CrateSearchIndex {
    pub fn new(index_dir: impl AsRef<Path>) -> tantivy::Result<Self> {
        let index_dir = index_dir.as_ref().join("tantivy");
        if !index_dir.exists() {
            return Self::reset_db_to_empty(&index_dir);
        }
        let tantivy_index = Index::open_in_dir(index_dir)?;
        let schema = tantivy_index.schema();

        Ok(Self {
            tantivy_index,
            crate_name_field: schema.get_field("crate_name").unwrap(),
            keywords_field: schema.get_field("keywords").unwrap(),
            description_field: schema.get_field("description").unwrap(),
            readme_field: schema.get_field("readme").unwrap(),
        })
    }

    fn reset_db_to_empty(index_dir: &Path) -> tantivy::Result<Self> {
        let _ = fs::create_dir_all(index_dir);

        let mut schema_builder = SchemaBuilder::default();

        let crate_name_field = schema_builder.add_text_field("crate_name", STRING | STORED); // STRING means stored literally
        let keywords_field = schema_builder.add_text_field("keywords", TEXT);
        let description_field = schema_builder.add_text_field("description", TEXT | STORED);
        let text_field_indexing = TextFieldIndexing::default().set_tokenizer("en_stem").set_index_option(IndexRecordOption::WithFreqs);
        let text_options = TextOptions::default().set_indexing_options(text_field_indexing);
        let readme_field = schema_builder.add_text_field("readme", text_options);

        let schema = schema_builder.build();
        let tantivy_index = Index::create_in_dir(index_dir, schema)?;
        tantivy_index.load_searchers()?;

        Ok(Self { tantivy_index, crate_name_field, keywords_field, description_field, readme_field })
    }

    pub fn search(&self, query_text: &str, limit: usize) -> tantivy::Result<Vec<CrateFound>> {
        let query_text = query_text.trim();
        let mut query_parser =
            QueryParser::for_index(&self.tantivy_index, vec![self.crate_name_field, self.keywords_field, self.description_field, self.readme_field]);
        query_parser.set_conjunction_by_default();

        let mut top_collector = TopCollector::with_limit(limit);
        let query = query_parser.parse_query(query_text)
            .or_else(|_| {
                let mangled_query: String = query_text.chars().map(|ch| {
                    if ch.is_alphanumeric() {ch} else {' '}
                }).collect();
                query_parser.parse_query(mangled_query.trim())
            })?;

        let searcher = self.tantivy_index.searcher();
        searcher.search(&*query, &mut top_collector)?;

        top_collector.top_docs().into_iter().map(|(score, doc_address)| {
            let retrieved_doc = searcher.doc(doc_address)?;
            let mut doc = self.tantivy_index.schema().to_named_doc(&retrieved_doc).0;
            Ok(CrateFound {
                score,
                crate_name: take_string(doc.remove("crate_name")),
                description: take_string(doc.remove("description")),
            })
        })
        .collect()
    }
}

fn take_string(val: Option<Vec<Value>>) -> String {
    match val {
        Some(mut val) => match val.remove(0) {
            Value::Str(s) => return s,
            _ => panic!("invalid value type"),
        },
        _ => panic!("missing value"),
    }
}

impl Indexer {
    pub fn new(index: CrateSearchIndex) -> tantivy::Result<Self> {
        Ok(Self { writer: index.tantivy_index.writer(250_000_000)?, index })
    }

    pub fn add(&mut self, crate_name: &str, keywords: &str, description: &str, readme: Option<&str>) {
        // delete old doc if any
        let crate_name_term = Term::from_field_text(self.index.crate_name_field, crate_name);
        self.writer.delete_term(crate_name_term);

        // index new one
        let mut doc = Document::default();
        doc.add_text(self.index.crate_name_field, crate_name);
        doc.add_text(self.index.keywords_field, keywords);
        doc.add_text(self.index.description_field, description);
        if let Some(readme) = readme {
            doc.add_text(self.index.readme_field, readme);
        }
        self.writer.add_document(doc);
    }

    pub fn commit(&mut self) -> tantivy::Result<()> {
        self.writer.commit()?;
        self.index.tantivy_index.load_searchers()?;
        Ok(())
    }

    pub fn bye(self) -> tantivy::Result<CrateSearchIndex> {
        self.writer.wait_merging_threads()?;
        Ok(self.index)
    }
}
