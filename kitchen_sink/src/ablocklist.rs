use ahash::HashMap;
use smartstring::alias::String as SmolStr;
use std::path::Path;
use std::io;

pub enum ABlockReason {
    /// Their crates are considered spam, malware, or otherwise junk.
    /// reason
    Banned(Box<str>),
    /// Their crates are OK, but they don't want to be on the site.
    /// reason, url
    Hidden(Box<str>, Option<SmolStr>),
}

pub struct ABlockList {
    by_lc_github_login: HashMap<SmolStr, ABlockReason>,
}

impl ABlockList {
    pub fn new(path: &Path) -> io::Result<Self> {
        let list = std::fs::read_to_string(path)?;

        Ok(Self {
            by_lc_github_login: Self::parse_list(&list)?,
        })
    }

    pub fn get(&self, username: &str) -> Option<&ABlockReason> {
        self.by_lc_github_login.get(username.to_ascii_lowercase().as_str())
    }

    fn parse_list(list: &str) -> io::Result<HashMap<SmolStr, ABlockReason>> {
        let mut out = HashMap::default();
        for (n, l) in list.lines().enumerate() {
            let line = l.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (k, v) = Self::parse_line(line).ok_or_else(|| io::Error::new(io::ErrorKind::Other, format!("ablocklist line {n} is borked: {line}")))?;
            out.insert(k, v);
        }
        Ok(out)
    }

    fn parse_line(line: &str) -> Option<(SmolStr, ABlockReason)> {
        let mut parts = line.splitn(4, ',');
        let username = parts.next()?.trim();
        debug_assert_eq!(username, username.to_ascii_lowercase());
        let kind = parts.next()?.trim();
        let url = parts.next()?.trim();
        let reason = parts.next()?.trim();

        let b = match kind {
            "b" => ABlockReason::Banned(reason.into()),
            "h" => ABlockReason::Hidden(reason.into(), (!url.is_empty()).then(|| url.into())),
            _ => return None,
        };
        Some((username.into(), b))
    }
}
