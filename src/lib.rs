use std::path::Path;
use std::collections::HashMap;

extern crate tokei;
extern crate ignore;
extern crate serde;
#[macro_use] extern crate serde_derive;

use tokei::LanguageType as LT;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum Language {
    Abap,
    ActionScript,
    Ada,
    Agda,
    Alex,
    Asp,
    AspNet,
    Assembly,
    AutoHotKey,
    Autoconf,
    Bash,
    Batch,
    BrightScript,
    C,
    CHeader,
    CMake,
    CSharp,
    CShell,
    Cabal,
    Cassius,
    Ceylon,
    Clojure,
    ClojureC,
    ClojureScript,
    Cobol,
    CoffeeScript,
    Cogent,
    ColdFusion,
    ColdFusionScript,
    Coq,
    Cpp,
    CppHeader,
    Crystal,
    Css,
    D,
    Dart,
    DeviceTree,
    Dockerfile,
    DreamMaker,
    Edn,
    Elisp,
    Elixir,
    Elm,
    Elvish,
    EmacsDevEnv,
    Erlang,
    FEN,
    FSharp,
    Fish,
    Forth,
    FortranLegacy,
    FortranModern,
    Fstar,
    GdScript,
    Glsl,
    Go,
    Groovy,
    Hamlet,
    Handlebars,
    Happy,
    Haskell,
    Haxe,
    Hcl,
    Hex,
    Html,
    Idris,
    IntelHex,
    Isabelle,
    Jai,
    Java,
    JavaScript,
    Json,
    Jsx,
    Julia,
    Julius,
    KakouneScript,
    Kotlin,
    Lean,
    Less,
    LinkerScript,
    Lisp,
    Lua,
    Lucius,
    Madlang,
    Makefile,
    Markdown,
    Meson,
    Mint,
    ModuleDef,
    MsBuild,
    Mustache,
    Nim,
    Nix,
    OCaml,
    ObjectiveC,
    ObjectiveCpp,
    Org,
    Oz,
    PSL,
    Pascal,
    Perl,
    Php,
    Polly,
    Processing,
    Prolog,
    Protobuf,
    PureScript,
    Python,
    Qcl,
    Qml,
    R,
    Racket,
    Rakefile,
    Razor,
    ReStructuredText,
    Ruby,
    RubyHtml,
    Rust,
    SRecode,
    Sass,
    Scala,
    Scheme,
    Scons,
    Sh,
    Sml,
    SpecmanE,
    Spice,
    Sql,
    Svg,
    Swift,
    SystemVerilog,
    Tcl,
    Tex,
    Text,
    Toml,
    TypeScript,
    UnrealScript,
    UrWeb,
    UrWebProject,
    VB6,
    VBScript,
    Vala,
    Verilog,
    VerilogArgsFile,
    Vhdl,
    VimScript,
    VisualBasic,
    Vue,
    Wolfram,
    XSL,
    Xaml,
    Xml,
    Xtend,
    Yaml,
    Zig,
    Zsh,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Stats {
    pub langs: HashMap<Language, Lines>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Lines {
    pub comments: usize,
    pub code: usize,
}

#[derive(Debug)]
pub struct Collect {
    dummy_dir_entry: ignore::DirEntry,
    stats: Stats,
}

impl Language {
    pub fn name(&self) -> &str {
        self.tokei_lang().name()
    }

    pub fn is_code(&self) -> bool {
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

    pub fn color(&self) -> &'static str {
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

    fn tokei_lang(self) -> LT {
        match self {
            Language::Abap => LT::Abap,
            Language::ActionScript => LT::ActionScript,
            Language::Ada => LT::Ada,
            Language::Agda => LT::Agda,
            Language::Alex => LT::Alex,
            Language::Asp => LT::Asp,
            Language::AspNet => LT::AspNet,
            Language::Assembly => LT::Assembly,
            Language::AutoHotKey => LT::AutoHotKey,
            Language::Autoconf => LT::Autoconf,
            Language::Bash => LT::Bash,
            Language::Batch => LT::Batch,
            Language::BrightScript => LT::BrightScript,
            Language::C => LT::C,
            Language::CHeader => LT::CHeader,
            Language::CMake => LT::CMake,
            Language::CSharp => LT::CSharp,
            Language::CShell => LT::CShell,
            Language::Cabal => LT::Cabal,
            Language::Cassius => LT::Cassius,
            Language::Ceylon => LT::Ceylon,
            Language::Clojure => LT::Clojure,
            Language::ClojureC => LT::ClojureC,
            Language::ClojureScript => LT::ClojureScript,
            Language::Cobol => LT::Cobol,
            Language::CoffeeScript => LT::CoffeeScript,
            Language::Cogent => LT::Cogent,
            Language::ColdFusion => LT::ColdFusion,
            Language::ColdFusionScript => LT::ColdFusionScript,
            Language::Coq => LT::Coq,
            Language::Cpp => LT::Cpp,
            Language::CppHeader => LT::CppHeader,
            Language::Crystal => LT::Crystal,
            Language::Css => LT::Css,
            Language::D => LT::D,
            Language::Dart => LT::Dart,
            Language::DeviceTree => LT::DeviceTree,
            Language::Dockerfile => LT::Dockerfile,
            Language::DreamMaker => LT::DreamMaker,
            Language::Edn => LT::Edn,
            Language::Elisp => LT::Elisp,
            Language::Elixir => LT::Elixir,
            Language::Elm => LT::Elm,
            Language::Elvish => LT::Elvish,
            Language::EmacsDevEnv => LT::EmacsDevEnv,
            Language::Erlang => LT::Erlang,
            Language::FEN => LT::FEN,
            Language::FSharp => LT::FSharp,
            Language::Fish => LT::Fish,
            Language::Forth => LT::Forth,
            Language::FortranLegacy => LT::FortranLegacy,
            Language::FortranModern => LT::FortranModern,
            Language::Fstar => LT::Fstar,
            Language::GdScript => LT::GdScript,
            Language::Glsl => LT::Glsl,
            Language::Go => LT::Go,
            Language::Groovy => LT::Groovy,
            Language::Hamlet => LT::Hamlet,
            Language::Handlebars => LT::Handlebars,
            Language::Happy => LT::Happy,
            Language::Haskell => LT::Haskell,
            Language::Haxe => LT::Haxe,
            Language::Hcl => LT::Hcl,
            Language::Hex => LT::Hex,
            Language::Html => LT::Html,
            Language::Idris => LT::Idris,
            Language::IntelHex => LT::IntelHex,
            Language::Isabelle => LT::Isabelle,
            Language::Jai => LT::Jai,
            Language::Java => LT::Java,
            Language::JavaScript => LT::JavaScript,
            Language::Json => LT::Json,
            Language::Jsx => LT::Jsx,
            Language::Julia => LT::Julia,
            Language::Julius => LT::Julius,
            Language::KakouneScript => LT::KakouneScript,
            Language::Kotlin => LT::Kotlin,
            Language::Lean => LT::Lean,
            Language::Less => LT::Less,
            Language::LinkerScript => LT::LinkerScript,
            Language::Lisp => LT::Lisp,
            Language::Lua => LT::Lua,
            Language::Lucius => LT::Lucius,
            Language::Madlang => LT::Madlang,
            Language::Makefile => LT::Makefile,
            Language::Markdown => LT::Markdown,
            Language::Meson => LT::Meson,
            Language::Mint => LT::Mint,
            Language::ModuleDef => LT::ModuleDef,
            Language::MsBuild => LT::MsBuild,
            Language::Mustache => LT::Mustache,
            Language::Nim => LT::Nim,
            Language::Nix => LT::Nix,
            Language::OCaml => LT::OCaml,
            Language::ObjectiveC => LT::ObjectiveC,
            Language::ObjectiveCpp => LT::ObjectiveCpp,
            Language::Org => LT::Org,
            Language::Oz => LT::Oz,
            Language::PSL => LT::PSL,
            Language::Pascal => LT::Pascal,
            Language::Perl => LT::Perl,
            Language::Php => LT::Php,
            Language::Polly => LT::Polly,
            Language::Processing => LT::Processing,
            Language::Prolog => LT::Prolog,
            Language::Protobuf => LT::Protobuf,
            Language::PureScript => LT::PureScript,
            Language::Python => LT::Python,
            Language::Qcl => LT::Qcl,
            Language::Qml => LT::Qml,
            Language::R => LT::R,
            Language::Racket => LT::Racket,
            Language::Rakefile => LT::Rakefile,
            Language::Razor => LT::Razor,
            Language::ReStructuredText => LT::ReStructuredText,
            Language::Ruby => LT::Ruby,
            Language::RubyHtml => LT::RubyHtml,
            Language::Rust => LT::Rust,
            Language::SRecode => LT::SRecode,
            Language::Sass => LT::Sass,
            Language::Scala => LT::Scala,
            Language::Scheme => LT::Scheme,
            Language::Scons => LT::Scons,
            Language::Sh => LT::Sh,
            Language::Sml => LT::Sml,
            Language::SpecmanE => LT::SpecmanE,
            Language::Spice => LT::Spice,
            Language::Sql => LT::Sql,
            Language::Svg => LT::Svg,
            Language::Swift => LT::Swift,
            Language::SystemVerilog => LT::SystemVerilog,
            Language::Tcl => LT::Tcl,
            Language::Tex => LT::Tex,
            Language::Text => LT::Text,
            Language::Toml => LT::Toml,
            Language::TypeScript => LT::TypeScript,
            Language::UnrealScript => LT::UnrealScript,
            Language::UrWeb => LT::UrWeb,
            Language::UrWebProject => LT::UrWebProject,
            Language::VB6 => LT::VB6,
            Language::VBScript => LT::VBScript,
            Language::Vala => LT::Vala,
            Language::Verilog => LT::Verilog,
            Language::VerilogArgsFile => LT::VerilogArgsFile,
            Language::Vhdl => LT::Vhdl,
            Language::VimScript => LT::VimScript,
            Language::VisualBasic => LT::VisualBasic,
            Language::Vue => LT::Vue,
            Language::Wolfram => LT::Wolfram,
            Language::XSL => LT::XSL,
            Language::Xaml => LT::Xaml,
            Language::Xml => LT::Xml,
            Language::Xtend => LT::Xtend,
            Language::Yaml => LT::Yaml,
            Language::Zig => LT::Zig,
            Language::Zsh => LT::Zsh,
        }
    }

    #[inline]
    pub fn from_path(path: impl AsRef<Path>) -> Option<Self> {
        LT::from_path(path.as_ref())
        .map(|l| match l {
            LT::Abap => Language::Abap,
            LT::ActionScript => Language::ActionScript,
            LT::Ada => Language::Ada,
            LT::Agda => Language::Agda,
            LT::Alex => Language::Alex,
            LT::Asp => Language::Asp,
            LT::AspNet => Language::AspNet,
            LT::Assembly => Language::Assembly,
            LT::AutoHotKey => Language::AutoHotKey,
            LT::Autoconf => Language::Autoconf,
            LT::Bash => Language::Bash,
            LT::Batch => Language::Batch,
            LT::BrightScript => Language::BrightScript,
            LT::C => Language::C,
            LT::CHeader => Language::CHeader,
            LT::CMake => Language::CMake,
            LT::CSharp => Language::CSharp,
            LT::CShell => Language::CShell,
            LT::Cabal => Language::Cabal,
            LT::Cassius => Language::Cassius,
            LT::Ceylon => Language::Ceylon,
            LT::Clojure => Language::Clojure,
            LT::ClojureC => Language::ClojureC,
            LT::ClojureScript => Language::ClojureScript,
            LT::Cobol => Language::Cobol,
            LT::CoffeeScript => Language::CoffeeScript,
            LT::Cogent => Language::Cogent,
            LT::ColdFusion => Language::ColdFusion,
            LT::ColdFusionScript => Language::ColdFusionScript,
            LT::Coq => Language::Coq,
            LT::Cpp => Language::Cpp,
            LT::CppHeader => Language::CppHeader,
            LT::Crystal => Language::Crystal,
            LT::Css => Language::Css,
            LT::D => Language::            D,
            LT::Dart => Language::Dart,
            LT::DeviceTree => Language::DeviceTree,
            LT::Dockerfile => Language::Dockerfile,
            LT::DreamMaker => Language::DreamMaker,
            LT::Edn => Language::Edn,
            LT::Elisp => Language::Elisp,
            LT::Elixir => Language::Elixir,
            LT::Elm => Language::Elm,
            LT::Elvish => Language::Elvish,
            LT::EmacsDevEnv => Language::EmacsDevEnv,
            LT::Erlang => Language::Erlang,
            LT::FEN => Language::FEN,
            LT::FSharp => Language::FSharp,
            LT::Fish => Language::Fish,
            LT::Forth => Language::Forth,
            LT::FortranLegacy => Language::FortranLegacy,
            LT::FortranModern => Language::FortranModern,
            LT::Fstar => Language::Fstar,
            LT::GdScript => Language::GdScript,
            LT::Glsl => Language::Glsl,
            LT::Go => Language::Go,
            LT::Groovy => Language::Groovy,
            LT::Hamlet => Language::Hamlet,
            LT::Handlebars => Language::Handlebars,
            LT::Happy => Language::Happy,
            LT::Haskell => Language::Haskell,
            LT::Haxe => Language::Haxe,
            LT::Hcl => Language::Hcl,
            LT::Hex => Language::Hex,
            LT::Html => Language::Html,
            LT::Idris => Language::Idris,
            LT::IntelHex => Language::IntelHex,
            LT::Isabelle => Language::Isabelle,
            LT::Jai => Language::Jai,
            LT::Java => Language::Java,
            LT::JavaScript => Language::JavaScript,
            LT::Json => Language::Json,
            LT::Jsx => Language::Jsx,
            LT::Julia => Language::Julia,
            LT::Julius => Language::Julius,
            LT::KakouneScript => Language::KakouneScript,
            LT::Kotlin => Language::Kotlin,
            LT::Lean => Language::Lean,
            LT::Less => Language::Less,
            LT::LinkerScript => Language::LinkerScript,
            LT::Lisp => Language::Lisp,
            LT::Lua => Language::Lua,
            LT::Lucius => Language::Lucius,
            LT::Madlang => Language::Madlang,
            LT::Makefile => Language::Makefile,
            LT::Markdown => Language::Markdown,
            LT::Meson => Language::Meson,
            LT::Mint => Language::Mint,
            LT::ModuleDef => Language::ModuleDef,
            LT::MsBuild => Language::MsBuild,
            LT::Mustache => Language::Mustache,
            LT::Nim => Language::Nim,
            LT::Nix => Language::Nix,
            LT::OCaml => Language::OCaml,
            LT::ObjectiveC => Language::ObjectiveC,
            LT::ObjectiveCpp => Language::ObjectiveCpp,
            LT::Org => Language::Org,
            LT::Oz => Language::Oz,
            LT::PSL => Language::PSL,
            LT::Pascal => Language::Pascal,
            LT::Perl => Language::Perl,
            LT::Php => Language::Php,
            LT::Polly => Language::Polly,
            LT::Processing => Language::Processing,
            LT::Prolog => Language::Prolog,
            LT::Protobuf => Language::Protobuf,
            LT::PureScript => Language::PureScript,
            LT::Python => Language::Python,
            LT::Qcl => Language::Qcl,
            LT::Qml => Language::Qml,
            LT::R => Language::            R,
            LT::Racket => Language::Racket,
            LT::Rakefile => Language::Rakefile,
            LT::Razor => Language::Razor,
            LT::ReStructuredText => Language::ReStructuredText,
            LT::Ruby => Language::Ruby,
            LT::RubyHtml => Language::RubyHtml,
            LT::Rust => Language::Rust,
            LT::SRecode => Language::SRecode,
            LT::Sass => Language::Sass,
            LT::Scala => Language::Scala,
            LT::Scheme => Language::Scheme,
            LT::Scons => Language::Scons,
            LT::Sh => Language::Sh,
            LT::Sml => Language::Sml,
            LT::SpecmanE => Language::SpecmanE,
            LT::Spice => Language::Spice,
            LT::Sql => Language::Sql,
            LT::Svg => Language::Svg,
            LT::Swift => Language::Swift,
            LT::SystemVerilog => Language::SystemVerilog,
            LT::Tcl => Language::Tcl,
            LT::Tex => Language::Tex,
            LT::Text => Language::Text,
            LT::Toml => Language::Toml,
            LT::TypeScript => Language::TypeScript,
            LT::UnrealScript => Language::UnrealScript,
            LT::UrWeb => Language::UrWeb,
            LT::UrWebProject => Language::UrWebProject,
            LT::VB6 => Language::VB6,
            LT::VBScript => Language::VBScript,
            LT::Vala => Language::Vala,
            LT::Verilog => Language::Verilog,
            LT::VerilogArgsFile => Language::VerilogArgsFile,
            LT::Vhdl => Language::Vhdl,
            LT::VimScript => Language::VimScript,
            LT::VisualBasic => Language::VisualBasic,
            LT::Vue => Language::Vue,
            LT::Wolfram => Language::Wolfram,
            LT::XSL => Language::XSL,
            LT::Xaml => Language::Xaml,
            LT::Xml => Language::Xml,
            LT::Xtend => Language::Xtend,
            LT::Yaml => Language::Yaml,
            LT::Zig => Language::Zig,
            LT::Zsh => Language::Zsh,
        })
    }
}

impl Collect {
    pub fn new() -> Self {
        // tokei wants DirEntry
        let dummy_dir_entry = ignore::WalkBuilder::new("-")
                            .parents(false).ignore(false)
                            .git_global(false).git_ignore(false).git_exclude(false)
                            .build()
                            .next()
                            .unwrap()
                            .unwrap();
        Self {
            dummy_dir_entry,
            stats: Stats::default(),
        }
    }

    pub fn finish(self) -> Stats {
        self.stats
    }

    pub fn add_to_stats(&mut self, lang: Language, file_content: &str) {
        match lang.tokei_lang().parse_from_str(self.dummy_dir_entry.clone(), file_content) {
            Ok(res) => {
                let stats = self.stats.langs.entry(lang).or_insert(Lines {comments:0, code:0});
                stats.comments += res.comments;
                stats.code += res.code;
            },
            Err(err) => {
                eprintln!("warning: {} ", err);
            },
        }
    }
}
