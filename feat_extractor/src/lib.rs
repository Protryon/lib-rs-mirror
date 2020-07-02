use rich_crate::RichCrateVersion;
use std::collections::HashSet;
use rich_crate::ManifestExt;
use rich_crate::MaintenanceStatus;
use rich_crate::Manifest;
use semver::VersionReq;

pub mod wlita;

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
            if !par.trim_start().is_empty() {
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
    sw.extend(STOPWORDS.iter().map(|s| (*s).to_string())); // TODO: use real stopwords, THEN filter via STOPWORDS again, because multiple Rust-y words are fine
    // normalize space and _ to -
    let r = rake::Rake::new(sw);
    let rake_keywords = r.run_fragments(d.iter().map(|(_, s)| s.as_str()));
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


pub fn is_deprecated(k: &RichCrateVersion) -> bool {
    if k.version().contains("deprecated") || k.version() == "0.0.0" || k.version() == "0.0.1" {
        return true;
    }
    if k.maintenance() == MaintenanceStatus::Deprecated {
        return true;
    }
    if let Some(orig_desc) = k.description() {
        if orig_desc == "..." { // spam by mahkoh
            return true;
        }
        let orig_desc = orig_desc.trim_matches(|c: char| !c.is_ascii_alphabetic());
        let desc = orig_desc.to_ascii_lowercase();
        return orig_desc.starts_with("WIP") || orig_desc.ends_with("WIP") ||
            desc.starts_with("deprecated") ||
            desc.starts_with("unfinished") ||
            desc.starts_with("an unfinished") ||
            desc.starts_with("unsafe and deprecated") ||
            desc.starts_with("crate is abandoned") ||
            desc.starts_with("abandoned") ||
            desc.contains("this crate is abandoned") ||
            desc.contains("this crate has been abandoned") ||
            desc.contains("do not use") ||
            desc.contains("this crate is a placeholder") ||
            desc.contains("this is a dummy package") ||
            desc.starts_with("an empty crate") ||
            desc.starts_with("discontinued") ||
            desc.starts_with("wip. ") ||
            desc.starts_with("very early wip") ||
            desc.starts_with("renamed to ") ||
            desc.starts_with("crate renamed to ") ||
            desc.starts_with("temporary fork") ||
            desc.contains("no longer maintained") ||
            desc.contains("this tool is abandoned") ||
            desc.ends_with("deprecated") || desc.contains("deprecated in favor") || desc.contains("project is deprecated");
    }
    if let Ok(req) = k.version().parse() {
        if is_deprecated_requirement(k.short_name(), &req) {
            return true;
        }
    }
    false
}

pub fn is_deprecated_requirement(name: &str, requirement: &VersionReq) -> bool {
    let v02 = "0.2.99".parse().unwrap();
    let v01 = "0.1.99".parse().unwrap();
    match name {
        "time" if requirement.matches(&v01) => true,
        "winapi" if requirement.matches(&v01) || requirement.matches(&v02) => true,
        "rustc-serialize" | "gcc" | "rustc-benchmarks" | "rust-crypto" |
        "flate2-crc" | "complex" | "simple_stats" | "concurrent" | "feed" |
        "isatty" | "thread-scoped" | "target_build_utils" | "chan" | "chan-signal" |
        "glsl-to-spirv" => true,
        // futures 0.1
        "futures-preview" | "futures-core-preview" | "tokio-io" | "tokio-timer" | "tokio-codec" => true,
        // fundamentally unsound
        "str-concat" => true,
        // uses old winapi
        "user32-sys" | "shell32-sys" | "advapi32-sys" | "gdi32-sys" | "ole32-sys" | "ws2_32-sys" | "kernel32-sys" | "userenv-sys" => true,
        // renamed
        "hdrsample" => true,
        _ => false,
    }
}

pub fn is_autopublished(k: &RichCrateVersion) -> bool {
    k.description().map_or(false, |d| d.starts_with("Automatically published "))
}

pub fn is_squatspam(k: &RichCrateVersion) -> bool {
    if k.version().contains("reserved") || k.version().contains("placeholder") {
        return true;
    }
    if let Some(desc) = k.description() {
        let desc = desc.trim_matches(|c: char| !c.is_ascii_alphabetic()).to_ascii_lowercase();
        return desc.contains("this crate is a placeholder") ||
            desc.contains("reserving this crate") ||
            desc.contains("this crate has been retired") ||
            desc.contains("want to use this name") ||
            desc.contains("this is a dummy package") ||
            desc == "reserved" ||
            desc.starts_with("placeholder") ||
            desc.ends_with(" placeholder") ||
            desc.starts_with("a placeholder") ||
            desc.starts_with("empty crate") ||
            desc.starts_with("an empty crate") ||
            desc.starts_with("reserved for ") ||
            desc.starts_with("stub to squat") ||
            desc.starts_with("claiming it before someone") ||
            desc.starts_with("reserved name") ||
            desc.starts_with("reserved package") ||
            desc.starts_with("reserve the name");
    }
    false
}
