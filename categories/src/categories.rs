use toml;

#[macro_use] extern crate serde_derive;
#[macro_use] extern crate lazy_static;
#[macro_use] extern crate quick_error;

use std::borrow::Cow;
use std::collections::BTreeMap;
use toml::value::{Table, Value};

mod tuning;
pub use crate::tuning::*;

const CATEGORIES_TOML: &[u8] = include_bytes!("categories.toml");

#[derive(Debug, Clone)]
pub struct Categories {
    pub root: CategoryMap,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Category {
    pub name: String,
    pub description: String,
    pub short_description: String,
    pub standalone_name: Option<String>,
    pub title: String,
    pub slug: String,
    // fudge factor for selecting primary category (how useful and specific it is)
    pub preference: f32,
    pub sub: CategoryMap,
    pub siblings: Vec<String>,
    pub obvious_keywords: Vec<String>,
}

pub type CategoryMap = BTreeMap<String, Category>;
pub type CResult<T> = Result<T, CatError>;

quick_error! {
    #[derive(Debug, Clone)]
    pub enum CatError {
        MissingField {}
        Parse(err: toml::de::Error) {
            display("Categories parse error: {}", err)
            from()
            cause(err)
        }
    }
}

lazy_static! {
    pub static ref CATEGORIES: Categories = { Categories::new().expect("built-in categories") };
}

impl Categories {
    fn new() -> CResult<Self> {
        Ok(Self {
            root: Self::categories_from_table("", toml::from_slice(CATEGORIES_TOML)?)?
        })
    }

    pub fn from_slug<S: AsRef<str>>(&self, slug: S) -> impl Iterator<Item = &Category> {
        let mut out = Vec::new();
        let mut cats = &self.root;
        for name in slug.as_ref().split("::") {
            match cats.get(name) {
                Some(cat) => {
                    cats = &cat.sub;
                    out.push(cat);
                },
                None => break,
            }
        }
        out.into_iter()
    }

    fn categories_from_table(full_slug_start: &str, toml: Table) -> CResult<CategoryMap> {
        toml.into_iter().map(|(slug, details)| {
            let mut details: Table = details.try_into()?;
            let name = details.remove("name").ok_or(CatError::MissingField)?.try_into()?;
            let description = details.remove("description").ok_or(CatError::MissingField)?.try_into()?;
            let short_description = details.remove("short-description").ok_or(CatError::MissingField)?.try_into()?;
            let title = details.remove("title").ok_or(CatError::MissingField)?.try_into()?;
            let standalone_name = details.remove("standalone-name").and_then(|v| v.try_into().ok());
            let obvious_keywords = details.remove("obvious-keywords").and_then(|v| v.try_into().ok()).unwrap_or_default();
            let preference = details.remove("preference").ok_or(CatError::MissingField)?.try_into()?;
            let siblings = details.remove("siblings").and_then(|v| v.try_into().ok()).unwrap_or_default();

            let mut full_slug = String::with_capacity(full_slug_start.len() + 2 + slug.len());
            if full_slug_start != "" {
                full_slug.push_str(full_slug_start);
                full_slug.push_str("::");
            }
            full_slug.push_str(&slug);

            let sub = if let Some(Value::Table(table)) = details.remove("categories") {
                Self::categories_from_table(&full_slug, table)?
            } else {
                CategoryMap::new()
            };
            Ok((slug, Category {
                name,
                title,
                short_description,
                standalone_name,
                description,
                preference,
                slug: full_slug,
                sub,
                siblings,
                obvious_keywords,
            }))
        }).collect()
    }

    pub fn filtered_category_slugs(cats: &[String]) -> impl Iterator<Item = &str> {
        cats.iter().filter_map(|s| {
            if s.len() < 2 {
                return None;
            }
            if s == "external-ffi-bindings" { // We pretend it doesn't exist
                return None;
            }
            if s == "api-bindings" { // We pretend it doesn't exist
                return None;
            }
            Some(s.as_str())
        })
    }

    pub fn fixed_category_slugs(cats: &[String]) -> Vec<Cow<'_, str>> {
        let mut cats = Self::filtered_category_slugs(cats).enumerate()
        .filter_map(|(idx, s)| {
            let s = s.trim_matches(':');
            let mut chars = s.chars().peekable();
            while let Some(cur) = chars.next() {
                // look for a:b instead of a::b
                if cur == ':' && chars.peek().map_or(false, |&c| c == ':') {
                    chars.next(); // OK, skip second ':'
                    continue;
                }
                if cur == '-' || cur.is_ascii_lowercase() || cur.is_ascii_digit() {
                    continue;
                }

                // bad syntax! Fix!
                let slug = s.to_ascii_lowercase().split(':').filter(|s| !s.is_empty()).collect::<Vec<_>>().join("::");
                if s.is_empty() {
                    return None;
                }
                let depth = slug.split("::").count();
                return Some((depth, idx, slug.into()));
            }
            let depth = s.split("::").count();
            Some((depth, idx, Cow::Borrowed(s)))
        })
        .filter(|(_, _, s)| {
            if CATEGORIES.from_slug(s).next().is_none() {
                println!("invalid cat name {}", s);
                return false;
            }
            true
        })
        .collect::<Vec<_>>();

        // depth, then original order
        cats.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));

        cats.into_iter().map(|(_, _, c)| c).collect()
    }
}

impl Category {
    pub fn standalone_name(&self) -> &str {
        self.standalone_name.as_ref().unwrap_or(&self.name).as_str()
    }
}

#[test]
fn cat() {
    Categories::new().expect("categories").root.get("parsing").expect("parsing");

    CATEGORIES.root.get("development-tools").expect("development-tools").sub.get("build-utils").expect("build-utils");
}
