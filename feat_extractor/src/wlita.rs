use regex::Regex;
use regex::RegexBuilder;

pub struct WLITA<'a> {
    variables: Regex,
    callback: &'a mut dyn for<'r, 's> FnMut(&'r str, &'s str),
}

impl<'a> WLITA<'a> {
    /// Variables will be removed/normalized to the same string
    /// Also ignores all digits
    ///
    /// callback: (normalized, orig) called for every sentence
    pub fn new<I: AsRef<str>>(variables: impl Iterator<Item = I>, callback: &'a mut dyn for<'r, 's> FnMut(&'r str, &'s str)) -> Self {
        let mut pattern = String::with_capacity(256);
        for v in variables.map(|v| regex::escape(v.as_ref())) {
            pattern.push_str("\\b");
            pattern.push_str(v.as_str());
            pattern.push_str("\\b|");
        }
        pattern.push_str("[0-9]+");

        let variables = RegexBuilder::new(&pattern)
            .case_insensitive(true)
            .build().expect("varregex");

        Self {
            variables,
            callback
        }
    }

    /// Any plain text. Calls callback
    pub fn add_text(&mut self, text: &str) {
        let sentence_end = Regex::new(r"[!?.;:] ").unwrap();
        let mut norm_acc = String::with_capacity(256);
        let mut orig_acc = String::with_capacity(256);

        // keep line boundaries to eliminate duplicate/redundant headers
        for line in text.lines().map(str::trim).filter(|s| !s.is_empty()) {
            let mut fragments = sentence_end.split(line);
            while let Some(text) = fragments.next() {
                if !norm_acc.is_empty() {
                    orig_acc.push(' ');
                    norm_acc.push(' ');
                }
                orig_acc.push_str(&text);
                let normalized = self.variables.replace_all(text, "_").to_ascii_lowercase();
                norm_acc.push_str(&normalized);
                if norm_acc.len() > 16 { // can't be too long, or it could easily fall out of sync
                    (self.callback)(&norm_acc, &orig_acc);
                    norm_acc.clear();
                    orig_acc.clear();
                }
            }
            if !norm_acc.is_empty() {
                (self.callback)(&norm_acc, &orig_acc);
                norm_acc.clear();
                orig_acc.clear();
            }
        }
    }
}

#[test]
fn lines_anyway() {
    let mut origs = Vec::new();
    let mut norms = Vec::new();
    let mut cb = |norm: &str, orig: &str| {
        origs.push(orig.to_string());
        norms.push(norm.to_string());
    };
    let mut t = WLITA::new(["foo", "IPSUM", "sit Amet", "bore"].iter(), &mut cb);
    t.add_text("

hi 123 4

        Lorem ipsum dolor sit amet, consectetur adipisicing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.

hello
foo
bar");

    assert_eq!(origs, &["hi 123 4", "Lorem ipsum dolor sit amet, consectetur adipisicing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua", "Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat", "Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur", "Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.", "hello", "foo", "bar"]);
    assert_eq!(norms, &["hi _ _", "lorem _ dolor _, consectetur adipisicing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua", "ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat", "duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur", "excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.", "hello", "_", "bar"]);
}
