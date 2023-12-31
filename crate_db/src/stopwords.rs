use lazy_static::lazy_static;
use std::collections::{HashMap, HashSet};

lazy_static! {
    /// ignore these as keywords
    pub(crate) static ref STOPWORDS: HashSet<&'static str> = [
    "a", "sys", "ffi", "placeholder", "app", "loops", "master", "library", "rs",
    "accidentally", "additional", "adds", "against", "all", "allow", "allows",
    "already", "also", "alternative", "always", "an", "and", "any", "appropriate",
    "arbitrary", "are", "as", "at", "available", "based", "be", "because", "been",
    "both", "but", "by", "can", "certain", "changes", "comes", "contains", "code", "core", "cost",
    "crate", "crates.io", "current", "currently", "custom", "dependencies",
    "dependency", "developers", "do", "don't", "e.g", "easily", "easy", "either",
    "enables", "etc", "even", "every", "example", "examples", "features", "feel",
    "files", "fast", "for", "from", "fully", "function", "get", "given", "had", "has",
    "hacktoberfest",
    "have", "here", "if", "implementing", "implements", "implementation", "in", "includes",
    "including", "incurring", "installation", "interested", "into", "is", "it",
    "it's", "its", "itself", "just", "known", "large", "later", "library",
    "license", "lightweight", "like", "made", "main", "make", "makes", "many",
    "may", "me", "means", "method", "minimal", "mit", "more", "mostly", "much",
    "need", "needed", "never", "new", "no", "noop", "not", "of", "on", "one",
    "only", "or", "other", "over", "plausible", "please", "possible", "program", "project",
    "provides", "put", "readme", "release", "runs", "rust", "rust's", "same",
    "see", "selected", "should", "similar", "simple", "simply", "since", "small", "so",
    "some", "specific", "still", "stuff", "such", "take", "than", "that", "the",
    "their", "them", "then", "there", "therefore", "these", "they", "things",
    "this", "those", "to", "todo", "too", "took", "travis", "two", "under", "us",
    "usable", "use", "used", "useful", "using", "usage", "v1", "v2", "v3", "v4", "various",
    "very", "via", "want", "way", "well", "we'll", "what", "when", "where", "which",
    "while", "will", "wip", "with", "without", "working", "works", "writing",
    "written", "yet", "you", "your", "build status", "meritbadge", "common",
    "file was generated", "easy to use", "general-purpose", "fundamental",
    ].iter().copied().collect();

    /// If one is present, ignore the others
    pub(crate) static ref COND_STOPWORDS: HashMap<&'static str, Option<&'static [&'static str]>> = [
        ("game-engine", Some(&["game", "ffi"][..])),
        ("game-engines", Some(&["game", "ffi"])),
        ("game-development", Some(&["game", "ffi"])),
        ("game-dev", Some(&["game", "games"])),
        ("gamedev", Some(&["game", "games"])),
        ("game", Some(&["wasm", "webassembly"])), // wasm games are nice, but should be in games category
        ("contract", Some(&["wasm", "webassembly"])), // that's crypto-babble-nothingburger
        ("opengl", Some(&["terminal", "console"])),
        ("protocol", Some(&["game", "games", "container"])),
        ("framework", Some(&["game", "games"])),
        ("engine", Some(&["ffi"])),
        ("mock", Some(&["macro", "derive", "plugin", "cargo"])),
        ("ffi", Some(&["api-bindings"])),

        ("caching", Some(&["allocator"])),
        ("distributed", Some(&["filesystem", "file"])),
        ("container", Some(&["filesystem", "file"])),
        ("aws", Some(&["ecs"])), // not game engine
        ("raspberry", Some(&["osx", "windows"])),
        ("linux", Some(&["windows", "winsdk", "macos", "mac", "osx"])),
        ("cross-platform", Some(&["windows", "winsdk", "macos", "mac", "osx", "linux", "unix", "gnu"])),
        ("portable", Some(&["windows", "winsdk", "macos", "mac", "osx", "linux", "unix", "gnu"])),
        ("winapi", Some(&["target", "windows", "gnu", "x86", "i686", "64", "pc"])),
        ("windows", Some(&["gnu"])),
        ("compile-time", Some(&["time"])),
        ("constant-time", Some(&["time"])),
        ("real-time", Some(&["time"])),
        ("time-series", Some(&["time"])),
        ("execution", Some(&["time"])),
        ("iterator", Some(&["window", "windows"])),
        ("buffer", Some(&["window", "windows"])),
        ("sliding", Some(&["window", "windows"])),
        ("web", Some(&["windows", "macos", "mac", "osx", "linux"])),
        ("error", Some(&["color"])),
        ("pretty-print", Some(&["color"])),
        ("pretty-printer", Some(&["color"])),
        ("ios", Some(&["core"])),
        ("macos", Some(&["core"])),
        ("osx", Some(&["core"])),
        ("mac", Some(&["core"])),
        ("module", Some(&["core"])),
        ("wasm", Some(&["embedded", "javascript", "no-std", "no_std", "feature:no_std", "deploy"])),
        ("javascript", Some(&["embedded", "no-std", "no_std", "feature:no_std"])),
        ("webassembly", Some(&["embedded", "javascript", "no-std", "no_std", "feature:no_std"])),
        ("deep-learning", Some(&["math", "statistics"])),
        ("machine-learning", Some(&["math", "statistics"])),
        ("neural-networks", Some(&["math", "statistics", "network"])),
        ("neural", Some(&["network"])),
        ("fantasy", Some(&["console"])),
        ("learning", Some(&["network"])),
        ("safe", Some(&["network"])),
        ("database", Some(&["embedded"])),
        ("robotics", Some(&["localization"])),
        ("thread", Some(&["storage"])),
        ("exchange", Some(&["twitch", "animation"])),
        ("animation", Some(&["kraken"])),
        ("bitcoin", Some(&["http", "day", "database", "key-value", "network", "wasm", "secp256k1", "client", "rpc", "websocket"])),
        ("solana", Some(&["http", "day", "database", "key-value", "network", "wasm", "secp256k1", "client", "cryptographic", "gfx", "sdk"])),
        ("exonum", Some(&["http", "day", "database", "key-value", "network", "wasm", "client"])),
        ("blockchain", Some(&["database", "key-value", "network", "wasm", "nosql", "orm", "driver", "fun", "rpc", "client", "server", "p2p", "networking", "websocket"])),
        ("cryptocurrencies", Some(&["database", "key-value", "network", "wasm", "nosql", "orm", "driver", "fun", "rpc", "client", "server", "p2p", "networking", "websocket"])),
        ("cryptocurrency", Some(&["database", "key-value", "network", "wasm", "nosql", "orm", "driver", "fun", "rpc", "client", "server", "p2p", "networking", "websocket", "twitch"])),
        ("ethereum", Some(&["http", "day", "nosql", "eth", "log", "generic", "network", "wasm", "key-value", "orm", "client", "database", "secp256k1", "websocket", "parity"])),
        ("iter", Some(&["math"])),
        ("ethernet", Some(&["eth"])),
        ("macro", Some(&["no-std", "no_std", "feature:no_std"])),
        ("macros", Some(&["no-std", "no_std", "feature:no_std"])),
        ("embedded", Some(&["no-std", "no_std", "feature:no_std"])),
        ("arm", Some(&["no-std", "no_std", "feature:no_std"])),
        ("float", Some(&["math"])),
        ("c64", Some(&["terminal", "core"])),
        ("emulator", Some(&["6502", "core", "gpu", "color", "timer"])),
        ("garbage", Some(&["tracing"])),
        ("terminal", Some(&["math", "emulator"])),
        ("terminal-emulator", Some(&["math", "emulator"])),
        ("editor", Some(&["terminal"])),
        ("build", Some(&["logic"])), // confuses categorization
        ("messaging", Some(&["matrix"])), // confuses categorization
        ("led", Some(&["matrix"])), // confuses categorization
        ("rgb", Some(&["matrix"])), // confuses categorization
        ("chat", Some(&["matrix"])), // confuses categorization
        ("math", Some(&["num", "symbolic", "algorithms", "algorithm", "utils"])), // confuses categorization
        ("mathematics", Some(&["num", "numeric", "symbolic", "algorithms", "algorithm", "utils"])), // confuses categorization
        ("cuda", Some(&["nvidia"])), // confuses categorization
        ("subcommand", Some(&["plugin"])),
        ("lint", Some(&["plugin"])),
        ("email", Some(&["validator", "validation"])),
        ("e-mail", Some(&["validator", "validation"])),
        ("template", Some(&["derive"])),
        ("dsl", Some(&["template"])),
        ("syn", Some(&["nom"])),
        ("cargo", Some(&["plugin"])),
        ("git", Some(&["terminal"])),
        ("nzxt", Some(&["kraken"])),
        ("wide", Some(&["windows", "win32"])),
        ("i18n", Some(&["text", "format", "message", "json", "ffi"])),
        ("l10n", Some(&["text", "format", "message", "json", "ffi"])),
        ("unicode", Some(&["text"])),
        ("parity", Some(&["fun", "backend"])),
        ("secp256k1", Some(&["fun", "backend", "alloc", "ecc"])),
        ("font", Some(&["text", "bitmap"])),
        ("freetype", Some(&["text", "bitmap"])),
        ("tex", Some(&["font"])),
        ("regex", Some(&["text", "linear", "time", "search"])),
        ("language", Some(&["server"])),
        ("server", Some(&["files"])),
        ("medical", Some(&["image"])),
        ("social", Some(&["media"])),
        ("codegen", Some(&["backend"])),
        ("game", Some(&["simulator", "simulation"])),
        ("vkontakte", Some(&["vk"])),
        ("vulkan", Some(&["vk"])),
        ("2d", Some(&["path", "paths"])),
        ("video", Some(&["audio"])), // have to pick one…
        ("sound", Some(&["3d", "windows"])),

        ("memory", Some(&["os", "system", "storage"])), // too generic

        ("data-structure", Some(&["no-std", "no_std"])), // it's a nice feature, but not defining one
        ("crypto", Some(&["no-std", "no_std"])), // it's a nice feature, but not defining one
        ("macro", Some(&["no-std", "no_std"])), // it's a nice feature, but not defining one
        ("parser", Some(&["no-std", "no_std", "game"])), // it's a nice feature, but not defining one
        ("cryptography", Some(&["no-std", "no_std"])), // it's a nice feature, but not defining one
        ("websocket", Some(&["http", "cli", "tokio", "client", "io", "network", "servo", "web"])), // there's a separate category for it
        ("rest", Some(&["api"])),
        ("cargo-subcommand", None),
        ("substrate", None),
        ("twitch", Some(&["kraken"])),
        ("chess", Some(&["bot"])),
        ("lichess", Some(&["bot"])),
        ("nftables", Some(&["nft"])),

        ("placeholder", None), // spam
        ("reserved", None), // spam
        ("name-squatting", None), // spam
        ("parked", None), // spam
        ("squatting", None), // spam
        ("malware", None), // spam
        ("unfinished", None), // spam
    ].iter().copied().collect();
}
