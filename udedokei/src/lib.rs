use serde_derive::*;
use ahash::HashMap;
use std::path::Path;

pub use tokei::LanguageType as Language;

#[derive(Debug, PartialEq, Clone, Default, Serialize, Deserialize)]
pub struct Stats {
    pub langs: HashMap<Language, Lines>,
    pub has_old_try: bool,
}

#[derive(Debug, PartialEq, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Lines {
    pub comments: u32,
    pub code: u32,
}

#[derive(Debug)]
pub struct Collect {
    stats: Stats,
}

pub trait LanguageExt {
    fn is_code(&self) -> bool;
    fn color(&self) -> &'static str;
}

pub fn from_path(path: impl AsRef<Path>) -> Option<Language> {
    Language::from_path(path.as_ref(), &tokei::Config {
        no_ignore: Some(true),
        no_ignore_parent: Some(true),
        treat_doc_strings_as_comments: Some(true),
        ..Default::default()
    })
}

impl LanguageExt for Language {
    fn is_code(&self) -> bool {
        match *self {
            Language::AutoHotKey |
            Language::Autoconf |
            Language::CHeader |
            Language::CMake |
            Language::CShell |
            Language::CppHeader |
            Language::Css |
            Language::DeviceTree |
            Language::Dockerfile |
            Language::DreamMaker |
            Language::EmacsDevEnv |
            Language::Fish |
            Language::Html |
            Language::Hex |
            Language::Org |
            Language::Json |
            Language::Less |
            Language::LinkerScript |
            Language::Makefile |
            Language::Markdown |
            Language::Meson |
            Language::ModuleDef |
            Language::MsBuild |
            Language::Nix |
            Language::Protobuf |
            Language::Rakefile |
            Language::ReStructuredText |
            Language::Sass |
            Language::Svg |
            Language::Tex |
            Language::Text |
            Language::Toml |
            Language::UrWebProject |
            Language::VerilogArgsFile |
            Language::Xaml |
            Language::Xml |
            Language::Yaml => false,
            _ => true,
        }
    }

    fn color(&self) -> &'static str {
        match *self {
            Language::Abap => "#E8274B",
            Language::ActionScript => "#882B0F",
            Language::Ada => "#02f88c",
            Language::Agda => "#315665",
            Language::Asp => "#6a40fd",
            Language::AspNet => "#6a40fd",
            Language::Assembly => "#6E4C13",
            Language::AutoHotKey => "#6594b9",
            Language::Batch => "#C1F12E",
            Language::CShell => "#C1F12E",
            Language::C => "#555555",
            Language::CHeader => "#555555",
            Language::Cassius => "#ccccff",
            Language::Ceylon => "#dfa535",
            Language::Clojure => "#db5855",
            Language::ClojureC => "#db5855",
            Language::ClojureScript => "#db5855",
            Language::CoffeeScript => "#244776",
            Language::ColdFusion => "#ed2cd6",
            Language::Cpp => "#f34b7d",
            Language::Crystal => "#776791",
            Language::CSharp => "#178600",
            Language::Css => "#563d7c",
            Language::D => "#ba595e",
            Language::Dart => "#00B4AB",
            Language::Dockerfile => "#0db7ed",
            Language::Edn => "#ccce35",
            Language::Elisp => "#c065db",
            Language::Elixir => "#6e4a7e",
            Language::Elm => "#60B5CC",
            Language::Elvish => "#913960",
            Language::Erlang => "#B83998",
            Language::FEN => "#FFF4F3",
            Language::Fish => "#88ccff",
            Language::Forth => "#341708",
            Language::FortranLegacy => "#4d41b1",
            Language::FortranModern => "#4d41b1",
            Language::FSharp => "#b845fc",
            Language::Fstar => "#14253c",
            Language::GdScript => "#8fb200",
            Language::Glsl => "#a78649",
            Language::Go => "#375eab",
            Language::Groovy => "#e69f56",
            Language::Handlebars => "#f0a9f0",
            Language::Haskell => "#5e5086",
            Language::Haxe => "#df7900",
            Language::Hex => "#0e60e3",
            Language::Html => "#e34c26",
            Language::Idris => "#b30000",
            Language::IntelHex => "#a9188d",
            Language::Isabelle => "#FEFE00",
            Language::Jai => "#9EEDFF",
            Language::Java => "#b07219",
            Language::JavaScript => "#f1e05a",
            Language::Jsx => "#40d47e",
            Language::Julia => "#a270ba",
            Language::KakouneScript => "#28431f",
            Language::Kotlin => "#F18E33",
            Language::Lean => "#4C3023",
            Language::Less => "#499886",
            Language::LinkerScript => "#185619",
            Language::Lua => "#000080",
            Language::Lucius => "#cc9900",
            Language::CMake => "#427819",
            Language::Makefile => "#427819",
            Language::Autoconf => "#427819",
            Language::Markdown => "#4A76B8",
            Language::Meson => "#007800",
            Language::Mint => "#62A8D6",
            Language::ModuleDef => "#b7e1f4",
            Language::MsBuild => "#28431f",
            Language::Mustache => "#ff2b2b",
            Language::Nim => "#37775b",
            Language::Nix => "#7e7eff",
            Language::ObjectiveC => "#438eff",
            Language::ObjectiveCpp => "#6866fb",
            Language::OCaml => "#3be133",
            Language::Org => "#b0b77e",
            Language::Oz => "#fab738",
            Language::Pascal => "#E3F171",
            Language::Perl => "#0298c3",
            Language::Php => "#4F5D95",
            Language::Polly => "#dad8d8",
            Language::Processing => "#0096D8",
            Language::Prolog => "#74283c",
            Language::Protobuf => "#7fa2a7",
            Language::PSL => "#7055b5",
            Language::PureScript => "#1D222D",
            Language::Python => "#3572A5",
            Language::Qcl => "#0040cd",
            Language::R => "#198CE7",
            Language::Racket => "#22228f",
            Language::Razor => "#9d5200",
            Language::ReStructuredText => "#358a5b",
            Language::Ruby => "#701516",
            Language::Rust => "#dea584",
            Language::Sass => "#64b970",
            Language::Scala => "#c22d40",
            Language::Scheme => "#1e4aec",
            Language::Scons => "#0579aa",
            Language::Sh => "#89e051",
            Language::Bash => "#89e051",
            Language::Sml => "#596706",
            Language::SpecmanE => "#AA6746",
            Language::Spice => "#3F3F3F",
            Language::Sql => "#646464",
            Language::SRecode => "#B34936",
            Language::Svg => "#b2011d",
            Language::Swift => "#ffac45",
            Language::SystemVerilog => "#DAE1C2",
            Language::Tcl => "#e4cc98",
            Language::Tex => "#3D6117",
            Language::Text => "#00004c",
            Language::Toml => "#A0AA87",
            Language::TypeScript => "#2b7489",
            Language::UnrealScript => "#a54c4d",
            Language::UrWeb => "#cf142b",
            Language::UrWebProject => "#cf142b",
            Language::Vala => "#fbe5cd",
            Language::Verilog => "#b2b7f8",
            Language::VerilogArgsFile => "#b2b7f8",
            Language::Vhdl => "#adb2cb",
            Language::VimScript => "#199f4b",
            Language::VisualBasic => "#945db7",
            Language::Vue => "#2c3e50",
            Language::Wolfram => "#42f1f4",
            Language::Xaml => "#7582D1",
            Language::Xml => "#EB8CEB",
            Language::Yaml => "#4B6BEF",
            Language::Zig => "#99DA07",
            Language::Zsh => "#5232e7",
            _ => "#a5a3a0",
        }
    }
}

impl Collect {
    pub fn new() -> Self {
        Self { stats: Stats::default() }
    }

    pub fn finish(self) -> Stats {
        self.stats
    }

    pub fn add_to_stats(&mut self, lang: Language, file_content: &str) {
        if lang == Language::Rust {
            self.rust_code_stats(file_content);
        }
        let res = lang.parse_from_str(file_content, &tokei::Config {
            no_ignore: Some(true),
            no_ignore_parent: Some(true),
            treat_doc_strings_as_comments: Some(true),
            ..Default::default()
        });
        let stats = self.stats.langs.entry(lang).or_insert(Lines { comments: 0, code: 0 });
        stats.comments += res.comments as u32;
        stats.code += res.code as u32;
    }

    fn rust_code_stats(&mut self, file_content: &str) {
        for line in file_content.lines().take(10000) {
            // half-assed effort to remove comments
            let line = line.find("//").map(|pos| &line[0..pos]).unwrap_or(line);

            if !self.stats.has_old_try {
                self.stats.has_old_try = line.contains("try!(");
            }
        }
    }
}
