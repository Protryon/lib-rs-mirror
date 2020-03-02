use std::collections::HashSet;
use rich_crate::ManifestExt;
use rich_crate::Manifest;


lazy_static::lazy_static! {
    /// ignore these as keywords
    pub(crate) static ref STOPWORDS: HashSet<&'static str> = [
    "a", "sys", "ffi", "placeholder", "app", "loops", "master", "library", "rs",
    "accidentally", "additional", "adds", "against", "all", "allow", "allows",
    "already", "also", "alternative", "always", "an", "and", "any", "appropriate",
    "arbitrary", "are", "as", "at", "available", "based", "be", "because", "been",
    "both", "but", "by", "can", "certain", "changes", "comes", "contains", "core", "cost",
    "crate", "crates.io", "current", "currently", "custom", "dependencies",
    "dependency", "developers", "do", "don't", "e.g", "easily", "easy", "either",
    "enables", "etc", "even", "every", "example", "examples", "features", "feel",
    "files", "for", "from", "fully", "function", "get", "given", "had", "has",
    "have", "here", "if", "implementing", "implements", "in", "includes",
    "including", "incurring", "installation", "interested", "into", "is", "it",
    "it's", "its", "itself", "just", "known", "large", "later", "library",
    "license", "lightweight", "like", "made", "main", "make", "makes", "many",
    "may", "me", "means", "method", "minimal", "mit", "more", "mostly", "much",
    "need", "needed", "never", "new", "no", "noop", "not", "of", "on", "one",
    "only", "or", "other", "over", "plausible", "please", "possible", "program",
    "provides", "put", "readme", "release", "runs", "rust", "rust's", "same",
    "see", "selected", "should", "similar", "simple", "simply", "since", "small", "so",
    "some", "specific", "still", "stuff", "such", "take", "than", "that", "the",
    "their", "them", "then", "there", "therefore", "these", "they", "things",
    "this", "those", "to", "todo", "too", "travis", "two", "under", "us",
    "usable", "use", "used", "useful", "using", "usage", "v1", "v2", "v3", "v4", "various",
    "very", "via", "want", "way", "well", "we'll", "what", "when", "where", "which",
    "while", "will", "wip", "with", "without", "working", "works", "writing",
    "written", "yet", "you", "your", "build status", "meritbadge", "common",
    "file was generated", "easy to use",
    ].iter().copied().collect();
}

    // returns an array of lowercase phrases
    fn extract_text_phrases(manifest: &Manifest, github_description: Option<&str>, readme_text: Option<&str>) -> Vec<(f64, String)> {
        let mut out = Vec::new();
        let mut len = 0;
        if let Some(s) = &manifest.package().description {
            let s = s.to_lowercase();
            len += s.len();
            out.push((1., s));
        }
        if let Some(s) = github_description {
            let s = s.to_lowercase();
            len += s.len();
            out.push((1., s));
        }
        if let Some(sub) = &readme_text {
            // render readme to DOM, extract nodes
            for par in sub.split('\n') {
                if len > 200 {
                    break;
                }
                let par = par.trim_start_matches(|c: char| c.is_whitespace() || c == '#' || c == '=' || c == '*' || c == '-');
                let par = par.replace("http://", " ").replace("https://", " ");
                if !par.is_empty() {
                    let par = par.to_lowercase();
                    len += par.len();
                    out.push((0.4, par));
                }
            }
        }
        out
    }

    pub fn auto_keywords(manifest: &Manifest, github_description: Option<&str>, readme_text: Option<&str>) -> Vec<(f32, String)> {
            let d = extract_text_phrases(manifest, github_description, readme_text);
            let mut sw = rake::StopWords::new();
            sw.reserve(STOPWORDS.len());
            sw.extend(STOPWORDS.iter().map(|s| s.to_string())); // TODO: use real stopwords, THEN filter via STOPWORDS again, because multiple Rust-y words are fine
            // normalize space and _ to -
            let r = rake::Rake::new(sw);
            let rake_keywords = r.run_sentences(d.iter().map(|(_, s)| s.as_str()));
            let rake_keywords = rake_keywords.iter()
                .map(|k| (
                    k.score.min(1.1), //
                    chop3words(k.keyword.as_str()) // rake generates very long setences sometimes
                ));
            // split on / and punctuation too
            let keywords = d.iter().flat_map(|&(w2, ref d)| d.split_whitespace().map(move |s| (w2, s.trim_end_matches("'s"))))
                .filter(|&(_, k)| k.len() >= 2)
                .filter(|&(_, k)| STOPWORDS.get(k).is_none());

            // replace ' ' with '-'
            // keep if 3 words or less
            rake_keywords.chain(keywords).take(25).map(|(w, s)| (w as f32, s.to_owned())).collect()
        }

fn chop3words(s: &str) -> &str {
    let mut words = 0;
    for (pos, ch) in s.char_indices() {
        if ch == ' ' {
            words += 1;
            if words >= 3 {
                return &s[0..pos];
            }
        }
    }
    s
}
