use serde_derive::*;

#[derive(Debug, Clone, Deserialize)]
pub struct CrateDepsFile {
    pub dependencies: Vec<CrateDependency>,
}

#[derive(Debug, Copy, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CrateDepKind {
    Normal,
    Dev,
    Build,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CrateDependency {
    pub id: usize,              // 403519,
    pub version_id: usize,      // 90309,
    pub crate_id: String,       // "quick-error",
    pub req: String,            // "^1.2.0",
    pub optional: bool,         // false,
    pub default_features: bool, // true,
    pub features: Vec<String>,  // [],
    pub target: Option<String>, // null,
    pub kind: CrateDepKind,     // "normal",
                                // downloads: usize, // 0; looks broken
}
