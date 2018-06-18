use std::collections::HashSet;

lazy_static! {
    pub(crate) static ref STOPWORDS: HashSet<String> = [
    "placeholder", "app", "loops", "master", "library",
    "accidentally", "additional", "adds", "against", "all", "allow", "allows",
    "already", "also", "alternative", "always", "an", "and", "any", "appropriate",
    "arbitrary", "are", "as", "at", "available", "based", "be", "because", "been",
    "both", "but", "by", "can", "certain", "changes", "comes", "contains", "cost",
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
    "usable", "use", "used", "useful", "using", "v1", "v2", "v3", "v4", "various",
    "very", "via", "want", "way", "well", "what", "when", "where", "which",
    "while", "will", "wip", "with", "without", "working", "works", "writing",
    "written", "yet", "you", "your",
    ].iter().map(|s|s.to_string()).collect();
}

