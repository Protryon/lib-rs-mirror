use CATEGORIES;
use std::collections::HashMap;
use std::collections::HashSet;

lazy_static! {
    /// If one is present, adjust score of a category
    ///
    /// `keyword: [(slug, multiply, add)]`
    pub(crate) static ref KEYWORD_CATEGORIES: Vec<(Cond, &'static [(&'static str, f64, f64)])> = [
        (Cond::Any(&["no-std", "no_std"]), &[("no-std", 1.4, 0.15), ("command-line-utilities", 0.5, 0.), ("cryptography::cryptocurrencies", 0.9, 0.)][..]),
        // derived from features
        (Cond::Any(&["feature:no_std", "feature:no-std", "heapless"]), &[("no-std", 1.2, 0.)]),
        (Cond::Any(&["print", "font", "parsing", "hashmap", "money", "flags", "data-structure", "cache", "macros", "wasm", "emulator", "hash"]), &[("no-std", 0.6, 0.)]),

        (Cond::Any(&["winsdk", "winrt", "directx", "dll", "win32", "winutil", "msdos", "winapi"]), &[("os::windows-apis", 1.5, 0.1), ("parser-implementations", 0.9, 0.), ("text-processing", 0.9, 0.)]),
        (Cond::All(&["windows", "ffi"]), &[("os::windows-apis", 1.1, 0.1)]),
        (Cond::All(&["windows"]), &[("os::windows-apis", 1.1, 0.1)]),
        (Cond::All(&["ffi", "winsdk"]), &[("os::windows-apis", 1.9, 0.5), ("science::math", 0.9, 0.)]),
        (Cond::All(&["ffi", "windows"]), &[("os::windows-apis", 1.2, 0.2)]),
        (Cond::Any(&["winauth", "ntlm"]), &[("os::windows-apis", 1.25, 0.2), ("authentication", 1.3, 0.2)]),

        (Cond::Any(&["windows", "winsdk", "win32", "activex"]), &[("os::macos-apis", 0., 0.), ("os::unix-apis", 0., 0.), ("science::math", 0.8, 0.)]),
        (Cond::Any(&["macos", "mac", "osx", "ios", "cocoa", "erlang"]), &[("os::windows-apis", 0., 0.), ("no-std", 0.01, 0.)]),
        (Cond::Any(&["macos", "mac", "osx", "cocoa", "mach-o", "uikit", "appkit"]), &[("os::macos-apis", 1.4, 0.2), ("science::math", 0.75, 0.)]),
        (Cond::All(&["os", "x"]), &[("os::macos-apis", 1.2, 0.)]),
        (Cond::Any(&["dmg"]), &[("os::macos-apis", 1.2, 0.1)]),
        (Cond::All(&["core", "foundation"]), &[("os::macos-apis", 1.2, 0.1), ("os", 0.8, 0.), ("concurrency", 0.3, 0.)]),
        (Cond::Any(&["corefoundation"]), &[("os::macos-apis", 1.2, 0.1), ("os", 0.8, 0.), ("concurrency", 0.3, 0.)]),
        (Cond::Any(&["core"]), &[("os::macos-apis", 1.05, 0.), ("os", 0.95, 0.), ("concurrency", 0.9, 0.)]),
        (Cond::Any(&["mount", "platforms", "platform", "package", "uname", "executable", "processes", "child",  "system", "boot", "kernel", "keycode"]),
            &[("os", 1.2, 0.1), ("network-programming", 0.7, 0.), ("cryptography", 0.5, 0.), ("games", 0.7, 0.), ("authentication", 0.6, 0.), ("localization", 0.7, 0.)]),
        (Cond::Any(&["dependency-manager", "debian", "deb", "package-manager", "clipboard", "process", "bootloader", "taskbar", "microkernel", "multiboot"]),
            &[("os", 1.2, 0.1), ("network-programming", 0.8, 0.), ("cryptography", 0.7, 0.), ("filesystem", 0.8, 0.), ("games", 0.2, 0.), ("authentication", 0.6, 0.), ("localization", 0.7, 0.)]),
        (Cond::All(&["device", "configuration"]), &[("os", 1.2, 0.2), ("config", 0.9, 0.)]),
        (Cond::All(&["os"]), &[("os", 1.2, 0.2), ("data-structures", 0.6, 0.)]),
        (Cond::Any(&["ios", "objective-c"]), &[("os::macos-apis", 1.1, 0.1)]),
        (Cond::Any(&["linux", "freebsd", "netbsd", "arch-linux", "pacman"]),
            &[("os", 1.1, 0.), ("os::unix-apis", 1.3, 0.1), ("os::macos-apis", 0., 0.), ("os::windows-apis", 0., 0.)]),
        (Cond::Any(&["redox", "rtos", "embedded"]), &[("os", 1.2, 0.1), ("os::macos-apis", 0., 0.), ("os::windows-apis", 0., 0.)]),
        (Cond::Any(&["rtos", "embedded", "microkernel"]), &[("embedded", 1.3, 0.1), ("science::math", 0.7, 0.)]),
        (Cond::All(&["operating", "system"]), &[("os", 1.2, 0.2)]),
        (Cond::All(&["file", "system"]), &[("filesystem", 1.2, 0.2)]),
        (Cond::Any(&["signal","epoll", "sigint", "syscall", "affinity", "ld_preload", "libnotify", "syslog", "systemd", "seccomp"]),
            &[("os::unix-apis", 1.2, 0.1), ("date-and-time", 0.1, 0.), ("games", 0.2, 0.), ("game-engines", 0.6, 0.), ("multimedia::images", 0.2, 0.),
            ("command-line-utilities", 0.6, 0.), ("development-tools", 0.8, 0.), ("science::math", 0.6, 0.)]),
        (Cond::Any(&["arch-linux", "unix", "archlinux", "docker", "pacman", "systemd", "posix", "x11", "epoll"]),
            &[("os::unix-apis", 1.2, 0.2), ("no-std", 0.5, 0.), ("os::windows-apis", 0.7, 0.), ("cryptography", 0.8, 0.), ("cryptography::cryptocurrencies", 0.6, 0.)]),
        (Cond::Any(&["ios", "android"]), &[("development-tools::profiling", 0.8, 0.), ("os::windows-apis", 0., 0.), ("development-tools::cargo-plugins", 0.8, 0.)]),
        (Cond::Any(&["cross-platform", "portable"]), &[("os::macos-apis", 0.25, 0.), ("os::windows-apis", 0.25, 0.), ("os::unix-apis", 0.25, 0.)]),
        (Cond::All(&["freebsd", "windows"]), &[("os::macos-apis", 0.6, 0.), ("os::windows-apis", 0.8, 0.), ("os::unix-apis", 0.8, 0.)]),
        (Cond::All(&["linux", "windows"]), &[("os::macos-apis", 0.5, 0.), ("os::windows-apis", 0.8, 0.), ("os::unix-apis", 0.8, 0.)]),
        (Cond::All(&["macos", "windows"]), &[("os::macos-apis", 0.8, 0.), ("os::windows-apis", 0.5, 0.), ("os::unix-apis", 0.5, 0.)]),

        (Cond::Any(&["ffi"]), &[("development-tools::ffi", 1.2, 0.), ("games", 0.1, 0.)]),
        (Cond::Any(&["sys"]), &[("development-tools::ffi", 0.9, 0.), ("games", 0.4, 0.)]),
        (Cond::Any(&["has:is_sys"]), &[("development-tools::ffi", 0.1, 0.), ("development-tools::debugging", 0.8, 0.), ("development-tools::websocket", 0.6, 0.), ("games", 0.2, 0.), ("multimedia", 0.8, 0.), ("cryptography", 0.8, 0.)]),
        (Cond::Any(&["bindgen"]), &[("development-tools::ffi", 1.5, 0.2)]),
        (Cond::All(&["interface", "api"]), &[("games", 0.2, 0.)]),
        (Cond::Any(&["bindings", "binding", "ffi-bindings", "wrapper", "api-wrapper"]),
            &[("development-tools::ffi", 0.8, 0.), ("games", 0.2, 0.), ("rendering::data-formats", 0.2, 0.)]),
        (Cond::Any(&["rgb", "palette"]), &[("command-line-utilities", 0.8, 0.)]),

        (Cond::Any(&["cargo"]), &[("development-tools", 1.1, 0.), ("development-tools::build-utils", 1.1, 0.),
            ("algorithms", 0.6, 0.), ("os", 0.7, 0.), ("os::macos-apis", 0.7, 0.), ("os::windows-apis", 0.7, 0.), ("cryptography::cryptocurrencies", 0.6, 0.)]),

        (Cond::Any(&["web", "chrome", "electron"]), &[("os::macos-apis", 0.5, 0.), ("filesystem", 0.8, 0.), ("os::unix-apis", 0.5, 0.), ("os::windows-apis", 0.5, 0.)]),
        (Cond::Any(&["wasm", "webasm", "webassembly"]), &[("wasm", 3., 0.7), ("embedded", 0.5, 0.), ("gui", 0.4, 0.), ("development-tools", 0.95, 0.), ("os::macos-apis", 0.5, 0.), ("os::unix-apis", 0.5, 0.), ("os::windows-apis", 0.5, 0.), ("command-line-utilities", 0.75, 0.)]),
        (Cond::Any(&["emscripten"]), &[("wasm", 1.1, 0.2), ("embedded", 0.3, 0.)]),
        (Cond::Any(&["parity", "mach-o", "intrusive", "cli"]), &[("wasm", 0.5, 0.), ("embedded", 0.8, 0.), ("development-tools::debugging", 0.8, 0.)]),
        (Cond::Any(&["native"]), &[("wasm", 0.5, 0.), ("web-programming", 0.5, 0.), ("multimedia::video", 0.8, 0.), ("multimedia", 0.8, 0.)]),

        (Cond::Any(&["api"]), &[("embedded", 0.9, 0.), ("web-programming::websocket", 0.9, 0.)]),
        (Cond::Any(&["sdk"]), &[("os", 1.05, 0.)]),
        (Cond::Any(&["toolchain", "tooling", "sdk", "compile-time", "compiler", "codegen", "asm", "cretonne", "llvm", "clang", "rustc", "cargo", "codebase"]),
                &[("development-tools", 1.2, 0.2), ("game-engines", 0.5, 0.), ("multimedia::audio", 0.8, 0.), ("concurrency", 0.9, 0.), ("games", 0.15, 0.)]),
        (Cond::All(&["code", "completion"]), &[("development-tools", 1.2, 0.2)]),
        (Cond::Any(&["framework", "generate", "generator", "precompiled", "precompile", "tools", "assets"]),
            &[("development-tools", 1.2, 0.15), ("development-tools::ffi", 1.3, 0.05)]),
        (Cond::Any(&["interface"]), &[("rust-patterns", 1.1, 0.), ("gui", 1.1, 0.), ("command-line-interface", 1.1, 0.)]),

        (Cond::Any(&["parser"]),
            &[("no-std", 0.85, 0.), ("embedded", 0.9, 0.), ("science", 0.9, 0.), ("development-tools", 0.8, 0.), ("development-tools::debugging", 0.8, 0.), ("rendering::graphics-api", 0.3, 0.), ("web-programming::http-client", 0.75, 0.), ("command-line-utilities", 0.75, 0.), ("command-line-interface", 0.5, 0.)]),
        (Cond::Any(&["git"]), &[("no-std", 0.85, 0.), ("command-line-interface", 0.5, 0.), ("cryptography::cryptocurrencies", 0.6, 0.)]),
        (Cond::Any(&["teaching"]), &[("gui", 0.1, 0.), ("rendering::engine", 0.1, 0.)]),

        (Cond::Any(&["openstreetmap", "osm", "geo", "gis", "geospatial", "triangulation", "seismology", "lidar"]),
            &[("science", 1.2, 0.2), ("science::math", 0.6, 0.), ("algorithms", 0.9, 0.), ("command-line-utilities", 0.75, 0.), ("cryptography::cryptocurrencies", 0.6, 0.)]), // geo
        (Cond::Any(&["astronomy", "ephemeris", "planet", "astro", "electromagnetism"]), &[("science", 1.2, 0.2), ("concurrency", 0.7, 0.), ("command-line-utilities", 0.75, 0.)]),
        (Cond::Any(&["bioinformatics", "benzene", "biological", "rna", "chemistry", "sensory", "interactomics", "transcriptomics"]),
            &[("science", 1.2, 0.3), ("science::math", 0.6, 0.), ("visualization", 1.1, 0.), ("algorithms", 0.7, 0.), ("command-line-utilities", 0.7, 0.)]),

        (Cond::All(&["validation", "api"]), &[("email", 0.7, 0.)]),

        (Cond::Any(&["tokenizer", "sanitizer", "parse", "lexer", "parser", "parsing"]),
            &[("science::math", 0.6, 0.), ("data-structures", 0.7, 0.), ("games", 0.5, 0.), ("os::macos-apis", 0.9, 0.), ("command-line-utilities", 0.8, 0.), ("command-line-interface", 0.5, 0.), ("text-editors", 0.5, 0.), ("development-tools::testing", 0.6, 0.)]),
        (Cond::Any(&["sanitizer", "nom"]), &[("parser-implementations", 1.3, 0.2)]),

        (Cond::Any(&["tokenizer", "parser-combinators", "peg", "lalr", "yacc", "ll1", "lexer", "context-free", "grammars", "grammar"]),
            &[("parsing", 1.2, 0.1), ("parser-implementations", 0.8, 0.)]),
        (Cond::Any(&["ll", "lr", "incremental"]),
            &[("parsing", 1.2, 0.), ("parser-implementations", 0.8, 0.)]),
        (Cond::Any(&["xml", "yaml", "syntex", "decoder", "mime", "html"]),
            &[("parsing", 0.8, 0.), ("parser-implementations", 1.2, 0.01)]),
        (Cond::Any(&[ "semver", "csv", "rss", "tex", "atoi", "ast", "syntax", "format", "iban"]),
            &[("parsing", 0.8, 0.), ("parser-implementations", 1.2, 0.01), ("os::macos-apis", 0.7, 0.), ("os::windows-apis", 0.7, 0.), ("os", 0.9, 0.)]),

        (Cond::All(&["parser", "nom"]), &[("parser-implementations", 1.3, 0.1)]),

        (Cond::Any(&["extraction", "serialization", "serializer", "serializes", "decoder", "decoding"]),
            &[("parser-implementations", 1.2, 0.1), ("parsing", 1.1, 0.), ("authentication", 0.8, 0.), ("value-formatting", 1.1, 0.), ("encoding", 1.2, 0.1)]),

        (Cond::All(&["machine", "learning"]), &[("science::ml", 1.5, 0.3), ("science::math", 0.8, 0.), ("science", 0.8, 0.), ("emulators", 0.15, 0.), ("command-line-utilities", 0.5, 0.)]),
        (Cond::All(&["neural", "network"]), &[("science::ml", 1.5, 0.3), ("science::math", 0.8, 0.), ("emulators", 0.15, 0.), ("command-line-utilities", 0.5, 0.)]),
        (Cond::Any(&["fuzzy-logic", "natural-language-processing", "nlp"]),
            &[("science", 1.25, 0.2), ("science::ml", 1.3, 0.1), ("games", 0.5, 0.), ("os", 0.8, 0.), ("game-engines", 0.75, 0.), ("algorithms", 1.2, 0.1), ("command-line-utilities", 0.8, 0.), ("command-line-interface", 0.4, 0.)]),
        (Cond::Any(&["blas", "tensorflow", "word2vec", "torch", "genetic-algorithm", "mnist", "deep-learning", "machine-learning", "neural-network", "neural-networks", "reinforcement", "perceptron"]),
            &[("science::ml", 1.25, 0.3), ("science::math", 0.8, 0.), ("science", 0.8, 0.), ("web-programming::http-client", 0.8, 0.), ("games", 0.5, 0.), ("os", 0.8, 0.), ("game-engines", 0.75, 0.), ("algorithms", 1.2, 0.), ("command-line-utilities", 0.8, 0.), ("command-line-interface", 0.4, 0.)]),
        (Cond::Any(&["bayesian", "classifier", "classify", "markov", "ai", "cuda", "svm", "nn", "rnn", "tensor", "learning", "statistics"]),
            &[("science::ml", 1.2, 0.), ("science::math", 0.9, 0.), ("algorithms", 1.1, 0.), ("web-programming::http-client", 0.8, 0.)]),
        (Cond::Any(&["math", "maths", "calculus", "geometry", "logic", "satisfiability", "combinatorics", "fft", "polynomial", "gaussian", "mathematics", "bignum", "prime", "primes", "linear-algebra", "algebra", "euler", "bijective"]),
            &[("science::math", 1.25, 0.3), ("science", 0.8, 0.), ("web-programming::http-client", 0.9, 0.), ("algorithms", 1.2, 0.1), ("games", 0.5, 0.), ("os", 0.8, 0.),("game-engines", 0.75, 0.), ("command-line-utilities", 0.8, 0.), ("command-line-interface", 0.4, 0.)]),
        (Cond::Any(&["optimization", "floating-point"]),
            &[("science::math", 0.8, 0.), ("science::ml", 0.9, 0.), ("science", 0.9, 0.), ("algorithms", 1.2, 0.1)]),
        (Cond::Any(&["physics"]),
            &[("science", 1.2, 0.), ("science::math", 0.8, 0.), ("science::ml", 0.7, 0.), ("game-engines", 1.1, 0.)]),
        (Cond::Any(&["read", "byte",  "ffi", "debuginfo", "debug", "api", "sys", "algorithms", "ieee754", "cast","macro", "ascii", "parser"]),
            &[("science::math", 0.6, 0.), ("science::ml", 0.8, 0.), ("science", 0.9, 0.), ("games", 0.8, 0.)]),
        (Cond::Any(&["openssl", "simd", "jit", "cipher", "sql", "collision", "data-structures", "plugin", "cargo",  "terminal", "game", "service", "piston", "system"]),
            &[("science::math", 0.6, 0.), ("encoding", 0.6, 0.), ("science::ml", 0.8, 0.), ("science", 0.9, 0.)]),
        (Cond::Any(&["algorithms", "algorithm"]),
            &[("algorithms", 1.1, 0.1), ("science::math", 0.8, 0.), ("science::ml", 0.8, 0.), ("science", 0.8, 0.)]),
        (Cond::All(&["gaussian", "blur"]),
            &[("science::math", 0.2, 0.), ("multimedia::images", 1.3, 0.2)]),
        (Cond::Any(&["hamming", "levenshtein"]),
            &[("algorithms", 1.2, 0.1), ("text-processing", 1.1, 0.), ("science::math", 0.5, 0.), ("rust-patterns", 0.5, 0.)]),

        (Cond::Any(&["tls", "ssl", "openssl"]),
            &[("network-programming", 1.1, 0.1), ("cryptography", 1.1, 0.05), ("science::math", 0.2, 0.), ("science", 0.7, 0.), ("cryptography::cryptocurrencies", 0.6, 0.), ("command-line-utilities", 0.75, 0.), ("development-tools::testing", 0.9, 0.)]),
        (Cond::Any(&["packet", "firewall"]), &[("network-programming", 1.1, 0.1), ("encoding", 0.9, 0.)]),
        (Cond::Any(&["cryptography", "cryptographic", "sponge", "ecdsa", "ed25519","argon2", "sha1", "shamir", "cipher", "aes", "rot13", "md5", "pkcs7", "keccak", "scrypt", "bcrypt", "digest", "chacha20"]),
            &[("cryptography", 1.4, 0.3), ("algorithms", 0.9, 0.), ("no-std", 0.95, 0.), ("command-line-utilities", 0.75, 0.), ("development-tools::testing", 0.8, 0.)]),
        (Cond::Any(&["ethereum", "bitcoin", "monero", "coinbase", "litecoin", "bitfinex", "nanocurrency", "cryptocurrency", "altcoin", "cryptocurrencies", "blockchain", "exonum"]),
            &[("cryptography::cryptocurrencies", 1.5, 0.44), ("science::math", 0.8, 0.), ("cryptography", 0.8, 0.), ("database-implementations", 0.8, 0.), ("value-formatting", 0.7, 0.), ("algorithms", 0.9, 0.), ("embedded", 0.8, 0.), ("no-std", 0.4, 0.), ("command-line-utilities", 0.8, 0.), ("development-tools::testing", 0.7, 0.), ("date-and-time", 0.4, 0.)]),
        (Cond::Any(&["parity", "stellar", "coin", "wallet", "bitstamp"]), &[("cryptography::cryptocurrencies", 1.3, 0.1), ("science::math", 0.8, 0.)]),

        (Cond::Any(&["tokio", "future", "futures", "promise", "non-blocking", "async"]),
            &[("asynchronous", 1.1, 0.1), ("os", 0.9, 0.), ("command-line-utilities", 0.75, 0.), ("cryptography::cryptocurrencies", 0.6, 0.), ("caching", 0.9, 0.), ("value-formatting", 0.5, 0.), ("games", 0.15, 0.)]),

        (Cond::Any(&["settings", "configuration", "config"]),
            &[("config", 1.15, 0.2), ("development-tools::debugging", 0.8, 0.), ("os::macos-apis", 0.95, 0.), ("command-line-utilities", 0.9, 0.), ("command-line-interface", 0.9, 0.)]),
        (Cond::Any(&["configure", "dotenv", "environment"]),
            &[("config", 1.2, 0.1), ("development-tools", 1.1, 0.), ("command-line-interface", 0.9, 0.)]),
        (Cond::Any(&["dlsym", "debug", "debugging", "debugger", "disassemlber", "demangle", "log", "logger", "logging", "dwarf", "backtrace", "valgrind", "lldb"]),
            &[("development-tools::debugging", 1.2, 0.1), ("concurrency", 0.9, 0.), ("algorithms", 0.7, 0.), ("emulators", 0.9, 0.), ("games", 0.01, 0.), ("development-tools::profiling", 0.5, 0.), ("command-line-utilities", 0.6, 0.)]),
        (Cond::Any(&["elf", "archive"]), &[("development-tools::debugging", 0.8, 0.), ("games", 0.4, 0.)]),
        (Cond::Any(&["elf"]), &[("encoding", 1.1, 0.), ("os::unix-apis", 1.1, 0.)]),
        (Cond::Any(&["travis", "jenkins", "ci", "testing", "test-driven", "tdd", "unittest", "testbed", "mocks"]),
            &[("development-tools::testing", 1.2, 0.3), ("development-tools::cargo-plugins", 0.9, 0.), ("development-tools", 0.75, 0.), ("games", 0.15, 0.), ("rendering::data-formats", 0.2, 0.), ("text-processing", 0.1, 0.)]),
        (Cond::All(&["gui", "automation"]), &[("development-tools::testing", 1.3, 0.3), ("gui", 0.25, 0.), ("os::macos-apis", 0.8, 0.)]),
        (Cond::Any(&["tests", "unittesting", "fuzzing"]), &[("development-tools::testing", 1.2, 0.2), ("development-tools", 0.9, 0.), ("development-tools::cargo-plugins", 0.9, 0.)]),
        (Cond::Any(&["integration", "test"]), &[("development-tools::testing", 1.2, 0.), ("date-and-time", 0.6, 0.)]),
        (Cond::Any(&["diff", "writer", "table", "gcd", "sh", "unwrap", "build", "relative", "path", "fail"]),
            &[("development-tools::testing", 0.5, 0.), ("internationalization", 0.7, 0.), ("gui", 0.7, 0.)]),
        (Cond::Any(&["string", "strings"]), &[("command-line-utilities", 0.5, 0.), ("multimedia::images", 0.5, 0.)]),
        (Cond::Any(&["rope"]), &[("command-line-utilities", 0.5, 0.), ("multimedia::images", 0.5, 0.)]),
        (Cond::Any(&["string", "binary", "streaming", "version", "buffer", "escape", "opengl", "memchr", "android", "ios", "recursive", "cuda"]), &[("development-tools::testing", 0.75, 0.), ("internationalization", 0.75, 0.)]),
        (Cond::Any(&["streams", "streaming"]), &[("algorithms", 1.1, 0.03), ("network-programming", 1.1, 0.)]),
        (Cond::Any(&["text", "boolean"]), &[("development-tools::testing", 0.9, 0.), ("multimedia::images", 0.8, 0.), ("internationalization", 0.9, 0.), ("rendering::data-formats", 0.8, 0.)]),

        (Cond::Any(&["ai", "piston", "logic", "2d", "graphic"]), &[("web-programming::http-client", 0.5, 0.), ("web-programming::websocket", 0.5, 0.)]),

        (Cond::Any(&["activitypub", "activitystreams", "pubsub"]), &[("web-programming", 1.25, 0.2), ("network-programming", 1.25, 0.2), ("web-programming::websocket", 1.1, 0.), ("command-line-utilities", 0.75, 0.)]),
        (Cond::Any(&["websocket", "websockets"]), &[("web-programming::websocket", 1.85, 0.4), ("command-line-utilities", 0.5, 0.)]),
        (Cond::Any(&["servo"]), &[("web-programming::websocket", 0.5, 0.), ("command-line-interface", 0.5, 0.)]),

        (Cond::Any(&["generic"]), &[("development-tools::debugging", 0.5, 0.), ("web-programming::websocket", 0.5, 0.)]),
        (Cond::Any(&["quaternion"]), &[("science::math", 1.1, 0.1), ("game-engines", 1.1, 0.), ("parsing", 0.25, 0.), ("algorithms", 0.9, 0.), ("command-line-utilities", 0.75, 0.)]),
        (Cond::Any(&["bitmap"]), &[("internationalization", 0.5, 0.)]),
        (Cond::Any(&["plural", "pluralize"]), &[("internationalization", 1.2, 0.1), ("localization", 1.2, 0.1)]),
        (Cond::Any(&["internationalisation", "i18n", "internationalization"]),
            &[("internationalization", 1.5, 0.3), ("localization", 0.75, 0.), ("value-formatting", 0.9, 0.), ("parsing", 0.8, 0.), ("os", 0.9, 0.), ("network-programming", 0.9, 0.), ("web-programming", 0.8, 0.), ("web-programming::http-server", 0.7, 0.)]),
        (Cond::Any(&["gettext"]), &[("internationalization", 1.3, 0.2)]),
        (Cond::Any(&["math"]), &[("rendering", 0.75, 0.), ("command-line-utilities", 0.75, 0.)]),
        (Cond::Any(&["rendering"]), &[("rendering::data-formats", 0.2, 0.), ("value-formatting", 0.8, 0.), ("hardware-support", 0.7, 0.)]),

        (Cond::Any(&["speech-recognition"]), &[("science", 1.3, 0.1),("multimedia::audio", 1.3, 0.1)]),
        (Cond::Any(&["tts", "speech"]), &[("multimedia::audio", 1.1, 0.), ("internationalization", 0.6, 0.)]),
        (Cond::Any(&["downsample", "dsp"]), &[("multimedia::audio", 1.2, 0.1)]),
        (Cond::Any(&["music", "flac", "vorbis", "chiptune", "synth", "chords", "audio", "sound", "sounds", "midi", "speech", "microphone", "pulseaudio"]),
            &[("multimedia::audio", 1.3, 0.3), ("command-line-utilities", 0.75, 0.), ("multimedia::images", 0.6, 0.), ("rendering::graphics-api", 0.75, 0.), ("cryptography::cryptocurrencies", 0.6, 0.), ("command-line-interface", 0.5, 0.), ("caching", 0.8, 0.)]),
        (Cond::Any(&["nyquist"]), &[("multimedia::audio", 1.1, 0.1), ("game-engines", 0.8, 0.)]),
        (Cond::All(&["mod", "tracker"]), &[("multimedia::audio", 1.1, 0.)]),
        (Cond::Any(&["perspective", "graphics", "cam"]), &[("multimedia::audio", 0.4, 0.)]),
        (Cond::Any(&["ffi", "sys", "daemon"]), &[("multimedia::audio", 0.9, 0.)]),
        (Cond::Any(&["sigabrt", "sigint"]), &[("multimedia::audio", 0.1, 0.), ("multimedia", 0.1, 0.)]),
        (Cond::Any(&["sigterm", "sigquit"]), &[("multimedia::audio", 0.1, 0.), ("multimedia", 0.1, 0.)]),

        (Cond::Any(&["multimedia", "chromecast", "media", "dvd"]), &[("multimedia", 1.3, 0.3), ("encoding", 0.5, 0.)]),
        (Cond::Any(&["image", "images", "viewer", "photos"]), &[("multimedia::images", 1.2, 0.1),("parsing", 0.6, 0.)]),
        (Cond::Any(&["imagemagick", "gamma", "photo", "exif", "openexr", "flif", "png", "jpeg", "svg", "pixel"]), &[("multimedia::images", 1.2, 0.1), ("encoding", 0.5, 0.), ("parsing", 0.6, 0.)]),
        (Cond::Any(&["color", "colors"]), &[("multimedia::images", 1.2, 0.1), ("multimedia", 1.1, 0.)]),
        (Cond::Any(&["quantization"]), &[("multimedia::images", 1.2, 0.1), ("multimedia", 1.1, 0.), ("command-line-interface", 0.2, 0.)]),
        (Cond::Any(&["webm", "av1", "dvd", "codec"]), &[("multimedia::encoding", 1.5, 0.2), ("multimedia::video", 1.4, 0.3), ("encoding", 0.15, 0.), ("parsing", 0.8, 0.)]),
        (Cond::Any(&["h265", "h264", "ffmpeg", "h263", "vp9", "video", "movies", "movie"]), &[("multimedia::video", 1.5, 0.3), ("multimedia::encoding", 1.3, 0.1), ("encoding", 0.15, 0.)]),
        (Cond::Any(&["opengl", "interpreter", "ascii", "mesh", "vulkan", "line"]), &[("multimedia::video", 0.5, 0.)]),
        (Cond::Any(&["reader"]), &[("multimedia::video", 0.85, 0.), ("parser-implementations", 1.1, 0.)]),
        (Cond::Any(&["timer"]), &[("multimedia::video", 0.8, 0.), ("multimedia", 0.8, 0.)]),
        (Cond::Any(&["sound"]), &[("multimedia::video", 0.9, 0.)]),

        (Cond::Any(&["gnuplot", "plotting", "codeviz", "viz", "chart", "plot", "visualizer"]),
            &[("visualization", 1.3, 0.3), ("science::math", 0.5, 0.), ("command-line-interface", 0.5, 0.), ("command-line-utilities", 0.75, 0.), ("games", 0.01, 0.), ("parsing", 0.6, 0.), ("caching", 0.5, 0.)]),

        (Cond::Any(&["interpreter", "jit", "zx", "emulator"]), &[("emulators", 1.25, 0.1), ("games", 0.7, 0.), ("multimedia::images", 0.5, 0.), ("command-line-interface", 0.5, 0.), ("multimedia::video", 0.5, 0.), ("command-line-utilities", 0.75, 0.)]),
        (Cond::Any(&["qemu", "vm"]), &[("emulators", 1.4, 0.1), ("multimedia::video", 0.5, 0.)]),
        (Cond::Any(&["security", "disassemlber"]), &[("emulators", 0.4, 0.)]),

        (Cond::Any(&["radix", "genetic"]), &[("science", 1.4, 0.), ("command-line-utilities", 0.75, 0.)]),

        (Cond::Any(&["protocol-specification"]), &[("gui", 0.5, 0.), ("command-line-utilities", 0.75, 0.)]),
        (Cond::Any(&["dsl", "embedded", "rtos"]), &[("gui", 0.75, 0.), ("command-line-utilities", 0.75, 0.)]),
        (Cond::Any(&["idl", "asmjs", "webasm"]), &[("gui", 0.5, 0.), ("command-line-utilities", 0.75, 0.)]),
        (Cond::Any(&["javascript"]), &[("gui", 0.9, 0.), ("command-line-utilities", 0.8, 0.), ("cryptography::cryptocurrencies", 0.6, 0.)]),

        (Cond::Any(&["concurrency", "spinlock", "parallel", "multithreaded", "barrier", "thread-local", "parallelism", "parallelizm", "coroutines", "threads", "threadpool", "fork-join", "parallelization", "actor"]),
            &[("concurrency", 1.35, 0.1), ("command-line-utilities", 0.8, 0.), ("games", 0.5, 0.), ("caching", 0.8, 0.), ("os", 0.8, 0.), ("parsing", 0.9, 0.), ("simulation", 0.8, 0.)]),
        (Cond::Any(&["atomic"]), &[("concurrency", 1.1, 0.1), ("command-line-utilities", 0.75, 0.)]),
        (Cond::Any(&["queue"]), &[("concurrency", 1.2, 0.)]),

        (Cond::Any(&["futures"]), &[("concurrency", 1.25, 0.1), ("asynchronous", 1.35, 0.3)]),
        (Cond::Any(&["events", "event"]), &[("asynchronous", 1.1, 0.), ("command-line-utilities", 0.8, 0.)]),
        (Cond::All(&["loop", "event"]), &[("game-engines", 1.2, 0.1), ("games", 0.4, 0.)]),
        (Cond::Any(&["consensus", "erlang", "gossip"]), &[("concurrency", 1.2, 0.1), ("network-programming", 1.2, 0.1), ("asynchronous", 1.2, 0.1)]),

        (Cond::Any(&["gui"]), &[("gui", 1.35, 0.1), ("command-line-interface", 0.15, 0.), ("multimedia::video", 0.5, 0.)]),
        (Cond::Any(&["qt", "x11", "wayland", "gtk"]), &[("gui", 1.35, 0.1), ("rendering::graphics-api", 1.1, 0.), ("os::unix-apis", 1.2, 0.1), ("cryptography::cryptocurrencies", 0.9, 0.), ("os::macos-apis", 0.25, 0.), ("caching", 0.5, 0.), ("command-line-interface", 0.15, 0.)]),
        (Cond::Any(&["window", "ui", "tui", "dashboard", "displaying", "desktop", "compositor"]), &[("gui", 1.2, 0.1), ("command-line-utilities", 0.9, 0.), ("hardware-support", 0.9, 0.), ("internationalization", 0.9, 0.)]),
        (Cond::Any(&["dashboard", "displaying", "inspector", "instrumentation"]), &[("visualization", 1.2, 0.1), ("games", 0.5, 0.)]),

        (Cond::Any(&["japanese", "arabic", "korean", "locale", "japan", "american", "uk", "country", "language-code"]),
            &[("localization", 1.2, 0.2), ("command-line-utilities", 0.75, 0.), ("rendering::engine", 0.1, 0.), ("rendering::data-formats", 0.2, 0.), ("filesystem", 0.2, 0.)]),
        (Cond::Any(&["l10n", "localization", "localisation"]), &[("localization", 1.3, 0.), ("internationalization", 0.7, 0.)]),
        (Cond::Any(&["make", "cmd"]), &[("localization", 0.2, 0.)]),

        (Cond::Any(&["time", "date", "week", "solar", "dow", "sunrise", "sunset", "moon", "calendar", "tz", "tzdata", "year", "stopwatch", "chrono"]),
            &[("date-and-time", 1.35, 0.2), ("value-formatting", 1.1, 0.), ("os", 0.8, 0.), ("command-line-interface", 0.7, 0.), ("parsing", 0.7, 0.), ("no-std", 0.95, 0.), ("games", 0.1, 0.), ("cryptography::cryptocurrencies", 0.8, 0.)]),
        (Cond::All(&["constant","time"]), &[("date-and-time", 0.4, 0.)]),
        (Cond::All(&["linear","time"]), &[("date-and-time", 0.8, 0.)]),
        (Cond::All(&["compile","time"]), &[("date-and-time", 0.4, 0.)]),
        (Cond::Any(&["uuid", "simulation", "failure", "fail", "iter", "domain", "engine", "kernel"]),
            &[("date-and-time", 0.4, 0.), ("value-formatting", 0.7, 0.)]),
        (Cond::Any(&["nan", "profile", "float", "timecode", "tsc", "fps", "arrow", "compiler"]),
            &[("date-and-time", 0.4, 0.), ("development-tools::debugging", 0.8, 0.)]),

        (Cond::Any(&["layout"]), &[("gui", 1.1, 0.06), ("rendering::graphics-api", 1.05, 0.), ("database", 0.7, 0.)]),

        (Cond::Any(&["cargo-subcommand"]), &[("development-tools::cargo-plugins", 1.8, 0.4), ("development-tools", 0.5, 0.), ("cryptography::cryptocurrencies", 0.6, 0.), ("development-tools::build-utils", 0.8, 0.)]),
        (Cond::All(&["cargo", "subcommand"]), &[("development-tools::cargo-plugins", 1.8, 0.4), ("development-tools", 0.7, 0.), ("development-tools::build-utils", 0.8, 0.), ("development-tools::testing", 0.8, 0.)]),
        (Cond::All(&["cargo", "sub-command"]), &[("development-tools::cargo-plugins", 1.8, 0.4), ("development-tools", 0.7, 0.), ("development-tools::build-utils", 0.8, 0.)]),
        (Cond::All(&["cargo"]), &[("development-tools::cargo-plugins", 1.2, 0.1), ("development-tools::build-utils", 1.1, 0.1)]),
        (Cond::All(&["build-dependencies"]), &[("config", 0.5, 0.), ("development-tools::build-utils", 1.5, 0.2)]),
        (Cond::All(&["development"]), &[("development-tools::build-utils", 1.1, 0.), ("development-tools::build-utils", 1.1, 0.)]),
        (Cond::All(&["build-time", "libtool", "build"]), &[("development-tools::build-utils", 1.2, 0.2), ("config", 0.9, 0.),("development-tools::cargo-plugins", 1.1, 0.)]),

        (Cond::Any(&["oauth", "2fa", "oauth2", "totp", "authorization", "authentication", "credentials"]),
            &[("authentication", 1.4, 0.2), ("command-line-utilities", 0.6, 0.), ("hardware-support", 0.7, 0.), ("web-programming::http-client", 0.8, 0.), ("parsing", 0.7, 0.)]),

        (Cond::Any(&["database", "datastore"]), &[("database-implementations", 1.3, 0.3), ("cryptography::cryptocurrencies", 0.9, 0.), ("database", 1.3, 0.1), ("caching", 0.88, 0.), ("development-tools", 0.9, 0.)]),
        (Cond::All(&["personal", "information", "management"]), &[("database-implementations", 1.5, 0.3)]),
        (Cond::Any(&["nosql", "geoip", "key-value", "tkiv", "transactions", "transactional"]), &[("database", 1.5, 0.3),("database-implementations", 1.2, 0.1), ("data-structures", 1.2, 0.1), ("command-line-utilities", 0.5, 0.)]),
        (Cond::Any(&["database", "db", "sqlite3", "sqlite", "postgres", "postgresql", "orm", "mysql","hadoop", "sqlite", "mongo","elasticsearch", "cassandra", "rocksdb", "redis", "couchdb", "diesel"]),
                &[("database", 1.4, 0.2), ("rust-patterns", 0.7, 0.), ("database-implementations", 1.1, 0.1), ("value-formatting", 0.7, 0.), ("hardware-support", 0.6, 0.),
                ("command-line-interface", 0.5, 0.), ("command-line-utilities", 0.9, 0.), ("memory-management", 0.7, 0.), ("development-tools", 0.9, 0.)]),
        (Cond::All(&["kv", "distributed"]), &[("database", 1.3, 0.2), ("database-implementations", 1.2, 0.1)]),
        (Cond::Any(&["rabbitmq", "amqp", "mqtt"]), &[("network-programming", 1.2, 0.), ("web-programming", 1.2, 0.), ("asynchronous", 1.2, 0.)]),

        (Cond::All(&["aws", "rusoto", "nextcloud"]), &[("network-programming", 1.2, 0.1), ("web-programming", 1.2, 0.1)]),
        (Cond::All(&["aws", "sdk"]), &[("network-programming", 1.2, 0.2), ("web-programming", 1.2, 0.1)]),
        (Cond::All(&["cloud", "google"]), &[("network-programming", 1.2, 0.1), ("web-programming", 1.2, 0.2)]),
        (Cond::Any(&["rusoto", "azure", "amazon"]), &[("network-programming", 1.3, 0.3), ("web-programming", 1.2, 0.1), ("cryptography::cryptocurrencies", 0.6, 0.)]),

        (Cond::Any(&["zlib", "libz", "7z", "lz4", "adler32", "brotli", "huffman", "xz", "lzma", "decompress", "compress", "compression", "rar", "archive", "archives", "zip"]),
            &[("compression", 1.3, 0.3), ("cryptography", 0.7, 0.), ("games", 0.4, 0.), ("command-line-interface", 0.4, 0.), ("command-line-utilities", 0.8, 0.), ("development-tools::testing", 0.6, 0.), ("development-tools::profiling", 0.2, 0.)]),

        (Cond::Any(&["simulation", "simulator"]), &[("simulation", 1.3, 0.3), ("emulators", 1.15, 0.1)]),
        (Cond::All(&["software", "implementation"]), &[("simulation", 1.3, 0.), ("emulators", 1.2, 0.)]),
        (Cond::Any(&["animation", "anim"]), &[("multimedia", 1.2, 0.), ("multimedia::video", 1.2, 0.1), ("rendering", 1.1, 0.), ("simulation", 0.7, 0.)]),

        (Cond::Any(&["rsync", "xmpp", "ldap", "ssh", "elb", "kademlia", "bittorrent", "sctp", "docker"]),
            &[("network-programming", 1.2, 0.2), ("web-programming", 0.6, 0.), ("os::windows-apis", 0.6, 0.)]),
        (Cond::Any(&["bot", "netsec", "waf", "curl", "net", "notification"]),
            &[("network-programming", 1.1, 0.1), ("web-programming", 1.1, 0.1), ("parsing", 0.8, 0.), ("development-tools::procedural-macro-helpers", 0.7, 0.)]),
        (Cond::Any(&["ip", "ipv6", "ipv4"]), &[("network-programming", 1.2, 0.1), ("web-programming", 1.1, 0.), ("parsing", 0.8, 0.)]),
        (Cond::Any(&["http2", "http", "https", "tcp", "tcp-client", "multicast", "anycast", "bgp", "amazon", "aws", "cloud", "service"]),
            &[("network-programming", 1.1, 0.1), ("filesystem", 0.8, 0.), ("asynchronous", 0.8, 0.), ("algorithms", 0.8, 0.), ("text-processing", 0.8, 0.),
            ("command-line-interface", 0.5, 0.), ("development-tools::procedural-macro-helpers", 0.8, 0.)]),
        (Cond::Any(&["ipfs", "io"]), &[("network-programming", 1.2, 0.1), ("filesystem", 1.3, 0.1), ("cryptography", 0.8, 0.), ("text-processing", 0.7, 0.), ("command-line-interface", 0.5, 0.)]),
        (Cond::Any(&["pipe", "read", "write"]), &[("filesystem", 1.1, 0.), ("development-tools::profiling", 0.6, 0.), ("science", 0.8, 0.)]),

        (Cond::Any(&["codegen", "references", "methods", "own", "function", "variables", "inference", "assert", "pointers", "pointer", "slices", "primitive", "primitives"]),
                &[("rust-patterns", 1.2, 0.1), ("command-line-utilities", 0.7, 0.), ("command-line-interface", 0.8, 0.), ("no-std", 0.95, 0.), ("asynchronous", 0.8, 0.),
                ("development-tools::testing", 0.9, 0.), ("internationalization", 0.7, 0.), ("template-engine", 0.8, 0.)]),
        (Cond::Any(&["endianness", "derive", "float", "floats", "floating-point", "initialized", "primitives", "tuple", "panic", "literal", "trait", "cow", "range", "annotation", "traits", "oop", "type", "types", "scope", "functions", "clone"]),
                &[("rust-patterns", 1.2, 0.1), ("no-std", 0.8, 0.), ("science", 0.8, 0.), ("science::ml", 0.7, 0.), ("science::math", 0.88, 0.), ("os", 0.9, 0.),
                ("command-line-utilities", 0.7, 0.), ("command-line-interface", 0.8, 0.), ("memory-management", 0.8, 0.), ("rendering", 0.8, 0.), ("template-engine", 0.8, 0.),
                ("hardware-support", 0.5, 0.), ("development-tools::cargo-plugins", 0.4, 0.), ("development-tools::testing", 0.8, 0.)]),
        (Cond::Any(&["u128", "closure",  "unwrap", "fnonce", "cell", "byteorder", "printf", "nightly", "std",  "macro", "null", "standard-library"]),
                &[("rust-patterns", 1.2, 0.1), ("algorithms", 0.8, 0.), ("science", 0.8, 0.), ("science::ml", 0.7, 0.), ("science::math", 0.88, 0.), ("os", 0.9, 0.), ("command-line-utilities", 0.7, 0.), ("command-line-interface", 0.8, 0.), ("memory-management", 0.8, 0.), ("rendering", 0.8, 0.), ("hardware-support", 0.6, 0.), ("development-tools::cargo-plugins", 0.4, 0.), ("development-tools::testing", 0.8, 0.)]),
        (Cond::Any(&["enum", "prelude", "boxing", "error", "error-handling", "println", "dsl"]),
                &[("rust-patterns", 1.2, 0.1), ("algorithms", 0.7, 0.), ("science", 0.7, 0.), ("science::ml", 0.7, 0.), ("science::math", 0.7, 0.), ("os", 0.8, 0.), ("command-line-utilities", 0.7, 0.), ("command-line-interface", 0.8, 0.), ("memory-management", 0.8, 0.), ("rendering", 0.8, 0.), ("hardware-support", 0.5, 0.), ("development-tools::cargo-plugins", 0.4, 0.), ("development-tools::ffi", 0.4, 0.), ("development-tools::testing", 0.7, 0.)]),
        (Cond::Any(&["singleton", "iterators", "newtype", "dictionary", "functor", "monad", "haskell","mutation", "monoidal", "monoid", "type-level", "bijective", "slice", "assert", "rustc", "string", "strings", "impl", "num", "struct"]),
            &[("rust-patterns", 1.1, 0.1), ("command-line-utilities", 0.7, 0.), ("development-tools", 0.8, 0.), ("memory-management", 0.8, 0.), ("command-line-interface", 0.8, 0.), ("games", 0.5, 0.)]),
        (Cond::Any(&["iterator", "stack", "type-inference", "builder", "nan"]),
            &[("rust-patterns", 1.1, 0.1), ("algorithms", 1.1, 0.1), ("gui", 0.9, 0.)]),
        (Cond::Any(&["structures", "data-structure", "trie", "incremental", "tree", "trees", "intersection", "structure", "endian", "big-endian", "binary", "binaries"]),
            &[("data-structures", 1.2, 0.1), ("algorithms", 1.1, 0.), ("science", 0.8, 0.), ("multimedia::audio", 0.9, 0.), ("command-line-utilities", 0.9, 0.), ("text-editors", 0.7, 0.), ("internationalization", 0.7, 0.)]),
        (Cond::All(&["structures", "data"]), &[("data-structures", 1.2, 0.2), ("algorithms", 0.9, 0.)]),
        (Cond::All(&["structure", "data"]), &[("data-structures", 1.2, 0.3), ("algorithms", 0.9, 0.)]),
        (Cond::Any(&["collection", "collections"]), &[("data-structures", 1.2, 0.1), ("algorithms", 0.9, 0.)]),
        (Cond::Any(&["safe", "unsafe", "specialized", "convenience", "helper", "helpers"]),
            &[("rust-patterns", 1.1, 0.), ("science", 0.8, 0.), ("cryptography::cryptocurrencies", 0.8, 0.)]),
        (Cond::Any(&["safe", "unsafe"]), &[("multimedia::video", 0.8, 0.), ("rendering::engine", 0.8, 0.)]),

        (Cond::Any(&["algorithms", "convert", "converter", "guid", "algorithm", "algorithmic", "algos"]),
            &[("algorithms", 1.2, 0.2), ("cryptography", 0.8, 0.), ("web-programming::http-client", 0.8, 0.), ("development-tools::testing", 0.5, 0.), ("development-tools", 0.5, 0.)]),
        (Cond::Any(&["implementation", "generator", "normalize", "random", "ordered", "set", "hierarchical", "multimap", "bitvector", "integers", "integer", "floating-point","partition", "abstractions","abstraction","sequences", "quadtree", "lookup",  "kernels", "sieve", "values"]),
            &[("algorithms", 1.1, 0.1), ("data-structures", 1.1, 0.1), ("science::math", 0.8, 0.), ("os", 0.8, 0.), ("games", 0.5, 0.), ("memory-management", 0.75, 0.), ("multimedia::video", 0.8, 0.)]),
        (Cond::Any(&["bloom", "arrays", "vec", "container", "octree", "map"]),
            &[("data-structures", 1.2, 0.1), ("algorithms", 1.1, 0.1), ("science::math", 0.8, 0.), ("os", 0.9, 0.), ("games", 0.8, 0.), ("memory-management", 0.75, 0.), ("multimedia::video", 0.8, 0.)]),
        (Cond::Any(&["concurrent", "producer", "condition",  "mutex", "futex"]), &[("concurrency", 1.3, 0.15), ("algorithms", 1.1, 0.1), ("data-structures", 0.95, 0.)]),
        (Cond::Any(&["scheduler", "lock", "deque", "channel"]), &[("concurrency", 1.3, 0.15), ("algorithms", 0.9, 0.), ("os", 1.1, 0.), ("data-structures", 0.8, 0.)]),
        (Cond::Any(&["persistent", "immutable"]), &[("algorithms", 1.25, 0.2), ("data-structures", 1.3, 0.1), ("database-implementations", 1.1, 0.1)]),

        (Cond::Any(&["statistics", "statistic", "order-statistics", "svd", "markov", "cognitive"]),
            &[("science", 1.2, 0.1), ("science::ml", 1.25, 0.), ("algorithms", 1.2, 0.1), ("data-structures", 1.2, 0.1), ("command-line-interface", 0.3, 0.), ("simulation", 0.75, 0.),("command-line-utilities", 0.75, 0.), ("games", 0.3, 0.), ("no-std", 0.95, 0.), ("caching", 0.8, 0.), ("development-tools::profiling", 0.6, 0.), ("internationalization", 0.7, 0.)]),
        (Cond::Any(&["variance", "units", "subsequences", "lazy", "linear", "distribution", "computation", "computational", "hpc", "tries", "collection", "pathfinding", "rational", "newtonian", "scientific", "science"]),
            &[("science", 1.25, 0.2), ("algorithms", 1.2, 0.1), ("data-structures", 1.2, 0.1), ("command-line-interface", 0.3, 0.), ("simulation", 0.75, 0.),("command-line-utilities", 0.75, 0.), ("games", 0.3, 0.), ("no-std", 0.95, 0.), ("caching", 0.8, 0.), ("development-tools::profiling", 0.6, 0.), ("internationalization", 0.7, 0.)]),
        (Cond::Any(&["median", "alpha", "equations", "matrix", "proving", "matrices", "multi-dimensional", "unification", "fibonacci", "interpolate", "interpolation", "dfa", "automata", "solvers", "solver", "integral"]),
            &[("science", 1.2, 0.1), ("science::math", 1.2, 0.1), ("algorithms", 1.2, 0.1), ("text-processing", 0.8, 0.), ("data-structures", 1.2, 0.1), ("command-line-interface", 0.3, 0.), ("simulation", 0.75, 0.),("command-line-utilities", 0.75, 0.), ("games", 0.3, 0.), ("no-std", 0.95, 0.), ("caching", 0.8, 0.), ("development-tools::profiling", 0.6, 0.), ("internationalization", 0.7, 0.)]),
        (Cond::Any(&["graph", "sparse", "summed", "kd-tree"]),
            &[("data-structures", 1.5, 0.2), ("algorithms", 1.3, 0.1), ("science", 1.2, 0.2), ("database", 0.7, 0.), ("concurrency", 0.9, 0.), ("command-line-interface", 0.3, 0.), ("command-line-utilities", 0.75, 0.)]),

        (Cond::Any(&["procedural", "procgen"]), &[("algorithms", 1.25, 0.2), ("game-engines", 1.25, 0.), ("games", 0.8, 0.), ("multimedia::images", 1.05, 0.)]),
        (Cond::All(&["finite", "state"]), &[("algorithms", 1.1, 0.), ("science::math", 0.8, 0.)]),
        (Cond::All(&["finite", "automata"]), &[("algorithms", 1.25, 0.2), ("science::math", 0.8, 0.)]),
        (Cond::All(&["machine", "state"]), &[("algorithms", 1.25, 0.2), ("science::math", 0.8, 0.)]),
        (Cond::All(&["fsm", "state"]), &[("algorithms", 1.25, 0.2), ("science::math", 0.8, 0.)]),
        (Cond::All(&["machine", "state", "logic", "fuzzy"]), &[("algorithms", 1.25, 0.2), ("science::math", 0.8, 0.)]),
        (Cond::Any(&["state-machine", "statemachine", "stateful"]), &[("algorithms", 1.25, 0.2), ("science", 1.1, 0.), ("science::math", 0.7, 0.)]),
        (Cond::Any(&["worker", "taskqueue", "a-star", "easing", "sorter", "sorting", "prng", "random", "mersenne"]),
                &[("algorithms", 1.25, 0.1), ("science::math", 0.8, 0.), ("caching", 0.8, 0.), ("command-line-interface", 0.4, 0.), ("database", 0.8, 0.), ("os", 0.9, 0.), ("command-line-utilities", 0.4, 0.)]),
        (Cond::Any(&["queue", "collection", "sort"]),
                &[("data-structures", 1.25, 0.1), ("science::math", 0.8, 0.), ("algorithms", 1.1, 0.), ("caching", 0.8, 0.), ("command-line-interface", 0.4, 0.), ("os", 0.9, 0.), ("command-line-utilities", 0.4, 0.)]),

        (Cond::Any(&["macro", "macros", "dsl", "procedural-macros", "proc-macro", "derive", "proc_macro", "custom-derive"]), &[
            ("development-tools::procedural-macro-helpers", 1.5, 0.2), ("cryptography", 0.7, 0.), ("memory-management", 0.7, 0.), ("algorithms", 0.8, 0.), ("science::math", 0.7, 0.),
            ("rust-patterns", 1.1, 0.2), ("web-programming::websocket", 0.6, 0.), ("no-std", 0.8, 0.), ("command-line-interface", 0.5, 0.),
            ("development-tools::testing", 0.8, 0.), ("development-tools::debugging", 0.8, 0.)]),
        (Cond::Any(&["similarity", "string"]), &[("development-tools::procedural-macro-helpers", 0.9, 0.), ("rust-patterns", 0.9, 0.)]),

        (Cond::Any(&["emoji", "stemming", "highlighting", "whitespace", "uppercase", "indentation", "spellcheck"]), &[("text-processing", 1.4, 0.2), ("science::math", 0.8, 0.), ("algorithms", 0.8, 0.)]),
        (Cond::Any(&["regex", "matching"]), &[("text-processing", 1.2, 0.1), ("science::math", 0.8, 0.), ("science::math", 0.8, 0.), ("science::math", 0.8, 0.)]),
        (Cond::Any(&["markdown", "common-mark", "pulldown-cmark", "bbcode", "braille", "ascii"]),
            &[("text-processing", 1.2, 0.2), ("parser-implementations", 1.2, 0.), ("parsing", 0.9, 0.), ("development-tools::testing", 0.2, 0.), ("development-tools", 0.7, 0.), ("multimedia::images", 0.5, 0.), ("command-line-utilities", 0.5, 0.)]),
        (Cond::Any(&["unicode", "grapheme", "crlf", "codepage", "whitespace", "utf-8", "utf8", "case", "case-folding", "text", "character-property", "character"]),
            &[("text-processing", 1.1, 0.2), ("rendering", 0.9, 0.), ("embedded", 0.8, 0.), ("web-programming", 0.9, 0.), ("rendering::data-formats", 0.5, 0.), ("development-tools::testing", 0.6, 0.)]),

        (Cond::Any(&["pdf", "epub", "ebook", "book", "typesetting", "xetex"]),
            &[("text-processing", 1.3, 0.2), ("science", 0.9, 0.), ("science::math", 0.8, 0.), ("rendering::data-formats", 1.2, 0.), ("rendering", 1.05, 0.), ("web-programming::http-client", 0.5, 0.), ("command-line-interface", 0.5, 0.)]),
        (Cond::All(&["auto", "correct"]), &[("text-processing", 1.2, 0.1), ("multimedia::images", 0.5, 0.)]),

        (Cond::Any(&["templating", "template", "template-engine", "handlebars"]),
            &[("template-engine", 1.4, 0.3), ("embedded", 0.2, 0.), ("command-line-interface", 0.4, 0.)]),

        (Cond::Any(&["benchmark", "bench", "profiler", "profiling", "perf"]),
            &[("development-tools::profiling", 1.2, 0.2), ("rust-patterns", 0.94, 0.), ("cryptography::cryptocurrencies", 0.7, 0.),
            ("simulation", 0.75, 0.), ("parsing", 0.8, 0.), ("os::macos-apis", 0.9, 0.), ("authentication", 0.5, 0.)]),
        (Cond::Any(&["version"]), &[("development-tools::profiling", 0.6, 0.)]),
        (Cond::Any(&["bump"]), &[("development-tools", 1.2, 0.), ("development-tools::profiling", 0.6, 0.)]),

        (Cond::Any(&["distributed"]), &[("text-processing", 0.4, 0.), ("command-line-interface", 0.4, 0.)]),
        (Cond::Any(&["filter", "download"]), &[("command-line-utilities", 0.75, 0.), ("command-line-interface", 0.5, 0.)]),
        (Cond::Any(&["error"]), &[("command-line-utilities", 0.5, 0.), ("command-line-interface", 0.7, 0.)]),
        (Cond::Any(&["serde", "encoding", "encode", "binary"]), &[("encoding", 1.3, 0.1), ("command-line-utilities", 0.5, 0.), ("command-line-interface", 0.7, 0.)]),
        (Cond::Any(&["json", "base64", "semver", "punycode", "syntex"]), &[("encoding", 1.2, 0.1), ("parsing", 1.2, 0.1), ("web-programming::websocket", 0.5, 0.), ("multimedia::encoding", 0.1, 0.)]),
        (Cond::Any(&["hash", "hashing", "sodium"]), &[("algorithms", 1.2, 0.1), ("cryptography", 1.1, 0.1), ("no-std", 0.9, 0.), ("memory-management", 0.7, 0.), ("development-tools", 0.7, 0.), ("command-line-utilities", 0.5, 0.)]),
        (Cond::Any(&["crc32", "fnv"]), &[("algorithms", 1.2, 0.1), ("cryptography", 0.4, 0.)]),

        (Cond::Any(&["pickle", "serde"]), &[("encoding", 1.3, 0.1), ("embedded", 0.9, 0.), ("development-tools", 0.8, 0.), ("parsing", 0.9, 0.), ("parser-implementations", 1.2, 0.1)]),
        (Cond::Any(&["manager"]), &[("encoding", 0.6, 0.), ("parser-implementations", 0.4, 0.)]),

        (Cond::Any(&["crypto", "nonce", "zero-knowledge", "cert", "certificate", "certificates", "pki", "cryptohash"]),
            &[("cryptography", 1.2, 0.2), ("algorithms", 0.9, 0.), ("no-std", 0.9, 0.), ("command-line-utilities", 0.6, 0.)]),
        (Cond::Any(&["secure", "keyfile", "key", "encrypt"]), &[("cryptography", 1.2, 0.), ("development-tools::ffi", 0.6, 0.)]),

        (Cond::Any(&["command-line-tool"]), &[("command-line-utilities", 1.2, 0.4)]),
        (Cond::Any(&["command-line-utility"]), &[("command-line-utilities", 1.2, 0.4)]),
        (Cond::All(&["command", "line"]), &[("command-line-utilities", 1.15, 0.1), ("command-line-interface", 1.15, 0.)]),
        (Cond::Any(&["commandline", "command-line", "cmdline"]),
            &[("command-line-utilities", 1.1, 0.1), ("command-line-interface", 1.1, 0.), ("development-tools::ffi", 0.7, 0.)]),

        (Cond::Any(&["numeral", "formatter", "notation", "pretty", "pretty-print", "pretty-printing", "punycode", "money", "units"]),
            &[("value-formatting", 1.2, 0.2), ("simulation", 0.5, 0.), ("wasm", 0.7, 0.)]),
        (Cond::Any(&["fpu", "simd", "comparison"]), &[("value-formatting", 0.5, 0.)]),
        (Cond::Any(&["math", "lint"]), &[("value-formatting", 0.9, 0.)]),

        (Cond::Any(&["roman", "phonenumber", "currency"]), &[("value-formatting", 1.2, 0.2), ("localization", 1.1, 0.)]),
        (Cond::Any(&["numbers", "numeric", "value"]), &[("value-formatting", 1.2, 0.), ("science", 1.2, 0.), ("encoding", 1.1, 0.), ("parsing", 1.1, 0.)]),
        (Cond::Any(&["bytes", "byte", "metadata"]), &[("value-formatting", 0.8, 0.)]),
        (Cond::Any(&["log", "logging", "serde", "utils", "nlp", "3d", "parser", "sdl2", "linear"]),
            &[("value-formatting", 0.7, 0.), ("database-implementations", 0.8, 0.), ("text-processing", 0.8, 0.), ("multimedia::images", 0.7, 0.), ("cryptography::cryptocurrencies", 0.8, 0.)]),
        (Cond::Any(&["performance", "bitflags", "storage", "terminal", "rpc"]),
            &[("value-formatting", 0.25, 0.), ("development-tools", 0.8, 0.), ("network-programming", 0.7, 0.), ("science::math", 0.8, 0.)]),

        (Cond::Any(&["pop3", "ssmtp", "smtp", "imap", "email"]), &[("email", 1.2, 0.3), ("parsing", 0.9, 0.), ("command-line-utilities", 0.75, 0.)]),
        (Cond::Any(&["editor", "vim", "emacs", "vscode", "sublime"]), &[("text-editors", 1.2, 0.2), ("games", 0.4, 0.), ("rendering::engine", 0.7, 0.)]),
        (Cond::Any(&["obj", "loop", "lattice", "api", "bin", "framework", "stopwatch", "sensor", "github", "algorithm", "protocol"]),
            &[("games", 0.5, 0.), ("development-tools::profiling", 0.8, 0.)]),
        (Cond::All(&["text", "editor"]), &[("text-editors", 1.4, 0.4), ("text-processing", 0.8, 0.), ("parsing", 0.5, 0.), ("internationalization", 0.1, 0.)]),
        (Cond::All(&["repl"]), &[("parsing", 0.7, 0.)]),

        (Cond::Any(&["terminal", "ncurses", "ansi", "progressbar", "vt100", "term", "console", "readline", "repl", "getopts"]),
            &[("command-line-interface", 1.2, 0.1), ("multimedia::images", 0.1, 0.), ("multimedia", 0.4, 0.), ("wasm", 0.9, 0.), ("science::math", 0.8, 0.), ("command-line-utilities", 0.75, 0.), ("internationalization", 0.9, 0.), ("development-tools::procedural-macro-helpers", 0.7, 0.)]),

        (Cond::Any(&["hardware", "verilog", "bluetooth", "drone", "rs232","enclave", "adafruit", "laser", "altimeter", "sensor", "cpuid", "tpu", "acpi", "uefi", "simd", "sgx", "raspberry", "raspberrypi", "broadcom", "usb", "scsi", "hdd"]),
                &[("hardware-support", 1.2, 0.3), ("command-line-utilities", 0.7, 0.), ("multimedia::images", 0.6, 0.), ("os", 0.9, 0.), ("development-tools::testing", 0.8, 0.), ("development-tools::procedural-macro-helpers", 0.6, 0.), ("development-tools", 0.9, 0.)]),
        (Cond::Any(&["hal", "keyboard", "joystick", "mouse", "enclave", "driver", "device"]),
                &[("hardware-support", 1.2, 0.3), ("command-line-utilities", 0.8, 0.), ("multimedia::images", 0.5, 0.), ("development-tools::testing", 0.8, 0.), ("development-tools::procedural-macro-helpers", 0.6, 0.), ("development-tools", 0.9, 0.)]),
        (Cond::All(&["hue", "light"]), &[("hardware-support", 1.2, 0.3)]),
        (Cond::All(&["controlling"]), &[("hardware-support", 1.1, 0.)]),
        (Cond::All(&["hue", "philips"]), &[("hardware-support", 1.2, 0.3)]),
        (Cond::Any(&["camera", "vesa", "ddcci", "ddc"]), &[("hardware-support", 1.1, 0.2),("multimedia::images", 1.2, 0.1)]),

        (Cond::Any(&["microcontrollers", "avr", "nickel", "bare-metal", "micropython", "6502", "sgx", "embedded"]),
            &[("embedded", 1.3, 0.25), ("no-std", 0.9, 0.), ("wasm", 0.7, 0.), ("web-programming", 0.7, 0.)]),
        (Cond::All(&["metal", "bare"]), &[("embedded", 1.3, 0.2), ("os", 0.9, 0.), ("no-std", 0.9, 0.)]),

        (Cond::Any(&["game", "utils", "json", "simulation", "turtle"]), &[("rendering::engine", 0.7, 0.)]),
        (Cond::Any(&["game", "games"]),
            &[("games", 1.25, 0.2), ("science::math", 0.6, 0.), ("science::ml", 0.7, 0.), ("development-tools::cargo-plugins", 0.7, 0.), ("rendering::engine", 0.8, 0.), ("embedded", 0.75, 0.), ("filesystem", 0.5, 0.),
            ("web-programming::http-client", 0.5, 0.), ("internationalization", 0.7, 0.), ("date-and-time", 0.3, 0.), ("development-tools::procedural-macro-helpers", 0.6, 0.)]),
        (Cond::Any(&["fun", "puzzle", "play", "steam", "conway", "starcraft", "roguelike", "minecraft", "sudoku"]), &[("games", 1.25, 0.3), ("rendering::engine", 0.8, 0.), ("cryptography", 0.5, 0.), ("command-line-utilities", 0.75, 0.)]),

        (Cond::Any(&["vector", "openai", "client"]), &[("games", 0.7, 0.), ("emulators", 0.7, 0.)]),
        (Cond::Any(&["convolution", "dsp", "movies", "movie"]), &[("games", 0.5, 0.)]),
        (Cond::All(&["gamedev", "engine"]), &[("game-engines", 1.5, 0.4), ("games", 0.1, 0.), ("multimedia::video", 0.5, 0.), ("rendering::data-formats", 0.8, 0.)]),
        (Cond::All(&["game", "ecs"]), &[("game-engines", 1.2, 0.), ("games", 0.4, 0.)]),
        (Cond::All(&["game", "parser"]), &[("game-engines", 1.1, 0.), ("rendering::engine", 0.1, 0.), ("games", 0.2, 0.)]),
        (Cond::All(&["chess", "engine"]), &[("game-engines", 1.5, 0.3), ("rendering::engine", 0.4, 0.), ("games", 0.4, 0.)]),
        (Cond::All(&["game", "scripting"]), &[("game-engines", 1.5, 0.3), ("rendering::engine", 0.4, 0.), ("games", 0.5, 0.), ("rendering::data-formats", 0.2, 0.)]),
        (Cond::All(&["game", "editor"]), &[("game-engines", 1.3, 0.1), ("rendering::engine", 0.4, 0.), ("games", 0.8, 0.), ("rendering::engine", 0.2, 0.)]),
        (Cond::All(&["game", "graphics"]), &[("game-engines", 1.3, 0.), ("games", 0.8, 0.)]),
        (Cond::All(&["game", "piston"]), &[("game-engines", 1.2, 0.1), ("games", 1.19, 0.1)]),
        (Cond::Any(&["piston"]), &[("game-engines", 1., 0.1), ("games", 1., 0.08)]),
        (Cond::Any(&["piston", "uuid", "scheduler", "countdown", "sleep"]), &[("development-tools::profiling", 0.2, 0.), ("algorithms", 0.8, 0.)]),
        (Cond::Any(&["timer"]), &[("development-tools::profiling", 0.8, 0.)]),
        (Cond::Any(&["engine", "amethyst"]), &[("game-engines", 1.3, 0.1), ("games", 0.7, 0.)]),
        (Cond::Any(&["gamedev", "game-dev", "game-development"]), &[("game-engines", 1.3, 0.2), ("games", 0.25, 0.), ("science", 0.5, 0.), ("concurrency", 0.75, 0.), ("science::ml", 0.8, 0.), ("science::math", 0.9, 0.), ("multimedia::video", 0.75, 0.)]),
        (Cond::Any(&["game-engine", "ecs", "game-engines"]), &[("game-engines", 1.5, 0.2), ("rendering::engine", 0.8, 0.), ("games", 0.95, 0.), ("command-line-utilities", 0.75, 0.), ("rendering::data-formats", 0.2, 0.)]),
        (Cond::All(&["game", "engine"]), &[("game-engines", 1.5, 0.3), ("games", 0.3, 0.), ("rendering::data-formats", 0.2, 0.), ("filesystem", 0.8, 0.), ("command-line-interface", 0.8, 0.)]),
        (Cond::Any(&["texture", "fps", "gamepad"]), &[("game-engines", 1.2, 0.1)]),
        (Cond::All(&["rendering", "engine"]), &[("rendering::engine", 1.5, 0.3), ("rendering::data-formats", 0.2, 0.)]),
        (Cond::Any(&["storage", "gluster"]), &[("game-engines", 0.5, 0.), ("rendering::engine", 0.1, 0.), ("rendering::data-formats", 0.1, 0.), ("filesystem", 1.2, 0.1), ("database", 1.2, 0.1)]),

        (Cond::Any(&["specs", "ecs", "http", "spider", "crawler"]), &[("command-line-utilities", 0.75, 0.)]),
        (Cond::Any(&["documentation"]), &[("rendering::data-formats", 0.2, 0.)]),

        (Cond::Any(&["basedir", "xdg", "nfs", "samba", "disk", "temporary-files", "temp-files", "tempfile", "temp-file", "backups", "backup",  "xattr", "ionotify", "inode", "directories", "dir", "filesystem", "fuse"]),
             &[("filesystem", 1.25, 0.3), ("command-line-interface", 0.3, 0.), ("no-std", 0.5, 0.),
             ("os", 0.95, 0.), ("gui", 0.9, 0.), ("science", 0.8, 0.), ("science::math", 0.3, 0.), ("development-tools", 0.95, 0.), ("cryptography", 0.6, 0.),
             ("asynchronous", 0.8, 0.), ("algorithms", 0.8, 0.), ("development-tools::testing", 0.9, 0.), ("command-line-utilities", 0.75, 0.)]),
        (Cond::Any(&["path", "files", "vfs", "glob"]),
             &[("filesystem", 1.2, 0.2), ("command-line-interface", 0.7, 0.), ("no-std", 0.6, 0.), ("cryptography", 0.6, 0.), ("development-tools::testing", 0.9, 0.), ("command-line-utilities", 0.8, 0.)]),
        (Cond::All(&["disk", "image"]),
             &[("filesystem", 1.3, 0.1), ("os", 1.3, 0.1), ("multimedia::images", 0.01, 0.)]),

        (Cond::Any(&["consistent", "checksum", "passphrase"]), &[("algorithms", 1.15, 0.1), ("cryptography", 1.05, 0.)]),
        (Cond::Any(&["encryption", "e2e", "keygen", "decryption", "password"]), &[("cryptography", 1.25, 0.2)]),
        (Cond::Any(&["overhead", "byte"]), &[("algorithms", 1.05, 0.), ("memory-management", 1.02, 0.)]),
        (Cond::Any(&["buffer", "buffered", "ringbuffer", "clone-on-write"]), &[("algorithms", 1.25, 0.2), ("memory-management", 1.25, 0.), ("caching", 1.2, 0.), ("network-programming", 0.25, 0.)]),
        (Cond::Any(&["memcached", "cache", "caching"]),
            &[("caching", 1.3, 0.2), ("memory-management", 1.1, 0.), ("data-structures", 0.7, 0.), ("algorithms", 0.7, 0.)]),
        (Cond::Any(&["allocate", "alloc", "allocator", "slab", "memory-allocator"]),
            &[("memory-management", 1.25, 0.1), ("caching", 0.8, 0.), ("algorithms", 0.8, 0.), ("game-engines", 0.7, 0.), ("development-tools", 0.8, 0.)]),
        (Cond::Any(&["memory", "garbage", "rc"]), &[("memory-management", 1.25, 0.1), ("development-tools::cargo-plugins", 0.8, 0.), ("development-tools::build-utils", 0.8, 0.), ("os", 1.1, 0.)]),

        (Cond::All(&["vector", "clock"]), &[("games", 0.25, 0.), ("algorithms", 1.25, 0.1), ("date-and-time", 0.3, 0.)]),
        (Cond::Any(&["vectorclock"]), &[("games", 0.25, 0.), ("algorithms", 1.5, 0.2), ("date-and-time", 0.3, 0.)]),

        (Cond::Any(&["cli", "utility", "utilities",  "tool", "command", "ripgrep", "tools"]),
            &[("command-line-utilities", 1.1, 0.2), ("internationalization", 0.8, 0.), ("games", 0.01, 0.), ("filesystem", 0.8, 0.), ("rendering::engine", 0.6, 0.), ("science", 0.9, 0.), ("simulation", 0.75, 0.)]),
        (Cond::All(&["cli", "utility"]),
            &[("command-line-utilities", 1.3, 0.3), ("command-line-interface", 0.3, 0.), ("games", 0.1, 0.), ("filesystem", 0.6, 0.), ("science", 0.8, 0.)]),
        (Cond::All(&["cli", "tool"]),
            &[("command-line-utilities", 1.3, 0.3), ("command-line-interface", 0.3, 0.), ("filesystem", 0.8, 0.), ("science", 0.8, 0.)]),
        (Cond::All(&["protocol", "web"]), &[("web-programming", 1.4, 0.1), ("parsing", 0.8, 0.), ("command-line-utilities", 0.75, 0.)]),
        (Cond::All(&["cloud", "web"]), &[("web-programming", 1.4, 0.2)]),
        (Cond::Any(&["web", "blog", "webdriver", "browsers", "browser", "cloud", "reqwest", "webhooks", "web-api"]),
            &[("web-programming", 1.2, 0.1), ("embedded", 0.9, 0.), ("development-tools::cargo-plugins", 0.5, 0.), ("emulators", 0.2, 0.)]),


        (Cond::Any(&["csv", "writer"]), &[("encoding", 1.2, 0.1), ("command-line-interface", 0.3, 0.), ("command-line-utilities", 0.75, 0.)]),
        (Cond::Any(&["html"]), &[("web-programming", 1.11, 0.), ("template-engine", 1.12, 0.), ("text-processing", 1.1, 0.)]),
        (Cond::All(&["static", "site"]), &[("web-programming", 1.11, 0.2), ("template-engine", 1.12, 0.2), ("text-processing", 1.1, 0.1)]),
        (Cond::Any(&["github", "ruby", "python", "gluon", "c", "esolang", "lisp", "java", "jni"]),
            &[("development-tools", 1.2, 0.12), ("development-tools::ffi", 1.3, 0.05), ("science", 0.9, 0.), ("os", 0.9, 0.), ("parser-implementations", 0.9, 0.), ("command-line-interface", 0.3, 0.), ("command-line-utilities", 0.75, 0.)]),
        (Cond::All(&["programming", "language"]), &[("development-tools", 1.4, 0.3), ("development-tools::ffi", 1.2, 0.05)]),
        (Cond::Any(&["runtime"]), &[("development-tools", 1.3, 0.1)]),

        (Cond::Any(&["iron", "kafka", "actix-web", "rest", "openid", "graphql", "restful", "http-server", "server", "micro-services", "webrtc"]),
            &[("web-programming::http-server", 1.2, 0.11), ("web-programming", 1.1, 0.), ("command-line-interface", 0.3, 0.),
            ("data-structures", 0.7, 0.),("command-line-utilities", 0.75, 0.), ("development-tools::cargo-plugins", 0.4, 0.)]),
        (Cond::All(&["web", "routing"]), &[("web-programming::http-server", 1.2, 0.1), ("command-line-utilities", 0.75, 0.)]),
        (Cond::All(&["language", "server"]), &[("web-programming::http-server", 0.2, 0.), ("development-tools", 1.2, 0.2)]),
        (Cond::All(&["lsp"]), &[("web-programming::http-server", 0.8, 0.), ("development-tools", 1.2, 0.)]),
        (Cond::All(&["web", "framework"]), &[("web-programming", 1.4, 0.2), ("web-programming::http-server", 1.2, 0.), ("command-line-utilities", 0.75, 0.)]),
        (Cond::All(&["wamp", "apache"]), &[("web-programming::http-server", 1.2, 0.1), ("web-programming::websocket", 0.9, 0.), ("command-line-utilities", 0.75, 0.)]),
        (Cond::Any(&["http", "dns", "dnssec", "grpc", "rpc", "json-rpc"]), &[("network-programming", 1.2, 0.), ("web-programming::websocket", 0.88, 0.), ("parsing", 0.9, 0.), ("value-formatting", 0.8, 0.), ("command-line-utilities", 0.9, 0.), ("development-tools::testing", 0.9, 0.)]),
        (Cond::Any(&["backend", "server-sent"]), &[("web-programming::http-server", 1.2, 0.1), ("command-line-utilities", 0.8, 0.)]),
        (Cond::Any(&["client"]), &[("web-programming", 1.1, 0.), ("network-programming", 1.1, 0.), ("web-programming::http-server", 0.9, 0.)]),
        (Cond::Any(&["kubernetes", "terraform", "coreos"]), &[("web-programming", 1.1, 0.), ("network-programming", 1.2, 0.)]),
        (Cond::All(&["http", "server"]), &[("web-programming::http-server", 1.2, 0.11)]),
        (Cond::All(&["http", "client"]), &[("web-programming", 1.2, 0.1), ("web-programming::http-server", 0.8, 0.), ("development-tools::procedural-macro-helpers", 0.2, 0.)]),
        (Cond::Any(&["http-client"]), &[("web-programming::http-client", 1.2, 0.1), ("web-programming::http-server", 0.8, 0.), ("command-line-utilities", 0.75, 0.), ("development-tools::procedural-macro-helpers", 0.4, 0.), ("development-tools", 0.75, 0.)]),
        (Cond::All(&["cli", "cloud"]), &[("web-programming::http-client", 1.2, 0.1), ("command-line-utilities", 1.2, 0.2)]),
        (Cond::Any(&["javascript", "stdweb", "sass", "lodash", "css", "webvr", "frontend", "emscripten", "asmjs", "slack", "url", "uri"]),
            &[("web-programming", 1.2, 0.2), ("command-line-utilities", 0.75, 0.), ("development-tools::testing", 0.6, 0.)]),
        (Cond::Any(&["json"]), &[("web-programming", 1.1, 0.1), ("algorithms", 0.8, 0.), ("command-line-utilities", 0.75, 0.), ("text-processing", 0.8, 0.)]),
        (Cond::Any(&["protocol", "network", "socket", "wifi"]), &[("network-programming", 1.2, 0.2), ("parsing", 0.8, 0.), ("os::windows-apis", 0.9, 0.), ("command-line-utilities", 0.75, 0.)]),
        (Cond::Any(&["protobuf", "proto"]), &[("network-programming", 1.2, 0.2), ("encoding", 1.2, 0.2), ("command-line-utilities", 0.75, 0.)]),
        (Cond::Any(&["p2p", "digitalocean"]), &[("network-programming", 1.4, 0.2), ("command-line-utilities", 0.75, 0.), ("development-tools", 0.75, 0.), ("multimedia", 0.5, 0.)]),

        (Cond::All(&["graphics", "bindings"]), &[("rendering::graphics-api", 1.34, 0.2)]),
        (Cond::All(&["graphics", "api"]), &[("rendering::graphics-api", 1.3, 0.15), ("parsing", 0.9, 0.), ("games", 0.2, 0.)]),
        (Cond::All(&["input"]), &[("rendering::graphics-api", 0.8, 0.)]),
        (Cond::Any(&["opengl", "gl", "skia", "vulkan", "vk", "directx", "direct2d", "glsl", "vulkan", "glium", "cairo", "freetype"]),
                &[("rendering::graphics-api", 1.3, 0.15), ("science::math", 0.8, 0.), ("rendering", 1.1, 0.1), ("rendering::data-formats", 0.9, 0.), ("web-programming::websocket", 0.15, 0.), ("rendering::graphics-api", 1.1, 0.1),("rendering::engine", 1.05, 0.05), ("games", 0.8, 0.), ("hardware-support", 0.8, 0.), ("development-tools::testing", 0.6, 0.)]),
        (Cond::Any(&["render", "bresenham", "oculus", "opengl-based", "gfx", "vr", "shader", "sprites", "nvidia", "ray", "renderer", "raytracing"]),
                &[("rendering", 1.15, 0.1), ("rendering::engine", 1.1, 0.05), ("rendering::data-formats", 0.9, 0.), ("web-programming::websocket", 0.15, 0.), ("rendering::graphics-api", 1.1, 0.1), ("games", 0.8, 0.), ("hardware-support", 0.8, 0.), ("development-tools::testing", 0.6, 0.)]),
        (Cond::Any(&["blender", "graphics", "image-processing"]), &[("multimedia::images", 1.2, 0.), ("rendering::graphics-api", 1.05, 0.), ("games", 0.8, 0.), ("simulation", 0.5, 0.)]),

        (Cond::Any(&["gpgpu"]), &[("asynchronous", 0.2, 0.), ("development-tools::testing", 0.8, 0.)]),
        (Cond::Any(&["validate", "windowing", "opencl"]), &[("games", 0.2, 0.), ("asynchronous", 0.2, 0.)]),

        (Cond::Any(&["fontconfig", "stdout"]), &[("web-programming::websocket", 0.25, 0.)]),
        (Cond::Any(&["font", "ttf", "truetype", "svg", "tesselation", "exporter", "mesh"]),
            &[("rendering::data-formats", 1.2, 0.1), ("gui", 0.8, 0.), ("parsing", 0.9, 0.), ("games", 0.5, 0.), ("web-programming::websocket", 0.25, 0.)]),
        (Cond::Any(&["loading", "loader", "algorithm", "gui", "git"]), &[("rendering::data-formats", 0.2, 0.)]),
        (Cond::Any(&["parsing", "game", "piston", "ascii"]), &[("rendering::data-formats", 0.7, 0.)]),
        (Cond::All(&["3d", "format"]), &[("rendering::data-formats", 1.3, 0.3), ("value-formatting", 0.5, 0.)]),
        (Cond::Any(&["2d", "3d", "sprite"]), &[("rendering::graphics-api", 1.11, 0.), ("data-structures", 1.1, 0.), ("rendering::data-formats", 1.2, 0.), ("rendering", 1.1, 0.), ("games", 0.8, 0.), ("multimedia::audio", 0.8, 0.), ("rendering::graphics-api", 1.1, 0.)]),

    ].iter().map(|s|*s).collect();
}

/// Based on the set of keywords, adjust relevance of given categories
///
/// Returns (weight, slug)
pub fn adjusted_relevance(mut candidates: HashMap<String, f64>, keywords: HashSet<String>, min_category_match_threshold: f64, max_num_categories: usize) -> Vec<(f64, String)> {
    for (cond, actions) in KEYWORD_CATEGORIES.iter() {
        if match cond {
            Cond::All(reqs) => {
                assert!(reqs.len() < 5);
                reqs.iter().all(|&k| keywords.contains(k))
            },
            Cond::Any(reqs) => reqs.iter().any(|&k| keywords.contains(k)),
        } {
            for &(slug, mul, add) in actions.iter() {
                assert!(CATEGORIES.from_slug(slug).next().is_some(), slug);
                assert!(mul >= 1.0 || add < 0.0000001, slug);
                let score = candidates.entry(slug.to_string()).or_insert(0.);
                *score *= mul;
                *score += add;
            }
        }
    }

    let max_score = candidates.iter()
        .map(|(_, v)| *v)
        .max_by(|a, b| a.partial_cmp(&b).unwrap())
        .unwrap_or(0.);

    let min_category_match_threshold = min_category_match_threshold.max(max_score * 0.951);

    let mut res: Vec<_> = candidates.clone().into_iter()
        .filter(|&(_, v)| v >= min_category_match_threshold)
        .filter(|&(ref k, _)| CATEGORIES.from_slug(k).next().is_some() /* FIXME: that checks top level only */)
        .take(max_num_categories)
        .map(|(k, v)| (v, k))
        .collect();
    res.sort_by(|a,b| b.0.partial_cmp(&a.0).unwrap());
    res
}


#[derive(Debug, Copy, Clone)]
pub(crate) enum Cond {
    Any(&'static [&'static str]),
    All(&'static [&'static str]),
}
