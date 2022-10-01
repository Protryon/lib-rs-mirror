use ahash::HashMap;
use std::path::Path;
use std::io;
use std::fs;
use heck::ToKebabCase;
use smartstring::alias::String as SmolStr;

pub struct Synonyms {
    mapping: HashMap<SmolStr, (SmolStr, u8)>,
}

impl Synonyms {
    pub fn new(data_dir: &Path) -> io::Result<Self> {
        let mapping = fs::read_to_string(data_dir.join("tag-synonyms.csv"))?;
        Ok(Self {
            mapping: mapping.lines()
                .filter(|l| !l.starts_with('#'))
                .map(|l| {
                    let mut cols = l.splitn(3, ',');
                    let find = cols.next().expect(l);
                    let replace = cols.next().expect(l);
                    let score: u8 = cols.next().unwrap().parse().expect(l);
                    (SmolStr::from(find), (SmolStr::from(replace), score))
                })
                .collect()
        })
    }

    #[inline]
    pub fn get(&self, keyword: &str) -> Option<(&str, f32)> {
        let (tag, votes) = self.mapping.get(keyword)?;
        let relevance = (*votes as f32 / 5. + 0.1).min(1.);
        Some((tag, relevance))
    }

    fn get_matching(&self, keyword: &str) -> Option<&str> {
        let (tag, votes) = self.mapping.get(keyword)?;
        if *votes > 0 {
            return Some(tag)
        }
        None
    }

    pub fn normalize<'a>(&'a self, keyword: &'a str) -> &'a str {
        if let Some(alt) = self.get_matching(keyword) {
            if let Some(alt2) = self.get_matching(alt) {
                return alt2;
            }
            return alt;
        }
        keyword
    }
}


pub fn normalize_keyword(k: &str) -> String {
    // heck messes up CJK
    if !k.is_ascii() {
        return k.to_lowercase();
    }

    // i-os looks bad
    let mut k = k;
    let tmp;
    if k.starts_with("eBPF") || k.starts_with("iOS") || k.starts_with("iP") || k.starts_with("iM") {
        tmp = k.to_lowercase();
        k = &tmp;
    }
    k.to_kebab_case()
}
