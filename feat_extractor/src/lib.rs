use rich_crate::MaintenanceStatus;
use rich_crate::Manifest;
use rich_crate::ManifestExt;
use rich_crate::Markup;
use rich_crate::RichCrateVersion;
use semver::VersionReq;
use std::collections::HashSet;
use log::debug;

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
            if len > 300 {
                break;
            }
            let par = par.trim_start_matches(|c: char| c.is_whitespace() || c == '#' || c == '=' || c == '*' || c == '-');
            let par = par.replace("http://", " ").replace("https://", " ").replace("the rust programming language", "rust");
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
        let orig_desc = orig_desc.trim_matches(|c: char| !c.is_ascii_alphabetic());
        let desc = orig_desc.to_ascii_lowercase();
        return orig_desc.starts_with("WIP") || orig_desc.ends_with("WIP") ||
            desc.starts_with("unmaintained ") ||
            desc.starts_with("deprecated") ||
            desc.starts_with("this crate was renamed") ||
            desc.starts_with("this crate is deprecated") ||
            desc.starts_with("this package was renamed") ||
            desc.starts_with("obsolete") ||
            desc.starts_with("unfinished") ||
            desc.starts_with("an unfinished") ||
            desc.starts_with("unsafe and deprecated") ||
            desc.starts_with("crate is abandoned") ||
            desc.starts_with("abandoned") ||
            desc.contains("this crate is abandoned") ||
            desc.contains("this crate has been abandoned") ||
            desc.contains("do not use") ||
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
    is_squatspam(k)
}

pub fn is_deprecated_requirement(name: &str, requirement: &VersionReq) -> bool {
    let v02 = "0.2.99".parse().unwrap();
    let v01 = "0.1.99".parse().unwrap();
    match name {
        "time" | "tokio" | "futures" | "opengl32-sys" | "uuid-sys" if requirement.matches(&v01) => true,
        "winapi" | "winmm-sys" if requirement.matches(&v01) || requirement.matches(&v02) => true,
        "tokio" | "secur32-sys" if requirement.matches(&v02) => true,
        "rustc-serialize" | "gcc" | "rustc-benchmarks" | "rust-crypto" |
        "flate2-crc" | "complex" | "simple_stats" | "concurrent" | "feed" |
        "isatty" | "thread-scoped" | "target_build_utils" | "chan" | "chan-signal" |
        "glsl-to-spirv" => true,
        // futures 0.1
        "futures-preview" | "futures-core-preview" | "tokio-io" | "tokio-timer" | "tokio-codec" |
        "tokio-executor" | "tokio-reactor" | "tokio-core" | "futures-cpupool" | "tokio-threadpool" | "tokio-tcp" => true,
        // fundamentally unsound
        "str-concat" => true,
        // uses old winapi
        "user32-sys" | "shell32-sys" | "advapi32-sys" | "gdi32-sys" | "ole32-sys" | "ws2_32-sys" | "kernel32-sys" | "userenv-sys" => true,
        // uses the newest windows api, still deprecated :)
        "winrt" => true,
        // renamed
        "hdrsample" => true,
        // in stdlib
        "insideout" | "file" | "ref_slice" => true,
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
    if k.authors().len() == 1 && k.authors()[0].name.as_deref() == Some("...") {
        return true; // spam by mahkoh
    }
    if let Some(desc) = k.description() {
        if desc == "..." { // spam by mahkoh
            return true;
        }
        if is_reserved_boilerplate_text(desc) {
            return true;
        }
    } else if let Some(readme) = k.readme() {
        match &readme.markup {
            Markup::Html(s) | Markup::Markdown(s) | Markup::Rst(s) => if is_reserved_boilerplate_text(s) {
                return true;
            }
        }
    }

    if let Some(l) = k.lib_file() {
        let hash = blake3::hash(l.as_bytes());
        if [
            [0x07,0xbc,0x44,0x9b,0xb2,0x62,0x3d,0xd1,0x8f,0x80,0xf4,0x1a,0xb0,0xa1,0xf8,0xd5,0x50,0x87,0x4a,0xe5,0xd0,0x2b,0xa2,0x9d,0x69,0xc2,0x40,0x27,0x11,0x3d,0x1e,0x4b],
            [0x25,0x80,0x23,0x73,0x7f,0xc5,0x1a,0xfb,0x32,0xfa,0x96,0xe7,0x53,0xbc,0x5d,0x7d,0xd6,0xcf,0x53,0x58,0x5a,0xec,0x10,0x0b,0x63,0x16,0xad,0x07,0x7e,0x91,0x30,0x9e],
            [0x5b,0x49,0xda,0x51,0x4c,0x3f,0x8f,0x34,0x1a,0x82,0xbd,0x94,0x45,0xd3,0x3d,0x5b,0x23,0x77,0x54,0x9b,0xb3,0x5f,0x63,0x58,0x77,0x6f,0x94,0x5f,0x3a,0xa0,0x7f,0xfc],
            [0x94,0x0a,0x22,0x83,0x8e,0x3b,0x0a,0xb0,0x5a,0xdd,0xf1,0xa6,0x3e,0xc6,0x24,0xfe,0x52,0x5e,0x25,0xf9,0xa7,0x74,0xc0,0x78,0x0e,0xd2,0x57,0x26,0x7c,0x1b,0x94,0x4d],
            [0xad,0x7d,0x56,0x3f,0x13,0x8f,0x96,0x04,0xbe,0xce,0x74,0xd7,0xf3,0x6d,0xc0,0x9a,0x03,0x2d,0xed,0x6d,0x31,0x96,0xbf,0xb1,0xfa,0x3f,0x29,0x37,0x2b,0x0a,0xcf,0x4b],
            [0xb8,0x68,0xa0,0x68,0x43,0xcd,0x43,0x7a,0x58,0xe0,0xf6,0x6f,0x75,0x1b,0xcb,0x3b,0xc9,0x34,0x36,0xc1,0xb4,0xc2,0xe7,0x76,0x2d,0x56,0x26,0xfc,0xe3,0x73,0x42,0x90],
            [0xbe,0xc2,0xe5,0xb4,0x19,0xef,0x46,0x60,0x6c,0xea,0x0a,0x74,0x57,0x7f,0x36,0x0c,0xaa,0xb5,0xb9,0x2d,0x23,0x59,0x71,0x60,0x41,0xe2,0x96,0x85,0xc4,0x92,0x20,0x23],
            [0xd5,0x94,0xd3,0x46,0x5d,0x20,0x3f,0x1d,0xc6,0x96,0x03,0x22,0xc9,0xf2,0xe9,0xa3,0xcc,0xdc,0x91,0xdf,0xc7,0x58,0xb2,0xc7,0x7d,0xa8,0xb7,0xa9,0xd6,0x16,0xb4,0xab],
        ].iter().any(move |h| hash.as_bytes() == h) {
            debug!("lib file");
            return true;
        }
    }

    if let Some(l) = k.bin_file() {
        let hash = blake3::hash(l.as_bytes());
        if hash == [0xfb,0x5c,0xe9,0xdc,0xd7,0x94,0x90,0xba,0x25,0xa7,0x00,0x04,0x07,0x0b,0x11,0xe6,0x2e,0x7f,0xca,0x85,0xc1,0xf6,0x50,0xcb,0x44,0x3e,0x4d,0xd0,0x45,0xbc,0x9f,0xd9] {
            debug!("bin file");
            return true;
        }
    }

    false
}

fn is_reserved_boilerplate_text(desc: &str) -> bool {
    let desc = desc.trim_matches(|c: char| !c.is_ascii_alphabetic()).to_ascii_lowercase();
    let desc2 = desc.trim_start_matches("this crate ")
        .trim_start_matches("is being ")
        .trim_start_matches("is ")
        .trim_start_matches("has been ")
        .trim_start_matches("has ")
        .trim_start_matches("a ").trim_start();
    return desc.contains("this crate is a placeholder") ||
        desc.contains("reserving this crate") ||
        desc.contains("reserving this crate") ||
        desc.contains("only to reserve the name") ||
        desc.contains("this crate has been retired") ||
        desc.contains(" if you want this crate name") ||
        desc.contains("want to use this name") ||
        desc.contains("this is a dummy package") ||
        desc.contains("if you would like to use this crate name, please contact") ||
        desc.starts_with("reserving this crate name for") ||
        desc.starts_with("contact me if you want this name") ||
        desc.starts_with("unused crate name") ||
        desc.starts_with("unused. contact me") ||
        desc.starts_with("crate name not in use") ||
        desc.starts_with("if you want to use this crate name, please contact ") ||
        desc.contains("if you want this name, please contact me") ||
        desc2.starts_with("reserved crate ") ||
        desc.contains("this crate is reserved ") ||
        desc == "reserved" ||
        desc2.starts_with("reserved for future use") ||
        desc2.starts_with("placeholder") ||
        desc.ends_with(" placeholder") ||
        desc.ends_with(" reserved for use") ||
        desc2.starts_with("dummy crate") ||
        desc2.starts_with("available for ownership transfer") ||
        desc2.starts_with("reserved, for") ||
        desc2.starts_with("crate name reserved for") ||
        desc2.starts_with("wip: reserved") ||
        desc2.starts_with("placeholder") ||
        desc2.starts_with("empty crate") ||
        desc2.starts_with("an empty crate") ||
        desc2.starts_with("reserved for ") ||
        desc2.starts_with("stub to squat") ||
        desc2.starts_with("claiming it before someone") ||
        desc2.starts_with("reserved name") ||
        desc2.starts_with("reserved package") ||
        desc2.starts_with("reserve the name");
}
