use std::collections::HashMap;
use std::path::Path;
use std::io;
use std::fs;
use heck::ToKebabCase;

pub struct Synonyms {
    mapping: HashMap<Box<str>, (Box<str>, u8)>,
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
                    (find.into(), (replace.into(), score))
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
