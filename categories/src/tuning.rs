use crate::CATEGORIES;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::BTreeMap;

lazy_static! {
    /// If one is present, adjust score of a category
    ///
    /// `keyword: [(slug, multiply, add)]`
    pub(crate) static ref KEYWORD_CATEGORIES: Vec<(Cond, &'static [(&'static str, f64, f64)])> = [
        (Cond::Any(&["no-std", "no_std"]), &[("no-std", 1.4, 0.15), ("command-line-utilities", 0.5, 0.), ("cryptography::cryptocurrencies", 0.9, 0.)][..]),
        // derived from features
        (Cond::Any(&["feature:no_std", "feature:no-std", "heapless"]), &[("no-std", 1.2, 0.05)]),
        (Cond::Any(&["feature:std"]), &[("no-std", 1., 0.)]),
        (Cond::Any(&["print", "font", "parsing", "hashmap", "money", "flags", "data-structure", "cache", "macros", "wasm", "emulator", "hash"]), &[("no-std", 0.6, 0.)]),

        (Cond::Any(&["winsdk", "winrt", "directx", "dll", "win32", "winutil", "msdos", "winapi"]),
            &[("os::windows-apis", 1.5, 0.1), ("parser-implementations", 0.9, 0.), ("text-processing", 0.9, 0.), ("text-editors", 0.8, 0.), ("no-std", 0.9, 0.)]),
        (Cond::All(&["windows", "ffi"]), &[("os::windows-apis", 1.1, 0.1), ("memory-management", 0.9, 0.)]),
        (Cond::Any(&["windows"]), &[("os::windows-apis", 1.1, 0.1), ("text-processing", 0.8, 0.)]),
        (Cond::All(&["ffi", "winsdk"]), &[("os::windows-apis", 1.9, 0.5), ("no-std", 0.5, 0.), ("parsing", 0.8, 0.), ("science::math", 0.9, 0.)]),
        (Cond::All(&["ffi", "windows"]), &[("os::windows-apis", 1.2, 0.2)]),
        (Cond::All(&["winrt", "com"]), &[("os::windows-apis", 1.2, 0.2)]),
        (Cond::All(&["windows", "uwp"]), &[("os::windows-apis", 1.2, 0.2)]),
        (Cond::All(&["microsoft", "api"]), &[("os::windows-apis", 1.2, 0.2)]),
        (Cond::Any(&["winauth", "ntlm"]), &[("os::windows-apis", 1.25, 0.2), ("authentication", 1.3, 0.2)]),
        (Cond::Any(&["winauth", "ntlm", "windows-runtime"]), &[("os::windows-apis", 1.25, 0.2), ("authentication", 1.3, 0.2)]),

        (Cond::Any(&["windows", "winsdk", "win32", "activex"]), &[("os::macos-apis", 0., 0.), ("os::unix-apis", 0., 0.), ("science::math", 0.8, 0.), ("memory-management", 0.9, 0.)]),
        (Cond::Any(&["macos", "osx", "ios", "cocoa", "erlang"]), &[("os::windows-apis", 0., 0.), ("no-std", 0.01, 0.)]),
        (Cond::Any(&["macos", "osx", "cocoa", "mach-o", "uikit", "appkit"]), &[("os::macos-apis", 1.4, 0.2), ("science::math", 0.75, 0.)]),
        (Cond::All(&["os", "x"]), &[("os::macos-apis", 1.2, 0.)]),
        (Cond::All(&["mac", "bindings"]), &[("os::macos-apis", 1.2, 0.), ("parsing", 0.8, 0.)]),
        (Cond::Any(&["dmg", "fsevents", "fseventsd"]), &[("os::macos-apis", 1.2, 0.1)]),
        (Cond::All(&["macos", "apis"]), &[("os::macos-apis", 1.15, 0.05)]),
        (Cond::All(&["core", "foundation"]), &[("os::macos-apis", 1.2, 0.1), ("os", 0.8, 0.), ("concurrency", 0.3, 0.)]),
        (Cond::Any(&["core-module"]), &[("os::macos-apis", 0.5, 0.)]),
        (Cond::Any(&["core-library"]), &[("os::macos-apis", 0.5, 0.)]),
        (Cond::Any(&["corefoundation"]), &[("os::macos-apis", 1.2, 0.1), ("os", 0.8, 0.), ("concurrency", 0.3, 0.)]),
        (Cond::All(&["mac"]), &[("os::macos-apis", 1.1, 0.)]),
        (Cond::Any(&["keycode", "platforms", "platform", "processor", "child", "system", "executable", "processes"]),
            &[("os", 1.2, 0.1), ("network-programming", 0.7, 0.), ("cryptography", 0.5, 0.), ("date-and-time", 0.8, 0.),
            ("games", 0.7, 0.), ("authentication", 0.6, 0.), ("internationalization", 0.7, 0.)]),
        (Cond::Any(&["mount", "package", "uname", "boot", "kernel"]),
            &[("os", 1.2, 0.1), ("network-programming", 0.7, 0.), ("cryptography", 0.5, 0.), ("date-and-time", 0.8, 0.),
            ("games", 0.7, 0.), ("multimedia", 0.8, 0.), ("multimedia::video", 0.7, 0.), ("multimedia::encoding", 0.8, 0.),
            ("rendering::engine", 0.8, 0.), ("rendering", 0.8, 0.), ("authentication", 0.6, 0.), ("internationalization", 0.7, 0.)]),
        (Cond::Any(&["dependency-manager", "package-manager", "debian-packaging", "clipboard", "process", "bootloader", "taskbar", "microkernel", "multiboot"]),
            &[("os", 1.2, 0.1), ("network-programming", 0.8, 0.), ("multimedia::video", 0.7, 0.), ("multimedia::encoding", 0.5, 0.), ("cryptography", 0.7, 0.), ("filesystem", 0.8, 0.), ("games", 0.2, 0.), ("authentication", 0.6, 0.),
            ("internationalization", 0.7, 0.)]),
        (Cond::All(&["package", "manager"]), &[("os", 1.2, 0.1), ("development-tools", 1.2, 0.1)]),
        (Cond::All(&["packaging"]), &[("os", 1.2, 0.), ("development-tools", 1.2, 0.1)]),
        (Cond::All(&["device", "configuration"]), &[("os", 1.2, 0.2), ("config", 0.9, 0.)]),
        (Cond::Any(&["os", "hostname"]), &[("os", 1.2, 0.1), ("data-structures", 0.6, 0.), ("no-std", 0.6, 0.)]),
        (Cond::All(&["shared", "library"]), &[("os", 1.2, 0.2), ("no-std", 0.3, 0.), ("config", 0.9, 0.)]),
        (Cond::Any(&["dlopen"]), &[("os", 1.2, 0.2), ("no-std", 0.3, 0.), ("config", 0.8, 0.)]),
        (Cond::Any(&["library"]), &[("games", 0.8, 0.), ("development-tools::cargo-plugins", 0.8, 0.)]),
        (Cond::Any(&["ios", "objective-c", "core-foundation"]), &[("os::macos-apis", 1.2, 0.1), ("no-std", 0.1, 0.)]),
        (Cond::Any(&["linux", "freebsd", "openbsd", "netbsd", "dragonflybsd", "arch-linux", "pacman", "deb", "rpm", "freebsd-apis", "linux-apis", "os-freebsd-apis", "os-linux-apis"]),
            &[("os", 1.1, 0.), ("os::unix-apis", 1.4, 0.1), ("os::macos-apis", 0.2, 0.), ("os::windows-apis", 0.1, 0.)]),
        (Cond::Any(&["sudo", "sudoers"]),
            &[("os", 1.1, 0.), ("os::unix-apis", 1.4, 0.1)]),
        (Cond::Any(&["bpf"]), &[("os", 1.1, 0.), ("os::unix-apis", 1.3, 0.1)]),
        (Cond::Any(&["ebpf", "libbpf"]), &[("os", 1.1, 0.), ("os::unix-apis", 1.3, 0.1)]),
        (Cond::Any(&["glib", "gobject", "gdk"]), &[("os", 1.1, 0.), ("parsing", 0.8, 0.), ("os::unix-apis", 1.4, 0.1)]),
        (Cond::Any(&["fedora", "centos", "redhat", "debian"]),
            &[("os::unix-apis", 1.3, 0.1), ("os::macos-apis", 0.1, 0.), ("os::windows-apis", 0.1, 0.)]),
        (Cond::All(&["linux", "kernel"]), &[("os::unix-apis", 1.3, 0.2), ("multimedia::encoding", 0.8, 0.)]),
        (Cond::All(&["api", "kernel"]), &[("os::unix-apis", 1.1, 0.), ("os", 1.1, 0.)]),
        (Cond::Any(&["dylib"]), &[("os", 1.1, 0.), ("os::windows-apis", 0.6, 0.)]),
        (Cond::Any(&["so"]), &[("os", 1.1, 0.), ("os::windows-apis", 0.6, 0.), ("os::macos-apis", 0.6, 0.)]),
        (Cond::Any(&["os"]), &[("os", 1.1, 0.), ("data-structures", 0.9, 0.)]),
        (Cond::Any(&["dll"]), &[("os", 1.1, 0.), ("os::unix-apis", 0.6, 0.), ("os::macos-apis", 0.6, 0.)]),
        (Cond::Any(&["redox", "rtos", "embedded", "hard-real-time"]), &[("os", 1.2, 0.1), ("gui", 0.8, 0.), ("rust-patterns", 0.8, 0.), ("os::macos-apis", 0., 0.), ("os::windows-apis", 0., 0.)]),
        (Cond::Any(&["rtos", "embedded", "microkernel", "hard-real-time", "tockos", "embedded-operating-system"]), &[("embedded", 1.3, 0.1), ("science::math", 0.7, 0.)]),
        (Cond::All(&["operating", "system"]), &[("os", 1.2, 0.2)]),
        (Cond::Any(&["signal"]),
            &[("os::unix-apis", 1.2, 0.), ("date-and-time", 0.4, 0.), ("memory-management", 0.8, 0.), ("games", 0.6, 0.),
            ("gui", 0.9, 0.), ("game-development", 0.8, 0.), ("multimedia::images", 0.5, 0.),
            ("command-line-utilities", 0.7, 0.), ("development-tools", 0.8, 0.), ("science::math", 0.7, 0.)]),
        (Cond::Any(&["autotools", "ld_preload", "libnotify", "syslog", "systemd", "seccomp", "kill", "ebpf"]),
            &[("os::unix-apis", 1.2, 0.05), ("date-and-time", 0.1, 0.), ("memory-management", 0.6, 0.), ("games", 0.2, 0.), ("multimedia::audio", 0.5, 0.),
            ("gui", 0.9, 0.), ("game-development", 0.6, 0.), ("multimedia::images", 0.2, 0.), ("no-std", 0.9, 0.), ("accessibility", 0.8, 0.),
            ("command-line-utilities", 0.8, 0.), ("science::math", 0.6, 0.)]),
        (Cond::Any(&["epoll", "affinity", "sigint", "syscall", "ioctl", "unix-socket", "unix-sockets"]),
            &[("os::unix-apis", 1.3, 0.1), ("date-and-time", 0.1, 0.), ("memory-management", 0.6, 0.), ("games", 0.2, 0.), ("multimedia::audio", 0.5, 0.),
            ("gui", 0.8, 0.), ("game-development", 0.6, 0.), ("multimedia::images", 0.2, 0.),
            ("command-line-utilities", 0.6, 0.), ("development-tools", 0.9, 0.), ("science::math", 0.6, 0.)]),
        (Cond::Any(&["arch-linux", "unix", "coreutils", "archlinux", "docker", "pacman", "systemd", "posix", "x11", "epoll"]),
            &[("os::unix-apis", 1.2, 0.2), ("no-std", 0.5, 0.), ("multimedia::audio", 0.8, 0.), ("os::windows-apis", 0.7, 0.), ("cryptography", 0.8, 0.), ("cryptography::cryptocurrencies", 0.6, 0.)]),
        (Cond::Any(&["users"]), &[("os::unix-apis", 1.1, 0.), ("caching", 0.8, 0.)]),
        (Cond::Any(&["hypervisor", "hyper-v", "efi"]), &[("os", 1.1, 0.1), ("hardware-support", 1.1, 0.1)]),
        (Cond::Any(&["docker", "docker-compose", "containerize", "containerized"]),
            &[("development-tools", 1.2, 0.1), ("web-programming", 1.1, 0.02), ("os::macos-apis", 0.9, 0.), ("config", 0.9, 0.), ("os::windows-apis", 0.1, 0.), ("command-line-utilities", 1.1, 0.02)]),
        (Cond::Any(&["kubernetes", "containerized", "k8s", "devops", "hypervisor"]),
            &[("development-tools", 1.2, 0.1), ("web-programming", 1.1, 0.02), ("web-programming::http-client", 0.9, 0.), ("no-std", 0.9, 0.),
            ("os::macos-apis", 0.8, 0.), ("config", 0.9, 0.), ("algorithms", 0.8, 0.), ("os::windows-apis", 0.1, 0.), ("command-line-utilities", 1.1, 0.02)]),
        (Cond::All(&["container", "tool"]), &[("development-tools", 1.2, 0.1)]),
        (Cond::All(&["deploy", "deployment"]), &[("development-tools", 1.2, 0.1), ("web-programming", 1.1, 0.02)]),
        (Cond::All(&["containers", "build"]), &[("development-tools", 1.2, 0.1)]),
        (Cond::Any(&["ios"]), &[("development-tools::profiling", 0.8, 0.), ("os::windows-apis", 0.1, 0.), ("no-std", 0.9, 0.), ("development-tools::cargo-plugins", 0.8, 0.)]),
        (Cond::Any(&["android"]), &[("os::macos-apis", 0.5, 0.), ("os::windows-apis", 0.7, 0.), ("os::unix-apis", 0.9, 0.), ("development-tools::profiling", 0.9, 0.)]),
        (Cond::Any(&["cross-platform", "portable"]), &[("os::macos-apis", 0.25, 0.), ("os::windows-apis", 0.25, 0.), ("os::unix-apis", 0.25, 0.)]),
        (Cond::All(&["cross", "platform"]), &[("os::macos-apis", 0.25, 0.), ("os::windows-apis", 0.25, 0.), ("os::unix-apis", 0.25, 0.)]),
        (Cond::All(&["freebsd", "windows"]), &[("os::macos-apis", 0.6, 0.), ("os::windows-apis", 0.8, 0.), ("os::unix-apis", 0.8, 0.)]),
        (Cond::All(&["linux", "windows"]), &[("os::macos-apis", 0.5, 0.), ("os::windows-apis", 0.8, 0.), ("os::unix-apis", 0.8, 0.)]),
        (Cond::All(&["macos", "windows"]), &[("os::macos-apis", 0.8, 0.), ("os::windows-apis", 0.5, 0.), ("os::unix-apis", 0.5, 0.), ("no-std", 0.9, 0.)]),
        (Cond::All(&["ios", "bindings"]), &[("os::macos-apis", 1.2, 0.1)]),
        (Cond::NotAny(&["ios", "objective-c", "objc", "obj-c", "objrs", "hfs", "osx", "os-x", "dylib", "mach", "xcode", "uikit", "appkit", "metal", "foundation", "macos", "mac", "apple", "cocoa"]), &[("os::macos-apis", 0.7, 0.)]),

        (Cond::NotAny(&["has:is_sys", "ffi", "sys", "bindings", "c", "libc", "libffi", "cstr", "python", "ruby", "lua", "jvm", "erlang", "unsafe"]),
            &[("development-tools::ffi", 0.7, 0.)]),
        (Cond::Any(&["ffi"]), &[("development-tools::ffi", 1.2, 0.), ("games", 0.1, 0.), ("filesystem", 0.9, 0.), ("compilers", 0.9, 0.)]),
        (Cond::Any(&["sys"]), &[("development-tools::ffi", 0.9, 0.), ("games", 0.4, 0.), ("asynchronous", 0.9, 0.), ("rendering::engine", 0.8, 0.), ("rendering", 0.8, 0.), ("multimedia", 0.9, 0.)]),
        (Cond::Any(&["has:is_sys"]), &[("development-tools::ffi", 0.1, 0.), ("no-std", 0.7, 0.), ("compilers", 0.7, 0.), ("algorithms", 0.8, 0.), ("data-structures", 0.7, 0.),
            ("development-tools::debugging", 0.8, 0.), ("web-programming::websocket", 0.6, 0.), ("development-tools::build-utils", 0.6, 0.),
            ("games", 0.2, 0.), ("filesystem", 0.9, 0.), ("multimedia", 0.8, 0.), ("cryptography", 0.8, 0.), ("command-line-utilities", 0.2, 0.)]),
        (Cond::Any(&["bindgen"]), &[("development-tools::ffi", 1.5, 0.2)]),
        (Cond::All(&["ffi", "has:is_dev"]), &[("development-tools::ffi", 1.2, 0.1)]),
        (Cond::All(&["ffi", "has:is_build"]), &[("development-tools::ffi", 1.2, 0.1)]),
        (Cond::All(&["interface", "api"]), &[("games", 0.2, 0.), ("algorithms", 0.8, 0.)]),
        (Cond::Any(&["bindings", "api-bindings", "binding", "ffi-bindings", "wrapper", "api-wrapper"]),
            &[("development-tools::ffi", 0.8, 0.), ("database-implementations", 0.8, 0.), ("games", 0.2, 0.), ("command-line-utilities", 0.2, 0.),
            ("no-std", 0.9, 0.), ("development-tools::cargo-plugins", 0.7, 0.), ("rust-patterns", 0.8, 0.), ("rendering::data-formats", 0.2, 0.)]),
        (Cond::Any(&["rgb", "palette"]), &[("command-line-utilities", 0.8, 0.), ("config", 0.8, 0.), ("compilers", 0.7, 0.)]),

        (Cond::Any(&["cargo", "rustup"]), &[("development-tools", 1.1, 0.), ("development-tools::build-utils", 1.1, 0.),
            ("algorithms", 0.6, 0.), ("os", 0.7, 0.), ("os::macos-apis", 0.7, 0.), ("os::windows-apis", 0.7, 0.), ("cryptography::cryptocurrencies", 0.6, 0.)]),
        (Cond::Any(&["scripts", "scripting"]), &[("development-tools", 1.1, 0.), ("compilers", 1.1, 0.), ("development-tools::build-utils", 1.1, 0.),
            ("algorithms", 0.9, 0.), ("cryptography::cryptocurrencies", 0.9, 0.)]),
        (Cond::All(&["compilation", "target"]), &[("development-tools", 1.1, 0.), ("compilers", 1.1, 0.), ("development-tools::build-utils", 1.1, 0.)]),
        (Cond::Any(&["build-tool", "build-time", "build-script", "build-system", "build-dependencies"]), &[("development-tools::build-utils", 1.2, 0.2),]),
        (Cond::All(&["build", "tool"]), &[("development-tools::build-utils", 1.1, 0.),]),
        (Cond::All(&["build", "script"]), &[("development-tools::build-utils", 1.1, 0.),]),
        (Cond::Any(&["pkg-config"]), &[("os::windows-apis", 0.5, 0.), ("config", 0.8, 0.), ("algorithms", 0.8, 0.), ("compilers", 0.7, 0.)]),
        (Cond::Any(&["autotools"]), &[("os::windows-apis", 0.7, 0.)]),

        (Cond::NotAny(&["wasm", "webasm", "asmjs", "webassembly", "web-assembly", "assembly", "wasi", "dep:wasi", "wasm-bindgen", "pwasm", "wasm32", "emscripten", "web-sys", "js", "frontend"]), &[("wasm", 0.6, 0.)]),
        (Cond::Any(&["web", "chrome", "electron"]), &[("os::macos-apis", 0.5, 0.), ("filesystem", 0.8, 0.), ("os::unix-apis", 0.5, 0.), ("os::windows-apis", 0.5, 0.)]),
        (Cond::Any(&["wasm", "webasm", "webassembly", "web-assembly"]),
            &[("wasm", 3., 0.7), ("embedded", 0.4, 0.), ("hardware-support", 0.9, 0.), ("gui", 0.4, 0.), ("no-std", 0.9, 0.), ("development-tools", 0.95, 0.), ("development-tools::ffi", 0.6, 0.),
            ("os::macos-apis", 0.5, 0.), ("os::unix-apis", 0.5, 0.), ("rust-patterns", 0.7, 0.), ("compilers", 0.7, 0.), ("os::windows-apis", 0.5, 0.), ("filesystem", 0.7, 0.),
            ("command-line-interface", 0.6, 0.), ("command-line-utilities", 0.75, 0.)]),
        (Cond::Any(&["emscripten", "wasi"]), &[("wasm", 1.1, 0.2), ("embedded", 0.3, 0.), ("no-std", 0.9, 0.), ("multimedia::encoding", 0.4, 0.)]),
        (Cond::Any(&["parity", "mach-o", "intrusive", "cli"]), &[("wasm", 0.5, 0.), ("embedded", 0.8, 0.), ("development-tools::debugging", 0.8, 0.)]),
        (Cond::Any(&["native"]), &[("wasm", 0.5, 0.), ("web-programming", 0.5, 0.), ("multimedia::encoding", 0.8, 0.), ("multimedia::video", 0.8, 0.), ("multimedia", 0.8, 0.)]),
        (Cond::Any(&["dep:wasm-bindgen", "wasm-bindgen", "dep:wasi"]), &[("wasm", 1.2, 0.)]),
        (Cond::All(&["web", "assembly"]), &[("wasm", 1.1, 0.)]),

        (Cond::NotAny(&["embedded", "dsp", "eeprom", "i2c", "arm", "sensor", "mems", "peripheral", "nordic", "riscv", "dep:bare-metal", "dep:cortex-m-rt", "dep:cortex-m", "svd2rust", "interrupt", "interrupts",
            "controller", "microcontroller", "microcontrollers", "analog", "lcd", "hw", "sdp", "bluez", "dep:btleplug", "hci", "xhci", "bluetooth",
            "ble", "cortex-m", "avr", "nickel", "device-drivers", "hal", "hardware-abstraction-layer", "driver", "register", "bare-metal",
            "crt0", "no-std", "stm32", "framebuffer", "no_std", "feature:no_std", "feature:no-std", "feature:std"]),
            &[("embedded", 0.7, 0.)]),
        (Cond::Any(&["svd2rust", "interrupt", "interrupts", "microcontroller", "microcontrollers", "analog", "sdp", "bluez", "dep:btleplug", "ble", "cortex-m", "avr", "nickel", "device-drivers", "hal", "hardware-abstraction-layer", "bare-metal", "crt0", "stm32"]),
            &[("embedded", 1.2, 0.1), ("hardware-support", 1.1, 0.)]),
        (Cond::Any(&["api"]), &[("embedded", 0.9, 0.), ("web-programming::websocket", 0.9, 0.)]),
        (Cond::All(&["embedded", "no-std"]), &[("embedded", 1.2, 0.2), ("no-std", 0.8, 0.)]),
        (Cond::All(&["embedded", "no_std"]), &[("embedded", 1.2, 0.2), ("no-std", 0.8, 0.)]),
        (Cond::Any(&["sdk"]), &[("os", 1.05, 0.), ("algorithms", 0.9, 0.), ("rust-patterns", 0.8, 0.), ("compilers", 0.8, 0.),]),
        (Cond::Any(&["compile-time"]), &[("compilers", 0.9, 0.), ("games", 0.5, 0.)]),
        (Cond::Any(&["compile-time", "codegen", "asm"]),
                &[("development-tools", 1.2, 0.2), ("rust-patterns", 1.1, 0.), ("game-development", 0.5, 0.), ("multimedia::audio", 0.8, 0.), ("concurrency", 0.9, 0.), ("games", 0.15, 0.)]),
        (Cond::Any(&["toolchain", "tooling", "sdk"]),
                &[("development-tools", 1.2, 0.2), ("game-development", 0.8, 0.), ("no-std", 0.8, 0.), ("multimedia::audio", 0.7, 0.), ("concurrency", 0.7, 0.), ("games", 0.5, 0.)]),
        (Cond::Any(&["llvm", "clang", "cretonne", "gcc"]),
                &[("development-tools", 1.2, 0.2), ("compilers", 1.2, 0.2), ("game-development", 0.8, 0.), ("no-std", 0.8, 0.), ("multimedia::audio", 0.7, 0.), ("concurrency", 0.7, 0.), ("games", 0.5, 0.)]),
        (Cond::Any(&[ "rustc", "cargo", "compiler"]),
                &[("development-tools", 1.2, 0.2), ("compilers", 1.2, 0.2), ("game-development", 0.5, 0.), ("network-programming", 0.8, 0.), ("development-tools::ffi", 0.7, 0.), ("multimedia::audio", 0.8, 0.), ("concurrency", 0.9, 0.), ("games", 0.15, 0.)]),
        (Cond::All(&["code", "completion"]), &[("development-tools", 1.2, 0.2)]),
        (Cond::Any(&["compilator"]), &[("development-tools", 1.1, 0.1), ("compilers", 1.2, 0.1)]),
        (Cond::Any(&["tree-sitter"]), &[("development-tools", 1.2, 0.1), ("compilers", 1.2, 0.1), ("parser-implementations", 1.2, 0.)]),
        (Cond::Any(&["code", "generation", "codebase"]), &[("visualization", 0.7, 0.), ("date-and-time", 0.7, 0.)]),
        (Cond::Any(&["framework", "generate", "generator", "precompiled", "precompile", "tools", "assets"]),
            &[("development-tools", 1.2, 0.15), ("development-tools::ffi", 1.3, 0.05), ("date-and-time", 0.6, 0.)]),
        (Cond::Any(&["interface"]), &[("rust-patterns", 1.1, 0.), ("gui", 1.1, 0.), ("command-line-interface", 1.1, 0.)]),

        (Cond::Any(&["lang", "programming-language"]), &[("compilers", 1.1, 0.05)]),
        (Cond::Any(&["typescript-compiler", "ecmascript-parser", "javascript-parser"]), &[("compilers", 1.4, 0.3), ("web-programming", 1.4, 0.3)]),
        (Cond::All(&["javascript", "compiler"]), &[("compilers", 1.2, 0.1), ("web-programming", 1.2, 0.1)]),

        (Cond::Any(&["git"]), &[("no-std", 0.85, 0.), ("command-line-interface", 0.5, 0.), ("algorithms", 0.9, 0.), ("cryptography::cryptocurrencies", 0.6, 0.)]),
        (Cond::Any(&["teaching"]), &[("gui", 0.1, 0.), ("rendering::engine", 0.1, 0.)]),

        (Cond::Any(&["gis", "latitude", "geospatial", "triangulation", "seismology", "lidar"]),
            &[("science", 1.2, 0.2), ("science::math", 0.6, 0.), ("algorithms", 0.9, 0.), ("no-std", 0.9, 0.), ("rust-patterns", 0.5, 0.), ("command-line-utilities", 0.75, 0.), ("config", 0.8, 0.), ("cryptography::cryptocurrencies", 0.6, 0.)]), // geo
        (Cond::Any(&["openstreetmap", "geojson", "osm", "geography", "geo", "geos", "wgs-84", "ephemeris"]),
            &[("science", 1.2, 0.2), ("science::math", 0.6, 0.), ("algorithms", 0.9, 0.), ("command-line-utilities", 0.75, 0.), ("cryptography::cryptocurrencies", 0.6, 0.)]), // geo
        (Cond::Any(&["dep:geo", "dep:gdal", "dep:geo-types"]),
            &[("science", 1.2, 0.2), ("compilers", 0.7, 0.)]),
        (Cond::Any(&["astronomy", "planet", "chemistry", "astro",  "geodesic", "geocoding", "wgs84", "electromagnetism"]),
            &[("science", 1.2, 0.2), ("rust-patterns", 0.5, 0.), ("concurrency", 0.7, 0.), ("compilers", 0.7, 0.), ("no-std", 0.9, 0.), ("command-line-utilities", 0.75, 0.)]),
        (Cond::Any(&["bioinformatics", "bio", "benzene", "chemical", "biological", "rna", "genotype", "genbank"]),
            &[("science", 1.2, 0.3), ("science::math", 0.6, 0.), ("visualization", 1.1, 0.), ("no-std", 0.9, 0.), ("algorithms", 0.7, 0.),
            ("parsing", 0.8, 0.), ("encoding", 0.9, 0.), ("embedded", 0.7, 0.), ("asynchronous", 0.7, 0.), ("command-line-utilities", 0.8, 0.)]),
        (Cond::Any(&["chemistry", "dep:bio", "sensory", "phenotype", "interactomics", "genomics", "molecules", "transcriptomics"]),
            &[("science", 1.2, 0.3), ("science::math", 0.6, 0.), ("visualization", 1.1, 0.), ("compilers", 0.7, 0.), ("no-std", 0.9, 0.), ("rust-patterns", 0.5, 0.), ("algorithms", 0.7, 0.), ("config", 0.8, 0.), ("command-line-utilities", 0.7, 0.)]),

        (Cond::All(&["validation", "api"]), &[("email", 0.7, 0.), ("multimedia::encoding", 0.7, 0.)]),

        (Cond::Any(&["parser", "syntex"]),
            &[("parser-implementations", 1.1, 0.), ("parsing", 1.08, 0.), ("no-std", 0.7, 0.), ("embedded", 0.9, 0.), ("science::robotics", 0.7, 0.), ("science", 0.9, 0.), ("development-tools", 0.8, 0.),
            ("development-tools::debugging", 0.8, 0.), ("rendering::graphics-api", 0.3, 0.), ("rendering", 0.6, 0.), ("gui", 0.8, 0.), ("web-programming::http-client", 0.75, 0.),
            ("command-line-utilities", 0.75, 0.), ("command-line-interface", 0.5, 0.), ("games", 0.7, 0.), ("visualization", 0.7, 0.)]),
        (Cond::All(&["elf", "parser"]), &[("parser-implementations", 1.2, 0.2), ("os::unix-apis", 1.1, 0.)]),
        (Cond::All(&["format", "parser"]), &[("parser-implementations", 1.3, 0.2), ("no-std", 0.8, 0.), ("parsing", 0.95, 0.), ("games", 0.8, 0.), ("development-tools::ffi", 0.8, 0.)]),
        (Cond::All(&["file", "format"]), &[("parser-implementations", 1.2, 0.1), ("parsing", 0.8, 0.), ("encoding", 1.1, 0.), ("algorithms", 0.8, 0.), ("web-programming::http-server", 0.8, 0.)]),
        (Cond::All(&["file", "format", "parser"]), &[("parser-implementations", 1.2, 0.1), ("algorithms", 0.8, 0.), ("parsing", 0.4, 0.), ("development-tools::ffi", 0.8, 0.)]),
        (Cond::Any(&["tokenizer", "sanitizer", "parse", "lexer", "parser", "parsing"]),
            &[("science::math", 0.6, 0.), ("data-structures", 0.7, 0.), ("os", 0.7, 0.), ("config", 0.9, 0.), ("games", 0.5, 0.),
            ("os::macos-apis", 0.9, 0.), ("command-line-utilities", 0.8, 0.), ("simulation", 0.8, 0.), ("no-std", 0.9, 0.), ("encoding", 0.8, 0.),
            ("command-line-interface", 0.5, 0.), ("text-editors", 0.5, 0.), ("development-tools::testing", 0.6, 0.)]),
        (Cond::Any(&["sanitizer", "nom"]), &[("parser-implementations", 1.3, 0.2), ("algorithms", 0.8, 0.), ("encoding", 0.8, 0.)]),
        (Cond::Any(&["tokenizer", "lexer", "parser", "jwt", "macro", "rpc"]), &[("encoding", 0.8, 0.), ("no-std", 0.7, 0.), ("emulators", 0.6, 0.), ("rendering::graphics-api", 0.9, 0.)]),

        (Cond::Any(&["tokenizer", "tokenize", "parser-combinators", "peg", "lalr", "yacc"]),
            &[("parsing", 1.2, 0.1), ("parser-implementations", 0.8, 0.), ("os", 0.7, 0.), ("emulators", 0.7, 0.), ("compilers", 0.8, 0.), ("internationalization", 0.8, 0.), ("games", 0.8, 0.)]),
        (Cond::Any(&["tokenizers", "tokenizer"]), &[("text-processing", 1.1, 0.1)]),
        (Cond::Any(&["combinator", "ll1", "lexer", "lex", "context-free", "grammars", "grammar"]),
            &[("parsing", 1.2, 0.1), ("parser-implementations", 0.8, 0.), ("os", 0.7, 0.), ("emulators", 0.7, 0.), ("internationalization", 0.8, 0.), ("games", 0.8, 0.)]),
        (Cond::All(&["parser", "generator"]), &[("parsing", 1.4, 0.3), ("parser-implementations", 0.9, 0.), ("compilers", 0.8, 0.), ("gui", 0.5, 0.)]),
        (Cond::Any(&["parser-generator"]), &[("parsing", 1.4, 0.3), ("parser-implementations", 0.9, 0.), ("gui", 0.5, 0.)]),
        (Cond::Any(&["backus–naur", "bnf"]), &[("parsing", 1.2, 0.), ("parser-implementations", 1.2, 0.)]),
        (Cond::All(&["parser", "combinators"]), &[("parsing", 1.3, 0.2), ("parser-implementations", 0.8, 0.), ("no-std", 0.9, 0.), ("multimedia::encoding", 0.5, 0.)]),
        (Cond::All(&["parser", "combinator"]), &[("parsing", 1.3, 0.2), ("parser-implementations", 0.8, 0.), ("no-std", 0.9, 0.), ("compilers", 0.8, 0.), ("multimedia::encoding", 0.5, 0.)]),
        (Cond::Any(&["dep:nom", "dep:lalrpop-util", "dep:lalrpop", "dep:pest_derive"]), &[("parser-implementations", 1.2, 0.1), ("parsing", 0.7, 0.)]),
        (Cond::Any(&["validator", "rfc", "glsl", "2d", "3d", "uris", "ftp", "savegame", "game", "json", "database", "protocol", "microsoft", "email"]),
            &[("parsing", 0.6, 0.)]),
        (Cond::Any(&["ll", "lr", "incremental"]),
            &[("parsing", 1.2, 0.), ("parser-implementations", 0.8, 0.)]),
        (Cond::Any(&["syntex", "decoder", "mime", "html", "dep:peg", "dep:pest"]),
            &[("parser-implementations", 1.2, 0.01), ("parsing", 0.6, 0.), ("caching", 0.8, 0.), ("no-std", 0.8, 0.)]),
        (Cond::Any(&["json", "asn1", "ue4", "javascript", "scraper", "blockchain", "irc", "twitch"]),
            &[("parsing", 0.1, 0.),]),
        (Cond::Any(&["xml", "yaml", "csv", "rss", "tex"]),
            &[("parsing", 0.3, 0.), ("parser-implementations", 1.2, 0.01), ("rust-patterns", 0.7, 0.), ("no-std", 0.9, 0.), ("compilers", 0.8, 0.), ("data-structures", 0.8, 0.), ("os::macos-apis", 0.7, 0.), ("development-tools::ffi", 0.9, 0.),
            ("os::windows-apis", 0.7, 0.), ("os", 0.9, 0.), ("multimedia", 0.7, 0.), ("multimedia::encoding", 0.7, 0.)]),
        (Cond::All(&["xml", "parser"]), &[("parsing", 0.3, 0.), ("parser-implementations", 1.2, 0.01)]),
        (Cond::Any(&["font", "dom", "files", "language", "formats", "lua", "format", "asn", "loader", "dep:serde"]), &[("parsing", 0.5, 0.)]),
        (Cond::Any(&["semver", "atoi", "ast", "syntax", "format", "iban"]),
            &[("parsing", 0.8, 0.), ("parser-implementations", 1.2, 0.01), ("os::macos-apis", 0.7, 0.), ("development-tools::ffi", 0.9, 0.),
            ("os::windows-apis", 0.7, 0.), ("os", 0.9, 0.), ("network-programming", 0.9, 0.), ("web-programming", 0.9, 0.), ("web-programming::http-server", 0.7, 0.)]),

        (Cond::All(&["parser", "nom"]), &[("parser-implementations", 1.3, 0.1), ("algorithms", 0.8, 0.), ("multimedia::encoding", 0.4, 0.)]),

        (Cond::Any(&["extraction", "serialization", "deserializer"]),
            &[("parser-implementations", 1.25, 0.2), ("parsing", 1.1, 0.), ("authentication", 0.8, 0.), ("network-programming", 0.8, 0.), ("value-formatting", 1.1, 0.), ("encoding", 1.2, 0.1)]),
        (Cond::Any(&["deserialize", "serializer", "serializes", "decoder", "decoding"]),
            &[("parser-implementations", 1.2, 0.15), ("parsing", 1.1, 0.), ("authentication", 0.8, 0.), ("compilers", 0.9, 0.), ("network-programming", 0.8, 0.), ("value-formatting", 1.1, 0.), ("encoding", 1.2, 0.1)]),

        (Cond::All(&["machine", "learning"]), &[("science::ml", 1.5, 0.3), ("network-programming", 0.8, 0.), ("science::math", 0.8, 0.), ("science", 0.8, 0.),
            ("emulators", 0.15, 0.), ("command-line-utilities", 0.5, 0.), ("games", 0.5, 0.), ("no-std", 0.9, 0.), ("gui", 0.8, 0.)]),
        (Cond::All(&["decision", "tree"]), &[("science::ml", 1.2, 0.1), ("algorithms", 1.1, 0.)]),
        (Cond::All(&["neural", "classifier"]), &[("science::ml", 1.5, 0.3), ("science::math", 0.8, 0.), ("network-programming", 0.5, 0.)]),
        (Cond::All(&["neural", "network"]), &[("science::ml", 1.5, 0.3), ("science::math", 0.8, 0.), ("network-programming", 0.5, 0.),
            ("emulators", 0.2, 0.), ("command-line-utilities", 0.6, 0.), ("development-tools::build-utils", 0.2, 0.)]),
        (Cond::All(&["deep", "learning"]), &[("science::ml", 1.5, 0.3), ("science::math", 0.8, 0.), ("network-programming", 0.5, 0.),
            ("emulators", 0.2, 0.), ("command-line-utilities", 0.6, 0.), ("development-tools::build-utils", 0.2, 0.)]),
        (Cond::Any(&["fuzzy-logic"]),
            &[("science", 1.25, 0.2), ("science::ml", 1.3, 0.1), ("games", 0.5, 0.), ("os", 0.8, 0.), ("algorithms", 1.2, 0.1), ("command-line-utilities", 0.8, 0.), ("command-line-interface", 0.4, 0.)]),
        (Cond::Any(&["natural-language-processing", "nlp", "language-processing", "language-recognition", "fasttext", "embeddings", "word2vec", "layered-nlp"]),
            &[("science", 1.2, 0.15), ("text-processing", 1.2, 0.2), ("science::ml", 1.3, 0.1), ("games", 0.5, 0.), ("wasm", 0.8, 0.), ("os", 0.8, 0.), ("game-development", 0.75, 0.),
            ("algorithms", 1.2, 0.1), ("command-line-utilities", 0.8, 0.), ("command-line-interface", 0.4, 0.)]),
        (Cond::Any(&["blas", "tensorflow", "tensor-flow", "word2vec", "deep-learning-framework", "torch", "decision-tree", "genetic-algorithm",
            "mnist", "deep-learning", "neuralnetworks", "neuralnetwork", "machine-learning"]),
            &[("science::ml", 1.25, 0.3), ("science::math", 0.8, 0.), ("development-tools", 0.8, 0.), ("science", 0.7, 0.), ("web-programming::http-client", 0.8, 0.), ("parsing", 0.8, 0.), ("games", 0.5, 0.), ("os", 0.8, 0.), ("game-development", 0.75, 0.), ("algorithms", 1.2, 0.), ("command-line-utilities", 0.8, 0.), ("command-line-interface", 0.4, 0.)]),
        (Cond::Any(&["neural-network", "machinelearning", "hyperparameter", "backpropagation", "cudnn", "randomforest", "neural-networks", "deep-neural-networks", "reinforcement", "perceptron"]),
            &[("science::ml", 1.25, 0.3), ("science::math", 0.8, 0.), ("science", 0.7, 0.), ("web-programming::http-client", 0.8, 0.), ("games", 0.5, 0.), ("os", 0.8, 0.), ("game-development", 0.75, 0.), ("compilers", 0.5, 0.), ("algorithms", 1.2, 0.), ("command-line-utilities", 0.8, 0.), ("command-line-interface", 0.4, 0.)]),
        (Cond::Any(&["bayesian", "bayes", "classifier", "classify", "markov", "ai", "cuda", "svm", "nn", "rnn", "tensor", "learning", "statistics"]),
            &[("science::ml", 1.2, 0.), ("science::math", 0.9, 0.), ("no-std", 0.9, 0.), ("algorithms", 1.1, 0.), ("web-programming::http-client", 0.8, 0.), ("development-tools", 0.9, 0.), ("development-tools::build-utils", 0.6, 0.)]),
        (Cond::Any(&["math", "maths", "calculus", "geometry", "calculator", "logic", "satisfiability", "haar", "combinatorics", "fft", "discrete"]),
            &[("science::math", 1.25, 0.3), ("algorithms", 1.2, 0.1), ("database", 0.8, 0.), ("web-programming::http-client", 0.9, 0.), ("config", 0.8, 0.), ("rendering::graphics-api", 0.8, 0.), ("games", 0.5, 0.), ("os", 0.8, 0.),("game-development", 0.75, 0.), ("command-line-utilities", 0.8, 0.), ("command-line-interface", 0.4, 0.)]),
        (Cond::Any(&["polynomial", "sigmoid", "numerics", "gaussian", "mathematics", "mathematical", "voronoi", "gmp", "bignum", "prime", "primes", "linear-algebra", "numpy", "lexicographic", "algebra", "euler", "bijective"]),
            &[("science::math", 1.25, 0.3), ("algorithms", 1.2, 0.1), ("web-programming::http-client", 0.9, 0.), ("rendering::graphics-api", 0.8, 0.), ("games", 0.5, 0.), ("os", 0.8, 0.),("game-development", 0.75, 0.), ("command-line-utilities", 0.7, 0.), ("command-line-interface", 0.4, 0.)]),
        (Cond::Any(&["arithmetic", "dihedral", "arithmetics", "tanh", "histogram", "arbitrary-precision", "algebraic", "topology"]),
            &[("science::math", 1.25, 0.15), ("algorithms", 1.2, 0.), ("text-processing", 0.8, 0.)]),
        (Cond::Any(&["precision", "computational", "polygon", "dep:num-complex"]), &[("science::math", 1.1, 0.)]),
        (Cond::Any(&["fractal"]), &[("science::math", 1.2, 0.1), ("multimedia::images", 1.1, 0.1)]),
        (Cond::All(&["discrete", "transforms"]),
            &[("science::math", 1.25, 0.1), ("algorithms", 1.1, 0.), ("simulation", 0.9, 0.)]),
        (Cond::Any(&["dep:nalgebra", "dep:num-complex", "dep:alga", "dep:openblas-src"]), &[("science::math", 1.2, 0.), ("science", 1.1, 0.), ("compilers", 0.7, 0.), ("algorithms", 1.1, 0.)]),
        (Cond::Any(&["optimization", "floating-point"]),
            &[("science::math", 0.8, 0.), ("science::ml", 0.9, 0.), ("science", 0.9, 0.), ("algorithms", 1.2, 0.1)]),
        (Cond::All(&["computer", "vision"]),
            &[("science::ml", 1.2, 0.1), ("science", 1.1, 0.), ("no-std", 0.9, 0.), ("algorithms", 1.1, 0.), ("multimedia::images", 1.1, 0.)]),
        (Cond::Any(&["computer-vision"]),
            &[("science::ml", 1.3, 0.3), ("multimedia::images", 1.3, 0.3), ("science", 1.1, 0.), ("no-std", 0.8, 0.), ("compilers", 0.5, 0.), ("algorithms", 1.1, 0.)]),
        (Cond::Any(&["physics", "ncollide", "dynamics", "pressure"]),
            &[("science", 1.1, 0.1), ("simulation", 1.25, 0.1), ("multimedia::video", 0.8, 0.), ("no-std", 0.9, 0.), ("game-development", 1.1, 0.), ("science::math", 0.8, 0.), ("parsing", 0.8, 0.), ("parser-implementations", 0.8, 0.), ("science::ml", 0.7, 0.), ]),
        (Cond::Any(&["collision", "aabb"]),
            &[("simulation", 1.2, 0.), ("game-development", 1.1, 0.), ("network-programming", 0.8, 0.), ("data-structures", 0.9, 0.), ("multimedia::audio", 0.7, 0.), ("parsing", 0.8, 0.), ("parser-implementations", 0.8, 0.), ("science::ml", 0.7, 0.)]),
        (Cond::All(&["rigid", "body"]),
            &[("simulation", 1.2, 0.), ("game-development", 1.1, 0.)]),
        (Cond::All(&["rigid", "joints"]),
            &[("simulation", 1.2, 0.), ("game-development", 1.1, 0.)]),
        (Cond::Any(&["read", "byte",  "ffi", "debuginfo", "debug", "api", "sys", "algorithms", "ieee754", "cast","macro", "ascii", "parser"]),
            &[("science::math", 0.6, 0.), ("science::ml", 0.8, 0.), ("science", 0.9, 0.), ("games", 0.8, 0.)]),
        (Cond::Any(&["simd", "jit", "cipher", "sql", "service", "data-structures", "plugin", "system"]),
            &[("science::math", 0.6, 0.), ("encoding", 0.6, 0.), ("science::ml", 0.8, 0.), ("science", 0.9, 0.), ("wasm", 0.8, 0.)]),
        (Cond::Any(&["cargo", "openssl", "terminal", "game", "collision", "piston"]),
            &[("science::math", 0.6, 0.), ("encoding", 0.6, 0.), ("memory-management", 0.6, 0.), ("internationalization", 0.9, 0.),
            ("database", 0.6, 0.), ("science::ml", 0.8, 0.), ("science", 0.9, 0.), ("wasm", 0.8, 0.)]),
        (Cond::Any(&["algorithms", "algorithm", "copy-on-write"]),
            &[("algorithms", 1.1, 0.1), ("science::math", 0.8, 0.), ("science::ml", 0.8, 0.), ("science", 0.8, 0.)]),
        (Cond::All(&["gaussian", "blur"]),
            &[("science::math", 0.2, 0.), ("multimedia::images", 1.3, 0.2), ("compilers", 0.7, 0.)]),
        (Cond::Any(&["hamming", "levenshtein"]),
            &[("algorithms", 1.2, 0.1), ("text-processing", 1.1, 0.), ("internationalization", 0.8, 0.), ("science::math", 0.5, 0.), ("rust-patterns", 0.5, 0.)]),
        (Cond::All(&["count", "lines"]), &[("text-processing", 1.2, 0.1)]),
        (Cond::Any(&["segmentation"]), &[("text-processing", 1.1, 0.), ("algorithms", 1.1, 0.)]),
        (Cond::Any(&["text"]), &[("text-processing", 1.1, 0.)]),
        (Cond::Any(&["word"]), &[("text-processing", 1.1, 0.)]),
        (Cond::All(&["count", "lines", "code"]), &[("text-processing", 1.2, 0.1)]),
        (Cond::All(&["pattern", "matches"]), &[("algorithms", 1.2, 0.), ("text-processing", 1.2, 0.)]),
        (Cond::All(&["pattern", "expression"]), &[("algorithms", 1.2, 0.), ("text-processing", 1.2, 0.), ("rust-patterns", 1.2, 0.)]),
        (Cond::All(&["evaluate", "expression"]), &[("algorithms", 1.2, 0.), ("rust-patterns", 1.2, 0.)]),

        (Cond::Any(&["openssl"]),
            &[("network-programming", 1.2, 0.1), ("cryptography", 1.2, 0.05), ("algorithms", 0.8, 0.), ("science::math", 0.2, 0.), ("science", 0.7, 0.), ("memory-management", 0.4, 0.),
            ("cryptography::cryptocurrencies", 0.7, 0.), ("command-line-utilities", 0.9, 0.), ("no-std", 0.9, 0.), ("compilers", 0.8, 0.), ("development-tools::testing", 0.7, 0.)]),
        (Cond::Any(&["subtle"]), &[("cryptography", 1.1, 0.01)]),
        (Cond::Any(&["tls", "ssl"]),
            &[("network-programming", 1.2, 0.1), ("cryptography", 1.2, 0.05), ("science::math", 0.2, 0.), ("science", 0.7, 0.), ("memory-management", 0.6, 0.),
            ("cryptography::cryptocurrencies", 0.6, 0.), ("no-std", 0.9, 0.), ("command-line-utilities", 0.7, 0.), ("compilers", 0.8, 0.), ("development-tools::testing", 0.9, 0.)]),
        (Cond::Any(&["packet", "firewall"]), &[("network-programming", 1.1, 0.1), ("no-std", 0.9, 0.), ("encoding", 0.9, 0.)]),
        (Cond::Any(&["hmac-sha256", "sha1", "sha-1", "blake3", "sha256", "sha2", "shamir", "cipher", "aes", "rot13", "md5", "pkcs7", "k12sum", "keccak", "scrypt", "bcrypt", "merkle", "digest", "chacha", "chacha20"]),
            &[("cryptography", 1.4, 0.3), ("algorithms", 0.8, 0.), ("no-std", 0.95, 0.), ("config", 0.8, 0.), ("rendering::engine", 0.7, 0.),
            ("command-line-utilities", 0.75, 0.), ("development-tools::testing", 0.8, 0.), ("development-tools::profiling", 0.8, 0.), ("development-tools", 0.8, 0.)]),
        (Cond::Any(&["cryptography", "cryptographic", "cryptographic-primitives", "sponge", "ecdsa", "galois", "ed25519","argon2", "pbkdf2"]),
            &[("cryptography", 1.4, 0.3), ("algorithms", 0.9, 0.), ("no-std", 0.95, 0.), ("command-line-utilities", 0.75, 0.), ("development-tools", 0.8, 0.), ("development-tools::testing", 0.8, 0.)]),
        (Cond::Any(&["lets-encrypt", "letsencrypt", "csr", "acme"]), &[("cryptography", 1.2, 0.1), ("web-programming", 1.1, 0.01)]),
        (Cond::Any(&["dep:constant_time_eq"]), &[("cryptography", 1.2, 0.05)]),
        (Cond::Any(&["dep:digest", "dep:subtle"]), &[("cryptography", 1.2, 0.), ("algorithms", 1.2, 0.)]),

        (Cond::NotAny(&["ethereum", "substrate", "eth", "xrp", "eth2", "dep:frame-support", "dep:gemachain-sdk", "dep:web3", "cryptocurrency", "vapory","arweave", "randomx", "dfinity", "mining", "tari", "pow", "proof-of-work", "binance", "tetsy", "diem", "fluence", "snarkvm", "snarkos", "zcash", "tetcore",
            "dep:ckb-types", "ethcore", "xynthe", "mimblewimble", "crypto", "contract", "safecoin", "contracts", "smart-contracts", "cosmos", "eth", "stake", "near",
            "zk-snark", "dep:near-sdk", "dep:tet-core", "dep:solana-sdk", "dep:solana-program", "fungible", "dep:ethabi", "dep:sp-core", "dep:frame-support", "dep:safecoin-sdk", "dep:ethereum-types", "dep:ethereumvm",
            "parity", "solidity", "dep:holochain_core_types", "dep:solid","kraken", "holochain", "web3", "bitcoin", "solana", "monero", "coinbase",
            "litecoin", "bitfinex", "ledger", "hyperledger", "btc", "interledger", "apdu", "dep:wedpr_l_macros", "consensus", "contract", "dep:parity-scale-codec", "self-sovereign", "dep:libp2p", "dep:bitcoin", "dep:blockchain",
            "nanocurrency", "currency", "stellar", "coin", "wallet", "bitstamp"]),
            &[("cryptography::cryptocurrencies", 0.8, 0.)]),
        (Cond::Any(&["ethereum", "rapid-blockchain-prototypes", "ethcore", "randomx", "dep:wedpr_l_macros", "dep:solana-program", "tari", "safecoin",  "chainlink", "vapory", "arweave","nft", "web3", "dfinity", "snarkvm", "tetsy", "fluence", "dep:diem-types", "binance", "dep:ckb-types", "snarkos",
            "zcash", "bitcoin", "dep:tet-core", "tetcore", "kraken", "xynthe","dep:ethereumvm", "solidity", "zk-snark", "smart-contracts", "holochain", "dep:solid", "mimblewimble",
            "dep:holochain_core_types", "gemachain-sdkreumvm", "dep:hdk", "dep:cosmwasm-std", "dep:gemachain-sdk", "dep:ethers-core", "dep:near-sdk","dep:gemachain-frozen-abi", "dep:safecoin-sdk", "dep:solana-sdk", "dep:ethabi", "dep:cxmr-currency",
            "dep:sp-core", "dep:frame-support", "monero", "hyperledger"]),
            &[("cryptography::cryptocurrencies", 1.7, 0.8), ("science::math", 0.8, 0.), ("parsing", 0.6, 0.), ("data-structures", 0.6, 0.), ("cryptography", 0.5, 0.), ("database-implementations", 0.7, 0.), ("compilers", 0.5, 0.),
            ("database", 0.3, 0.), ("email", 0.6, 0.), ("value-formatting", 0.7, 0.), ("rust-patterns", 0.7, 0.), ("algorithms", 0.9, 0.), ("encoding", 0.7, 0.), ("embedded", 0.7, 0.), ("no-std", 0.4, 0.), ("development-tools::cargo-plugins", 0.6, 0.),
            ("command-line-utilities", 0.4, 0.), ("multimedia::video", 0.5, 0.), ("multimedia", 0.6, 0.), ("multimedia::encoding", 0.5, 0.), ("development-tools::testing", 0.7, 0.), ("development-tools", 0.6, 0.), ("date-and-time", 0.4, 0.)]),
        (Cond::Any(&["coinbase", "litecoin", "gemachain", "bitfinex", "dep:finality-grandpa", "nft", "dep:parity-scale-codec", "dep:solana-vote-program", "dep:grin_util", "dep:ethereum-block", "dep:ethereum-types", "dep:oasis-types", "nanocurrency", "dep:ethereum-types", "dep:ethbloom", "self-sovereign"]),
            &[("cryptography::cryptocurrencies", 1.7, 0.8), ("science::math", 0.8, 0.), ("data-structures", 0.7, 0.), ("cryptography", 0.4, 0.), ("database-implementations", 0.7, 0.),
            ("database", 0.3, 0.), ("email", 0.6, 0.), ("value-formatting", 0.7, 0.), ("algorithms", 0.9, 0.), ("embedded", 0.8, 0.), ("no-std", 0.4, 0.), ("development-tools::cargo-plugins", 0.7, 0.),
            ("command-line-utilities", 0.8, 0.), ("development-tools::testing", 0.7, 0.), ("development-tools", 0.6, 0.), ("date-and-time", 0.4, 0.)]),
        (Cond::Any(&["cryptocurrency", "dep:blockchain", "altcoin", "dep:web3", "dep:near-primitives", "dep:cw-utils", "dep:bitcoin", "dep:holochain_core_types", "dep:grin_api",
            "dep:wedpr_l_utils", "dep:grin_core", "bitcoincash", "cryptocurrencies", "blockchain", "blockchains", "exonum", "solana", "libra"]),
            &[("cryptography::cryptocurrencies", 1.7, 0.8), ("science::math", 0.7, 0.), ("multimedia::encoding", 0.8, 0.), ("data-structures", 0.7, 0.), ("cryptography", 0.4, 0.),
            ("database-implementations", 0.6, 0.), ("database", 0.3, 0.), ("email", 0.5, 0.), ("value-formatting", 0.7, 0.), ("rust-patterns", 0.7, 0.),
            ("algorithms", 0.8, 0.), ("os::unix-apis", 0.3, 0.), ("os", 0.5, 0.), ("science", 0.5, 0.), ("embedded", 0.6, 0.), ("no-std", 0.4, 0.), ("development-tools::cargo-plugins", 0.6, 0.),
            ("command-line-utilities", 0.8, 0.), ("multimedia::video", 0.5, 0.), ("multimedia", 0.6, 0.), ("multimedia::encoding", 0.5, 0.),
            ("development-tools::testing", 0.6, 0.), ("development-tools", 0.6, 0.), ("date-and-time", 0.4, 0.)]),
        (Cond::Any(&["parity", "stellar", "coin", "wallet", "wallets", "bitstamp", "dep:parity-codec", "dep:libp2p"]),
            &[("cryptography::cryptocurrencies", 1.3, 0.1), ("cryptography", 0.4, 0.), ("command-line-utilities", 0.8, 0.), ("rust-patterns", 0.7, 0.), ("multimedia::encoding", 0.6, 0.), ("compilers", 0.5, 0.), ("science::math", 0.8, 0.)]),
        (Cond::All(&["bitcoin", "cash"]), &[("cryptography::cryptocurrencies", 1.5, 0.2), ("cryptography", 0.4, 0.)]),
        (Cond::All(&["kraken", "exchange"]), &[("cryptography::cryptocurrencies", 1.5, 0.3), ("cryptography", 0.4, 0.)]),
        (Cond::All(&["smart", "contract"]), &[("cryptography::cryptocurrencies", 1.3, 0.1), ("cryptography", 0.6, 0.), ("no-std", 0.9, 0.), ("wasm", 0.5, 0.)]),
        (Cond::All(&["proof", "work"]), &[("cryptography::cryptocurrencies", 1.3, 0.1), ("no-std", 0.9, 0.), ("wasm", 0.5, 0.)]),
        (Cond::All(&["smart", "contracts"]), &[("cryptography::cryptocurrencies", 1.3, 0.1), ("no-std", 0.9, 0.), ("wasm", 0.25, 0.)]),
        (Cond::Any(&["smart-contracts"]), &[("cryptography::cryptocurrencies", 1.3, 0.1), ("no-std", 0.5, 0.), ("wasm", 0.2, 0.)]),
        (Cond::All(&["fungible", "tokens"]), &[("cryptography::cryptocurrencies", 1.3, 0.1), ("cryptography", 0.4, 0.)]),
        (Cond::All(&["crypto", "asset"]), &[("cryptography::cryptocurrencies", 1.2, 0.05)]),
        (Cond::All(&["cosmwasm"]), &[("cryptography::cryptocurrencies", 1.2, 0.2), ("wasm", 0.4, 0.), ("cryptography", 0.4, 0.)]),
        (Cond::All(&["ledger", "public"]), &[("cryptography::cryptocurrencies", 1.2, 0.2)]),
        (Cond::Any(&["polkadot", "dep:borsh", "proof-of-work", "eth", "substrate", "dep:frame-support"]), &[("cryptography::cryptocurrencies", 1.2, 0.05), ("cryptography", 0.8, 0.)]),
        (Cond::Any(&["uint"]), &[("rust-patterns", 1.2, 0.), ("cryptography::cryptocurrencies", 0.8, 0.)]),
        (Cond::All(&["integer", "types"]), &[("rust-patterns", 1.1, 0.), ("cryptography::cryptocurrencies", 0.6, 0.)]),
        (Cond::All(&["primitive", "integer"]), &[("rust-patterns", 1.1, 0.), ("cryptography::cryptocurrencies", 0.6, 0.)]),
        (Cond::All(&["unsigned", "integers"]), &[("data-structures", 1.1, 0.), ("cryptography::cryptocurrencies", 0.9, 0.)]),
        (Cond::Any(&["fixed-size", "fixed-size-integers", "stack-based"]), &[("data-structures", 1.1, 0.), ("algorithms", 1.1, 0.), ("rust-patterns", 1.1, 0.), ("cryptography::cryptocurrencies", 0.5, 0.)]),
        (Cond::Any(&["fixed-size-integers"]), &[("data-structures", 1.2, 0.), ("cryptography::cryptocurrencies", 0.5, 0.)]),

        // diesel's troll:
        (Cond::All(&["blockchain", "sql"]), &[("cryptography::cryptocurrencies", 0.3, 0.), ("database", 1.2, 0.1)]),
        (Cond::All(&["blockchain", "orm"]), &[("cryptography::cryptocurrencies", 0.3, 0.), ("database", 1.2, 0.1)]),
        (Cond::All(&["blockchain", "postgresql"]), &[("cryptography::cryptocurrencies", 0.3, 0.), ("database", 1.2, 0.1)]),
        (Cond::All(&["blockchain", "mysql"]), &[("cryptography::cryptocurrencies", 0.3, 0.), ("database", 1.2, 0.1)]),
        (Cond::Any(&["exonum"]), &[("database", 0.5, 0.), ("database-implementations", 0.5, 0.)]),
        (Cond::Any(&["bitcoin", "nervos"]), &[("database", 0.5, 0.), ("database-implementations", 0.5, 0.)]),
        (Cond::Any(&["network"]), &[("database", 0.8, 0.), ("database-implementations", 0.8, 0.), ("rendering::data-formats", 0.8, 0.)]),

        (Cond::NotAny(&["tokio", "future", "futures", "dep:tokio", "dep:futures-core", "promise", "promises", "executor", "reactor", "pin", "eventloop", "event-loop", "event", "callback",
            "callbacks", "non-blocking", "async", "async-await", "message", "queue", "message-queue", "stream", "await", "asynchronous", "aio", "mio", "timer", "dep:mio", "dep:async-task"]),
            &[("asynchronous", 0.8, 0.)]),
        (Cond::Any(&["tokio", "future", "futures", "async-await", "promise", "stream", "non-blocking", "async"]),
            &[("asynchronous", 1.1, 0.1), ("concurrency", 0.9, 0.), ("os", 0.9, 0.), ("command-line-utilities", 0.75, 0.), ("cryptography::cryptocurrencies", 0.6, 0.),
            ("caching", 0.9, 0.), ("value-formatting", 0.5, 0.), ("text-processing", 0.6, 0.), ("config", 0.8, 0.), ("memory-management", 0.9, 0.), ("games", 0.15, 0.)]),
        (Cond::Any(&["async-std", "await", "runtime", "asynchronous", "dep:async-task", "dep:mio", "pin", "dep:pin-utils"]),
            &[("asynchronous", 1.2, 0.1), ("value-formatting", 0.8, 0.0)]),
        (Cond::Any(&["dep:reqwest", "dep:curl", "dep:hyper"]),
            &[("date-and-time", 0.7, 0.), ("parsing", 0.7, 0.), ("rust-patterns", 0.7, 0.), ("embedded", 0.7, 0.), ("algorithms", 0.8, 0.),
            ("data-structures", 0.7, 0.), ("rendering", 0.7, 0.), ("value-formatting", 0.8, 0.0)]),
        (Cond::Any(&["dep:tokio", "dep:futures-core", "dep:actix", "dep:mio", "dep:async-std"]),
            &[("value-formatting", 0.7, 0.), ("parsing", 0.7, 0.), ("algorithms", 0.8, 0.), ("data-structures", 0.7, 0.), ("no-std", 0.7, 0.)]),

        (Cond::NotAny(&["settings", "configuration", "config", "dotenv", "configurator", "dotfile", "dotfiles", "env", "customization", "environment"]),
            &[("config", 0.75, 0.)]),
        (Cond::Any(&["settings", "configuration", "configurator", "config"]),
            &[("config", 1.15, 0.2), ("development-tools::debugging", 0.8, 0.), ("os::macos-apis", 0.95, 0.), ("algorithms", 0.9, 0.),
            ("command-line-utilities", 0.9, 0.), ("internationalization", 0.9, 0.), ("command-line-interface", 0.9, 0.)]),
        (Cond::Any(&["configure", "dotenv", "dotfile", "dotfiles", "environment"]),
            &[("config", 1.2, 0.1), ("command-line-interface", 0.9, 0.), ("multimedia::video", 0.8, 0.)]),
        (Cond::All(&["configuration", "management"]),
            &[("config", 1.2, 0.1)]),
        (Cond::Any(&["log", "logger", "logging", "logs"]),
            &[("development-tools::debugging", 1.2, 0.1), ("rust-patterns", 0.9, 0.), ("wasm", 0.7, 0.), ("no-std", 0.7, 0.),
            ("concurrency", 0.8, 0.), ("algorithms", 0.6, 0.), ("asynchronous", 0.9, 0.), ("multimedia::video", 0.8, 0.),
            ("config", 0.9, 0.), ("emulators", 0.8, 0.), ("encoding", 0.8, 0.), ("games", 0.01, 0.), ("development-tools::profiling", 0.9, 0.),
            ("command-line-interface", 0.9, 0.), ("command-line-utilities", 0.8, 0.)]),
        (Cond::Any(&["tracing"]), &[("development-tools::debugging", 1.1, 0.1)]),
        (Cond::Any(&["dep:tracing-core"]), &[("development-tools::debugging", 1.1, 0.)]),
        (Cond::Any(&["dlsym", "debug", "debugging", "debugger", "disassemlber", "demangle", "dwarf", "stacktrace", "sentry"]),
            &[("development-tools::debugging", 1.2, 0.1), ("concurrency", 0.9, 0.), ("no-std", 0.9, 0.), ("algorithms", 0.7, 0.), ("wasm", 0.9, 0.), ("emulators", 0.9, 0.), ("games", 0.01, 0.), ("development-tools::profiling", 0.7, 0.), ("command-line-utilities", 0.8, 0.)]),
        (Cond::Any(&["backtrace", "disassembly", "disassembler", "symbolic", "symbolicate", "coredump", "valgrind", "lldb"]),
            &[("development-tools::debugging", 1.2, 0.1), ("concurrency", 0.9, 0.), ("data-structures", 0.9, 0.), ("algorithms", 0.7, 0.), ("wasm", 0.9, 0.), ("emulators", 0.9, 0.),
            ("games", 0.01, 0.), ("multimedia::encoding", 0.8, 0.), ("development-tools::profiling", 0.7, 0.), ("command-line-utilities", 0.8, 0.)]),
        (Cond::Any(&["elf", "archive"]), &[("development-tools::debugging", 0.8, 0.), ("games", 0.4, 0.)]),
        (Cond::Any(&["monitor", "monitoring"]), &[("development-tools::debugging", 1.1, 0.), ("development-tools::profiling", 1.1, 0.), ("development-tools", 1.1, 0.)]),
        (Cond::Any(&["elf"]), &[("encoding", 1.1, 0.), ("os::unix-apis", 1.1, 0.)]),
        (Cond::Any(&["travis", "jenkins", "ci", "testing", "quickcheck", "test-driven", "tdd", "unittest"]),
            &[("development-tools::testing", 1.2, 0.2), ("development-tools::cargo-plugins", 0.9, 0.), ("rust-patterns", 0.9, 0.),
            ("development-tools", 0.8, 0.), ("os::macos-apis", 0.8, 0.), ("development-tools::build-utils", 0.8, 0.),
            ("games", 0.4, 0.), ("rendering::data-formats", 0.5, 0.), ("text-processing", 0.5, 0.)]),
        (Cond::Any(&["unittests", "junit", "unit-testing", "pentest", "code-coverage", "testbed", "mock", "mocks"]),
            &[("development-tools::testing", 1.2, 0.2), ("development-tools::cargo-plugins", 0.9, 0.), ("rust-patterns", 0.9, 0.),
            ("development-tools", 0.8, 0.), ("os::macos-apis", 0.8, 0.), ("development-tools::build-utils", 0.8, 0.),
            ("games", 0.4, 0.), ("rendering::data-formats", 0.5, 0.), ("text-processing", 0.5, 0.)]),
        (Cond::All(&["gui", "automation"]), &[("development-tools::testing", 1.3, 0.3), ("gui", 0.25, 0.), ("no-std", 0.9, 0.), ("algorithms", 0.8, 0.), ("os::macos-apis", 0.8, 0.)]),
        (Cond::All(&["continuous", "integration"]), &[("development-tools::testing", 1.3, 0.3)]),
        (Cond::All(&["test", "framework"]), &[("development-tools::testing", 1.3, 0.2)]),
        (Cond::Any(&["fuzzing"]), &[("development-tools::testing", 1.3, 0.2)]),
        (Cond::Any(&["automation"]), &[("compression", 0.75, 0.)]),
        (Cond::Any(&["tests", "unittesting", "american-fuzzy-lop", "afl"]), &[("development-tools::testing", 1.2, 0.2), ("development-tools", 0.9, 0.), ("development-tools::cargo-plugins", 0.9, 0.)]),
        (Cond::All(&["integration", "test"]), &[("development-tools::testing", 1.2, 0.1), ("rust-patterns", 0.9, 0.), ("date-and-time", 0.6, 0.)]),
        (Cond::All(&["integration", "tests"]), &[("development-tools::testing", 1.2, 0.1), ("date-and-time", 0.6, 0.)]),
        (Cond::All(&["unit", "tests"]), &[("development-tools::testing", 1.2, 0.1), ("date-and-time", 0.6, 0.)]),
        (Cond::Any(&["diff", "writer", "table", "gcd", "sh", "unwrap", "build", "relative", "path", "fail"]),
            &[("development-tools::testing", 0.5, 0.), ("internationalization", 0.7, 0.), ("gui", 0.7, 0.)]),
        (Cond::Any(&["string", "strings"]), &[("command-line-utilities", 0.5, 0.), ("multimedia::images", 0.5, 0.)]),
        (Cond::Any(&["rope"]), &[("command-line-utilities", 0.5, 0.), ("multimedia::images", 0.5, 0.)]),
        (Cond::Any(&["binary", "streaming", "version", "buffer", "recursive", "escape"]),
            &[("development-tools::testing", 0.75, 0.), ("internationalization", 0.75, 0.), ("development-tools::build-utils", 0.8, 0.)]),
        (Cond::Any(&["escape", "text-processing"]), &[("text-processing", 1.2, 0.05), ("compression", 0.8, 0.)]),
        (Cond::Any(&["string", "unescape", "opengl", "opengl-es", "memchr", "ios",  "cuda"]),
            &[("development-tools::testing", 0.75, 0.), ("internationalization", 0.8, 0.), ("development-tools::build-utils", 0.8, 0.)]),
        (Cond::Any(&["dex", "android"]), &[("development-tools", 1.2, 0.)]),
        (Cond::All(&["build", "system"]), &[("development-tools", 1.1, 0.1), ("development-tools::build-utils", 1.15, 0.1)]),
        (Cond::Any(&["streams", "streaming"]), &[("algorithms", 1.1, 0.03), ("network-programming", 1.1, 0.), ("development-tools::cargo-plugins", 0.7, 0.)]),
        (Cond::Any(&["boolean", "search"]), &[("development-tools::testing", 0.9, 0.), ("multimedia::images", 0.8, 0.), ("multimedia::audio", 0.8, 0.),
            ("internationalization", 0.8, 0.), ("rendering::data-formats", 0.8, 0.)]),
        (Cond::Any(&["fuzzy"]), &[("algorithms", 1.1, 0.), ("multimedia::audio", 0.8, 0.), ("internationalization", 0.8, 0.), ("rendering::data-formats", 0.8, 0.)]),
        (Cond::Any(&["text"]), &[("development-tools::testing", 0.9, 0.), ("caching", 0.8, 0.), ("multimedia::images", 0.8, 0.), ("multimedia::audio", 0.8, 0.),
            ("rendering::data-formats", 0.9, 0.)]),

        (Cond::Any(&["ai", "piston", "logic", "2d", "graphic"]), &[("web-programming::http-client", 0.5, 0.), ("web-programming::websocket", 0.5, 0.)]),

        (Cond::Any(&["dep:cookie"]), &[("web-programming", 1.25, 0.1), ("web-programming::http-client", 1.25, 0.1), ("web-programming::http-server", 1.25, 0.1)]),
        (Cond::Any(&["activitypub", "activitystreams", "pubsub"]), &[("web-programming", 1.25, 0.2), ("network-programming", 1.25, 0.2), ("rust-patterns", 0.9, 0.), ("algorithms", 0.8, 0.), ("web-programming::websocket", 1.1, 0.), ("command-line-utilities", 0.75, 0.)]),
        (Cond::Any(&["websocket", "websockets"]), &[("web-programming::websocket", 1.85, 0.4), ("no-std", 0.9, 0.), ("command-line-utilities", 0.5, 0.)]),
        (Cond::NotAny(&["sse", "tungstenite", "websocket", "websockets", "ws", "pubsub", "broadcast", "server-sent-events", "rfc6455"]), &[("web-programming::websocket", 0.5, 0.)]),
        (Cond::Any(&["servo"]), &[("web-programming::websocket", 0.5, 0.), ("no-std", 0.3, 0.), ("command-line-interface", 0.5, 0.)]),

        (Cond::Any(&["generic"]), &[("development-tools::debugging", 0.5, 0.), ("web-programming::websocket", 0.5, 0.)]),
        (Cond::Any(&["quaternion"]), &[("science::math", 1.1, 0.1), ("game-development", 1.1, 0.), ("parsing", 0.25, 0.), ("algorithms", 0.9, 0.), ("command-line-utilities", 0.75, 0.)]),
        (Cond::Any(&["bitmap", "raster"]), &[("rendering::data-formats", 1.1, 0.0), ("multimedia::images", 1.1, 0.0), ("internationalization", 0.5, 0.)]),
        (Cond::Any(&["plural", "pluralize", "iso3166-1", "iso3166", "iso-3166-1", "bcp47", "translate"]), &[("internationalization", 1.2, 0.1)]),
        (Cond::Any(&["internationalisation", "i18n", "iso-639", "internationalization"]),
            &[("internationalization", 1.5, 0.3), ("value-formatting", 0.9, 0.), ("parsing", 0.8, 0.), ("os", 0.9, 0.), ("algorithms", 0.8, 0.),
            ("network-programming", 0.9, 0.), ("web-programming", 0.8, 0.), ("web-programming::http-server", 0.7, 0.)]),
        (Cond::Any(&["gettext"]), &[("internationalization", 1.3, 0.2)]),
        (Cond::Any(&["math"]), &[("rendering", 0.75, 0.), ("command-line-utilities", 0.75, 0.)]),
        (Cond::Any(&["rendering", "sql", "sqlx"]), &[("rendering::data-formats", 0.2, 0.), ("caching", 0.8, 0.), ("value-formatting", 0.8, 0.), ("hardware-support", 0.7, 0.)]),

        (Cond::Any(&["speech-recognition"]), &[("science", 1.3, 0.1),("multimedia::audio", 1.3, 0.1)]),
        (Cond::Any(&["tts", "speech"]), &[("multimedia::audio", 1.1, 0.), ("internationalization", 0.6, 0.)]),
        (Cond::Any(&["downsample", "dsp", "samplerate"]), &[("multimedia::audio", 1.2, 0.1), ("filesystem", 0.7, 0.)]),
        (Cond::Any(&["music", "chiptune", "synth", "chords", "audio", "sound", "sounds", "speech", "microphone"]),
            &[("multimedia::audio", 1.3, 0.3), ("command-line-utilities", 0.8, 0.), ("multimedia::images", 0.6, 0.),
            ("rendering::graphics-api", 0.6, 0.), ("rendering", 0.6, 0.), ("cryptography::cryptocurrencies", 0.6, 0.), ("command-line-interface", 0.5, 0.),
            ("caching", 0.8, 0.), ("no-std", 0.8, 0.)]),
        (Cond::Any(&["flac", "spotify", "vst", "vorbis", "midi", "pulseaudio", "mp3", "aac", "wav"]),
            &[("multimedia::audio", 1.3, 0.3), ("command-line-utilities", 0.8, 0.), ("multimedia::images", 0.6, 0.), ("rendering::graphics-api", 0.75, 0.), ("cryptography::cryptocurrencies", 0.6, 0.), ("command-line-interface", 0.5, 0.), ("caching", 0.8, 0.)]),
        (Cond::Any(&["nyquist"]), &[("multimedia::audio", 1.1, 0.1), ("game-development", 0.8, 0.)]),
        (Cond::Any(&["dep:cpal", "dep:coreaudio-rs"]), &[("multimedia::audio", 1.1, 0.1)]),
        (Cond::All(&["gain", "level"]), &[("multimedia::audio", 1.2, 0.1)]),
        (Cond::All(&["gain", "microphone"]), &[("multimedia::audio", 1.2, 0.1)]),
        (Cond::All(&["mod", "tracker"]), &[("multimedia::audio", 1.1, 0.)]),
        (Cond::Any(&["perspective", "graphics", "cam"]), &[("multimedia::audio", 0.4, 0.)]),
        (Cond::Any(&["ffi", "sys", "daemon"]), &[("multimedia::audio", 0.9, 0.)]),
        (Cond::Any(&["sigabrt", "sigint"]), &[("multimedia::audio", 0.1, 0.), ("algorithms", 0.8, 0.), ("multimedia", 0.1, 0.)]),
        (Cond::Any(&["sigterm", "sigquit"]), &[("multimedia::audio", 0.1, 0.), ("multimedia", 0.1, 0.)]),

        (Cond::Any(&["multimedia", "chromecast", "media", "dvd", "mpeg"]), &[
            ("multimedia", 1.3, 0.3), ("algorithms", 0.8, 0.), ("rust-patterns", 0.8, 0.), ("data-structures", 0.9, 0.), ("encoding", 0.5, 0.)]),
        (Cond::Any(&["dep:gstreamer"]), &[("multimedia", 1.2, 0.1)]),
        (Cond::Any(&["dep:allegro"]), &[("multimedia", 1.1, 0.)]),
        (Cond::Any(&["image", "images", "viewer", "photos"]), &[
            ("multimedia::images", 1.2, 0.1), ("parser-implementations", 0.9, 0.), ("parsing", 0.6, 0.), ("data-structures", 0.8, 0.)]),
        (Cond::Any(&["icons", "icns", "ico", "favicon"]), &[("multimedia::images", 1.2, 0.1), ("no-std", 0.5, 0.)]),
        (Cond::All(&["kernel", "image"]), &[("multimedia::images", 0.5, 0.)]),
        (Cond::Any(&["dep:dssim", "dep:imgref"]), &[("multimedia::images", 1.2, 0.05), ("no-std", 0.2, 0.)]),
        (Cond::Any(&["dep:mozjpeg", "dep:lodepng", "dep:image"]), &[("multimedia::images", 1.2, 0.05), ("no-std", 0.5, 0.)]),
        (Cond::Any(&["dep:rgb", "dep:gif", "dep:png"]), &[("multimedia::images", 1.2, 0.05), ("no-std", 0.5, 0.)]),
        (Cond::All(&["binary", "image"]), &[("multimedia::images", 0.8, 0.)]),
        (Cond::All(&["bootable", "image"]), &[("multimedia::images", 0.1, 0.)]),
        (Cond::All(&["image", "generation"]), &[("multimedia::images", 1.1, 0.), ("no-std", 0.7, 0.)]),
        (Cond::All(&["image", "processing"]), &[("multimedia::images", 1.2, 0.), ("no-std", 0.7, 0.)]),
        (Cond::All(&["qr", "code"]), &[("multimedia::images", 1.15, 0.)]),
        (Cond::Any(&["qr-code", "qrcode"]), &[("multimedia::images", 1.2, 0.05)]),
        (Cond::Any(&["dicom"]), &[("multimedia::images", 1.2, 0.1), ("parser-implementations", 1.1, 0.05), ("science", 1.05, 0.)]),
        (Cond::Any(&["flif", "png", "jpeg2000", "j2k", "jpeg", "heif", "heic", "avif", "avic", "exif", "ocr", "svg", "pixel"]), &[
            ("multimedia::images", 1.3, 0.15), ("encoding", 0.8, 0.), ("parsing", 0.8, 0.), ("rust-patterns", 0.6, 0.), ("data-structures", 0.9, 0.)]),
        (Cond::Any(&["imagemagick", "gamma", "photo", "openexr"]), &[
            ("multimedia::images", 1.3, 0.15), ("encoding", 0.5, 0.), ("parsing", 0.6, 0.), ("data-structures", 0.8, 0.)]),
        (Cond::Any(&["color", "colors", "colour", "colours", "opencv", "colorspace", "hsl"]), &[("multimedia::images", 1.2, 0.1), ("multimedia", 1.1, 0.)]),
        (Cond::Any(&["quantization"]), &[("multimedia::images", 1.2, 0.1), ("multimedia", 1.1, 0.), ("command-line-interface", 0.2, 0.)]),
        (Cond::Any(&["webm", "av1", "dvd", "codec", "vpx"]), &[("multimedia::encoding", 1.5, 0.2), ("multimedia::video", 1.4, 0.3),
            ("encoding", 0.15, 0.), ("parsing", 0.8, 0.), ("data-structures", 0.7, 0.)]),
        (Cond::Any(&["h265", "h264", "ffmpeg", "h263", "movie"]), &[
            ("multimedia::video", 1.5, 0.3), ("multimedia::encoding", 1.3, 0.1), ("encoding", 0.15, 0.), ("data-structures", 0.8, 0.)]),
        (Cond::Any(&["x265", "x264", "mp4", "h263", "vp9", "libvpx", "video", "movies"]), &[
            ("multimedia::video", 1.5, 0.3), ("multimedia::encoding", 1.3, 0.1), ("encoding", 0.15, 0.), ("parsing", 0.15, 0.), ("data-structures", 0.8, 0.)]),
        (Cond::Any(&["webcam", "videocamera"]), &[("multimedia::video", 1.5, 0.3), ("multimedia", 1.1, 0.), ("parsing", 0.1, 0.), ("no-std", 0.9, 0.), ("multimedia::encoding", 1.1, 0.)]),
        (Cond::Any(&["opengl", "opengl-es", "esolang", "interpreter", "ascii", "mesh", "vulkan", "line"]), &[("multimedia::video", 0.5, 0.)]),
        (Cond::Any(&["reader"]), &[("multimedia::video", 0.85, 0.), ("parser-implementations", 1.1, 0.), ("data-structures", 0.8, 0.)]),
        (Cond::Any(&["timer"]), &[("multimedia::video", 0.8, 0.), ("multimedia", 0.8, 0.)]),
        (Cond::All(&["timer", "rpm"]), &[("date-and-time", 1.1, 0.), ("os", 0.7, 0.), ("os::unix-apis", 0.7, 0.)]),
        (Cond::All(&["rate", "rpm"]), &[("date-and-time", 1.1, 0.), ("os", 0.7, 0.), ("os::unix-apis", 0.7, 0.)]),
        (Cond::Any(&["sound"]), &[("multimedia::video", 0.9, 0.)]),

        (Cond::Any(&["plotting", "codeviz", "viz"]),
            &[("visualization", 1.3, 0.3), ("science::math", 0.5, 0.), ("science", 0.85, 0.), ("command-line-interface", 0.75, 0.), ("command-line-utilities", 0.6, 0.), ("games", 0.01, 0.), ("parsing", 0.6, 0.), ("caching", 0.5, 0.)]),
        (Cond::Any(&["visualizer", "renderer"]),
            &[("visualization", 1.3, 0.3), ("parsing", 0.8, 0.), ("caching", 0.5, 0.)]),
        (Cond::Any(&["dot", "graph"]), &[("visualization", 1.3, 0.)]),
        (Cond::Any(&["gnuplot", "chart", "plot"]),
            &[("visualization", 1.3, 0.3), ("science::math", 0.75, 0.), ("science", 0.8, 0.), ("command-line-interface", 0.5, 0.), ("command-line-utilities", 0.75, 0.), ("caching", 0.5, 0.)]),
        (Cond::Any(&["aws", "s3", "cpython", "interpreter", "pdf", "derive"]), &[("visualization", 0.8, 0.), ("filesystem", 0.8, 0.)]),

        (Cond::Any(&["security", "disassemlber"]), &[("emulators", 0.6, 0.), ("multimedia::encoding", 0.8, 0.), ("os::macos-apis", 0.5, 0.)]),
        (Cond::Any(&["compilers"]), &[("development-tools", 1.3, 0.2), ("emulators", 1.2, 0.)]),
        (Cond::Any(&["zx", "gameboy", "super-nintendo", "emulator", "emulation"]),
            &[("emulators", 1.25, 0.15), ("games", 0.7, 0.), ("parsing", 0.3, 0.), ("no-std", 0.7, 0.), ("email", 0.8, 0.), ("concurrency", 0.7, 0.), ("text-processing", 0.5, 0.),
            ("parser-implementations", 0.9, 0.), ("data-structures", 0.8, 0.), ("algorithms", 0.9, 0.),
            ("multimedia::images", 0.5, 0.), ("multimedia::audio", 0.6, 0.), ("no-std", 0.8, 0.), ("gui", 0.8, 0.), ("command-line-interface", 0.5, 0.), ("multimedia::video", 0.5, 0.), ("command-line-utilities", 0.75, 0.)]),
        (Cond::Any(&["qemu", "vm", "codegen", "cranelift"]), &[("emulators", 1.4, 0.1), ("parser-implementations", 0.9, 0.), ("parsing", 0.5, 0.), ("development-tools", 1.1, 0.),
            ("multimedia::video", 0.5, 0.), ("multimedia::encoding", 0.5, 0.), ("wasm", 0.8, 0.)]),
        (Cond::Any(&["z80", "mos6502", "6502", "intel-8080"]), &[("emulators", 1.3, 0.1), ("hardware-support", 1.3, 0.1), ("embedded", 1.1, 0.), ("wasm", 0.5, 0.), ("multimedia::encoding", 0.7, 0.)]),
        (Cond::Any(&["rom", "sega"]), &[("emulators", 1.1, 0.), ("hardware-support", 1.1, 0.)]),
        (Cond::Any(&["c64", "ms-dos", "chip-8", "spc700", "snes", "gameboy", "game-boy", "gba", "nintendo", "playstation", "commodore", "nes", "atari"]),
            &[("emulators", 1.3, 0.1), ("game-development", 1.1, 0.), ("multimedia::encoding", 0.7, 0.), ("development-tools::build-utils", 0.7, 0.), ("rendering::graphics-api", 0.7, 0.), ("wasm", 0.5, 0.), ("no-std", 0.8, 0.)]),
        (Cond::All(&["virtual", "machine"]), &[("emulators", 1.4, 0.1), ("simulation", 1.1, 0.), ("development-tools::build-utils", 0.7, 0.)]),
        (Cond::All(&["virtual", "machines"]), &[("emulators", 1.1, 0.), ("simulation", 1.1, 0.), ("parser-implementations", 0.8, 0.)]),
        (Cond::All(&["game", "gb"]), &[("emulators", 1.2, 0.05), ("wasm", 0.8, 0.)]),
        (Cond::All(&["game", "rom"]), &[("emulators", 1.2, 0.05)]),
        (Cond::All(&["commodore", "64"]), &[("emulators", 1.3, 0.1), ("wasm", 0.8, 0.), ("no-std", 0.9, 0.)]),
        (Cond::All(&["emulator", "gb"]), &[("emulators", 1.2, 0.1)]),
        (Cond::All(&["accurate", "cpu"]), &[("emulators", 1.1, 0.)]),
        (Cond::All(&["super", "nintendo"]), &[("emulators", 1.3, 0.1), ("wasm", 0.8, 0.)]),
        (Cond::Any(&["tick-accurate"]), &[("emulators", 1.1, 0.), ("parsing", 0.5, 0.), ("parser-implementations", 0.5, 0.)]),
        (Cond::Any(&["esolang", "interpreter", "jit", "brainfuck"]), &[("emulators", 0.7, 0.), ("compilers", 1.2, 0.2), ("parsing", 0.5, 0.), ("parser-implementations", 0.8, 0.)]),
        (Cond::Any(&["vte", "virtual-terminal", "terminal-emulator"]), &[("emulators", 0.5, 0.), ("command-line-interface", 1.1, 0.)]),
        (Cond::All(&["terminal", "emulator"]), &[("emulators", 0.8, 0.), ("command-line-interface", 1.2, 0.1)]),

        (Cond::Any(&["radix", "genetic"]), &[("science", 1.4, 0.), ("command-line-utilities", 0.75, 0.)]),

        (Cond::Any(&["protocol-specification"]), &[("gui", 0.5, 0.), ("algorithms", 0.8, 0.), ("command-line-utilities", 0.75, 0.)]),
        (Cond::Any(&["dsl", "embedded", "rtos"]), &[("gui", 0.75, 0.), ("command-line-utilities", 0.75, 0.)]),
        (Cond::Any(&["idl", "asmjs", "webasm"]), &[("gui", 0.5, 0.), ("command-line-utilities", 0.75, 0.)]),
        (Cond::Any(&["javascript", "typescript"]), &[("gui", 0.9, 0.), ("caching", 0.9, 0.), ("command-line-utilities", 0.8, 0.), ("multimedia::encoding", 0.7, 0.), ("visualization", 0.8, 0.), ("cryptography::cryptocurrencies", 0.6, 0.)]),

        (Cond::Any(&["concurrency", "spinlock", "semaphore", "parallel", "multithreaded", "barrier", "thread-local", "parallelism"]),
            &[("concurrency", 1.35, 0.1), ("command-line-utilities", 0.8, 0.), ("games", 0.5, 0.), ("memory-management", 0.8, 0.), ("caching", 0.8, 0.), ("os", 0.8, 0.), ("parsing", 0.9, 0.), ("simulation", 0.8, 0.)]),
        (Cond::Any(&["parallelizm", "coroutines", "threads", "threadpool", "fork-join", "parallelization", "actor", "openmp"]),
            &[("concurrency", 1.35, 0.1), ("command-line-utilities", 0.8, 0.), ("games", 0.5, 0.), ("memory-management", 0.8, 0.), ("caching", 0.8, 0.), ("os", 0.8, 0.), ("parsing", 0.9, 0.), ("simulation", 0.8, 0.)]),
        (Cond::Any(&["atomic"]), &[("concurrency", 1.15, 0.15), ("data-structures", 0.9, 0.), ("command-line-utilities", 0.75, 0.)]),
        (Cond::Any(&["thread-safe"]), &[("concurrency", 1.15, 0.15), ("data-structures", 1.1, 0.), ("asynchronous", 1.1, 0.), ("algorithms", 0.75, 0.)]),
        (Cond::Any(&["queue", "zookeeper"]), &[("concurrency", 1.2, 0.)]),
        (Cond::Any(&["cuda"]), &[("concurrency", 1.2, 0.)]),
        (Cond::All(&["cuda", "compute"]), &[("concurrency", 1.2, 0.1), ("no-std", 0.9, 0.)]),
        (Cond::All(&["opencl", "compute"]), &[("concurrency", 1.2, 0.1)]),
        (Cond::All(&["parallel", "compute"]), &[("concurrency", 1.2, 0.1)]),
        (Cond::All(&["parallel", "computing"]), &[("concurrency", 1.2, 0.1)]),

        (Cond::Any(&["futures", "actor"]), &[("concurrency", 1.25, 0.1), ("parsing", 0.5, 0.), ("asynchronous", 1.35, 0.3)]),
        (Cond::Any(&["events", "event"]), &[("asynchronous", 1.2, 0.), ("concurrency", 0.9, 0.), ("command-line-utilities", 0.8, 0.)]),
        (Cond::All(&["loop", "event"]), &[("game-development", 1.2, 0.1), ("asynchronous", 0.8, 0.), ("games", 0.4, 0.)]),
        (Cond::All(&["tile"]), &[("game-development", 1.1, 0.), ("network-programming", 0.8, 0.), ("asynchronous", 0.8, 0.), ("internationalization", 0.8, 0.)]),
        (Cond::All(&["map", "grid"]), &[("game-development", 1.1, 0.)]),
        (Cond::All(&["game", "map"]), &[("game-development", 1.1, 0.1), ("games", 1.1, 0.1), ("data-structures", 0.8, 0.)]),
        (Cond::All(&["game", "graphics"]), &[("game-development", 1.1, 0.1), ("games", 1.1, 0.1), ("rendering::graphics-api", 1.2, 0.1)]),
        (Cond::Any(&["consensus", "erlang", "gossip"]), &[("concurrency", 1.2, 0.1), ("network-programming", 1.2, 0.1), ("asynchronous", 1.2, 0.1), ("gui", 0.8, 0.)]),

        (Cond::Any(&["gui"]), &[("gui", 1.35, 0.1), ("command-line-interface", 0.15, 0.), ("algorithms", 0.8, 0.), ("multimedia::video", 0.5, 0.)]),
        (Cond::Any(&["qt", "x11", "wayland", "gtk", "window-events"]), &[("gui", 1.35, 0.1), ("rendering::graphics-api", 1.1, 0.), ("algorithms", 0.8, 0.), ("no-std", 0.7, 0.), ("os::unix-apis", 1.2, 0.1), ("cryptography::cryptocurrencies", 0.9, 0.), ("os::macos-apis", 0.25, 0.), ("caching", 0.5, 0.), ("command-line-interface", 0.15, 0.)]),
        (Cond::Any(&["sixtyfps", "dep:sixtyfps-corelib", "dep:sixtyfps", "dep:sixtyfps-build", "gui-application"]), &[("gui", 1.2, 0.2)]),
        (Cond::All(&["window", "manager"]), &[("gui", 1.4, 0.2)]),
        (Cond::All(&["gui", "toolkit"]), &[("gui", 1.4, 0.2)]),
        (Cond::All(&["status", "bar"]), &[("gui", 1.2, 0.1)]),
        (Cond::All(&["copy", "pasting", "user-interface"]), &[("gui", 1.1, 0.), ("parser-implementations", 0.9, 0.)]),
        (Cond::All(&["ui", "interface"]), &[("gui", 1.1, 0.05)]),
        (Cond::All(&["ui", "framework"]), &[("gui", 1.2, 0.), ("rendering::graphics-api", 1.2, 0.)]),
        (Cond::All(&["ui", "layout"]), &[("gui", 1.1, 0.1)]),
        (Cond::Any(&["window", "ui", "tui", "dashboard", "notification"]),
            &[("gui", 1.2, 0.1), ("command-line-utilities", 0.9, 0.), ("hardware-support", 0.9, 0.), ("asynchronous", 0.8, 0.), ("internationalization", 0.9, 0.)]),
        (Cond::Any(&["displaying", "desktop", "compositor"]),
            &[("gui", 1.2, 0.1), ("command-line-utilities", 0.9, 0.), ("hardware-support", 0.9, 0.), ("asynchronous", 0.8, 0.), ("internationalization", 0.9, 0.)]),
        (Cond::Any(&["dashboard", "displaying", "inspector", "instrumentation"]), &[("visualization", 1.2, 0.1), ("games", 0.5, 0.)]),
        (Cond::All(&["user", "interface"]), &[("gui", 1.1, 0.)]),
        (Cond::Any(&["toolkit", "imgui"]), &[("gui", 1.1, 0.)]),
        (Cond::Any(&["dep:miniquad", "dep:imgui", "dep:egui"]), &[("gui", 1.1, 0.), ("games", 1.1, 0.)]),
        (Cond::Any(&["dep:winit", "dep:relm4","dep:wry", "dep:druid", "dep:azul", "dep:conrod_winit", "dep:tauri", "dep:gtk4", "dep:gtk", "dep:iced"]), &[("gui", 1.2, 0.1)]),

        (Cond::Any(&["scotland", "scottish", "japan", "thai", "american", "uk", "country", "language-code"]),
            &[("internationalization", 1.2, 0.2), ("os::macos-apis", 0.8, 0.), ("command-line-utilities", 0.75, 0.), ("rendering::engine", 0.1, 0.), ("rendering::data-formats", 0.2, 0.), ("filesystem", 0.2, 0.)]),
        (Cond::Any(&["japanese", "arabic", "korean", "hangul", "pinyin", "hanzi", "locale", "chinese", "chinese-numbers"]),
            &[("internationalization", 1.2, 0.2), ("os::macos-apis", 0.7, 0.), ("command-line-utilities", 0.75, 0.), ("rendering::engine", 0.1, 0.), ("rendering::data-formats", 0.2, 0.), ("filesystem", 0.2, 0.)]),
        (Cond::Any(&["l10n", "localization", "localisation"]), &[("internationalization", 1.3, 0.2)]),
        (Cond::Any(&["make", "cmd"]), &[("internationalization", 0.4, 0.)]),

        (Cond::Any(&["time", "date", "timezone", "calendar", "tz", "dow", "sunrise", "time-ago", "hour-ago"]),
            &[("date-and-time", 1.35, 0.2), ("value-formatting", 1.1, 0.), ("no-std", 0.4, 0.), ("os", 0.8, 0.), ("command-line-interface", 0.7, 0.), ("parsing", 0.7, 0.), ("science", 0.7, 0.), ("no-std", 0.95, 0.), ("games", 0.1, 0.), ("cryptography::cryptocurrencies", 0.8, 0.)]),
        (Cond::Any(&["week", "solar", "time-zone", "sunset", "moon", "tzdata", "year", "timeago", "stopwatch", "chrono"]),
            &[("date-and-time", 1.3, 0.18), ("value-formatting", 1.1, 0.), ("no-std", 0.6, 0.), ("os", 0.9, 0.), ("os::windows-apis", 0.9, 0.), ("command-line-interface", 0.7, 0.), ("parsing", 0.7, 0.), ("science", 0.7, 0.), ("no-std", 0.95, 0.), ("games", 0.1, 0.), ("cryptography::cryptocurrencies", 0.8, 0.)]),
        (Cond::All(&["constant","time"]), &[("date-and-time", 0.3, 0.)]),
        (Cond::All(&["linear","time"]), &[("date-and-time", 0.7, 0.)]),
        (Cond::All(&["compile","time"]), &[("date-and-time", 0.4, 0.)]),
        (Cond::Any(&["uuid", "simulation", "failure", "fail", "iter", "domain", "engine", "kernel"]),
            &[("date-and-time", 0.4, 0.), ("value-formatting", 0.7, 0.), ("rendering::graphics-api", 0.9, 0.)]),
        (Cond::Any(&["nan", "profile", "float", "timecode", "tsc", "fps", "arrow", "compiler"]),
            &[("date-and-time", 0.4, 0.), ("network-programming", 0.8, 0.), ("os::windows-apis", 0.9, 0.), ("development-tools::debugging", 0.8, 0.)]),

        (Cond::Any(&["finance", "financial"]), &[("date-and-time", 0.7, 0.), ("science::math", 1.2, 0.1)]),
        (Cond::All(&["quantitative", "analysis"]), &[("date-and-time", 0.7, 0.), ("science::math", 1.2, 0.2)]),
        (Cond::All(&["bond", "period"]), &[("date-and-time", 0.7, 0.), ("science::math", 1.2, 0.2)]),
        (Cond::All(&["market", "quotes"]), &[("date-and-time", 0.7, 0.), ("science::math", 1.2, 0.2)]),
        (Cond::All(&["stock", "market"]), &[("date-and-time", 0.7, 0.), ("science::math", 1.2, 0.2)]),
        (Cond::Any(&["investment"]), &[("date-and-time", 0.7, 0.), ("science::math", 1.2, 0.1)]),

        (Cond::Any(&["layout"]), &[("gui", 1.1, 0.06), ("rendering::graphics-api", 1.05, 0.), ("database", 0.7, 0.)]),

        (Cond::NotAny(&["has:cargo-bin", "subcommand", "cargo-subcommand", "sub-command", "cargo-plugin", "cargo", "crate"]),
            &[("development-tools::cargo-plugins", 0.6, 0.)]),
        (Cond::Any(&["cargo-subcommand"]), &[
            ("development-tools::cargo-plugins", 1.8, 0.4), ("development-tools", 0.3, 0.), ("algorithms", 0.8, 0.),
            ("cryptography::cryptocurrencies", 0.6, 0.), ("development-tools::build-utils", 0.8, 0.)]),
        (Cond::Any(&["has:cargo-bin"]), &[
            ("development-tools::cargo-plugins", 1.2, 0.2), ("development-tools", 0.8, 0.), ("development-tools::build-utils", 0.7, 0.),
            ("development-tools::procedural-macro-helpers", 0.8, 0.), ("memory-management", 0.6, 0.)]),
        (Cond::All(&["has:cargo-bin", "has:is_dev"]), &[("development-tools::cargo-plugins", 1.2, 0.1)]),
        (Cond::All(&["cargo", "subcommand"]), &[("development-tools::cargo-plugins", 1.8, 0.4), ("development-tools", 0.7, 0.), ("development-tools::build-utils", 0.8, 0.), ("development-tools::testing", 0.8, 0.)]),
        (Cond::All(&["cargo", "debian"]), &[("development-tools::cargo-plugins", 1.3, 0.2), ("os::unix-apis", 1.3, 0.1)]),
        (Cond::All(&["cargo", "sub-command"]), &[("development-tools::cargo-plugins", 1.8, 0.4), ("development-tools", 0.7, 0.), ("development-tools::build-utils", 0.8, 0.)]),
        (Cond::Any(&["cargo"]), &[("development-tools::cargo-plugins", 1.2, 0.1), ("multimedia::encoding", 0.9, 0.), ("development-tools::build-utils", 1.1, 0.1)]),
        (Cond::Any(&["build-dependencies"]), &[("config", 0.5, 0.), ("development-tools::build-utils", 1.3, 0.15)]),
        (Cond::All(&["development", "helper"]), &[("development-tools", 1.1, 0.), ("development-tools::build-utils", 1.1, 0.)]),
        (Cond::All(&["build", "helper"]), &[("development-tools::build-utils", 1.1, 0.1)]),
        (Cond::Any(&["build-time", "libtool"]), &[("development-tools::build-utils", 1.2, 0.2), ("config", 0.9, 0.),("development-tools::cargo-plugins", 1.1, 0.)]),
        (Cond::All(&["build", "scripts"]), &[("development-tools::build-utils", 1.2, 0.2)]),
        (Cond::All(&["build", "script"]), &[("development-tools::build-utils", 1.2, 0.2)]),

        (Cond::Any(&["oauth", "auth", "authentication", "authorization", "authorisation", "credentials"]),
            &[("authentication", 1.4, 0.2), ("command-line-utilities", 0.6, 0.), ("hardware-support", 0.7, 0.), ("no-std", 0.9, 0.), ("accessibility", 0.7, 0.), ("config", 0.9, 0.), ("web-programming::http-client", 0.8, 0.), ("parsing", 0.7, 0.)]),
        (Cond::Any(&["diceware", "totp", "credentials", "authenticator"]),
            &[("authentication", 1.4, 0.2), ("hardware-support", 0.8, 0.), ("parsing", 0.7, 0.)]),
        (Cond::Any(&["gpg", "pgp"]),
            &[("authentication", 1.2, 0.1), ("cryptography", 1.2, 0.1)]),
        (Cond::Any(&["authorize", "authenticate", "2fa", "oauth2", "u2f", "credential", "passphrase"]),
            &[("authentication", 1.4, 0.2), ("command-line-utilities", 0.9, 0.), ("hardware-support", 0.8, 0.), ("config", 0.8, 0.), ("web-programming::http-client", 0.8, 0.), ("parsing", 0.7, 0.)]),
        (Cond::Any(&["secret", "secrets", "vaults"]),
            &[("authentication", 1.2, 0.), ("cryptography", 1.2, 0.)]),
        (Cond::Any(&["session", "askpass", "saml", "okta", "ldap", "sspi", "acl", "rbac", "login", "pam", "yubikey", "fido", "access-control", "authorization-framework"]),
            &[("authentication", 1.1, 0.05)]),

        (Cond::NotAny(&["database", "db", "databases", "datastore", "persistence", "wal", "diesel", "queryable", "indexed", "columnar", "persistent", "relational", "search",
            "dbms", "migrations", "key-value", "kv", "kvs", "sql", "sqlx", "nosql", "geoip", "key-value", "orm", "schema", "lmdb", "odbc", "transactions", "transactional",
            "sqlite3", "leveldb", "postgres", "postgresql", "dynamodb", "mysql", "hadoop", "sqlite", "mongo", "mongodb", "mongo-db", "memcached", "lucene", "elasticsearch", "tkiv", "cassandra", "rocksdb"]),
            &[("database-implementations", 0.8, 0.), ("database", 0.8, 0.)]),
        (Cond::Any(&["database", "databases", "datastore", "write-ahead-log"]), &[("database-implementations", 1.3, 0.3), ("no-std", 0.9, 0.), ("cryptography::cryptocurrencies", 0.9, 0.), ("multimedia::encoding", 0.8, 0.),("database", 1.3, 0.1), ("caching", 0.8, 0.), ("development-tools", 0.9, 0.)]),
        (Cond::All(&["personal", "information", "management"]), &[("database-implementations", 1.5, 0.3)]),
        (Cond::Any(&["sql"]), &[("database", 1.3, 0.1), ("database-implementations", 1.1, 0.1), ("accessibility", 0.8, 0.)]),
        (Cond::All(&["rpm", "redis"]), &[("databases", 1.25, 0.1), ("os", 0.5, 0.), ("parsing", 0.5, 0.), ("os::unix-apis", 0.7, 0.)]),
        (Cond::Any(&["redis", "elasticsearch", "elastic-search"]), &[("database", 1.25, 0.1), ("os", 0.5, 0.), ("parsing", 0.5, 0.), ("os::unix-apis", 0.7, 0.)]),
        (Cond::Any(&["nosql", "geoip", "key-value", "wal", "schema"]), &[
            ("database", 1.5, 0.3), ("database-implementations", 1.2, 0.1), ("data-structures", 1.2, 0.1),
            ("command-line-utilities", 0.5, 0.), ("rendering::engine", 0.6, 0.)]),
        (Cond::Any(&["tkiv", "transactions", "transactional"]), &[("database", 1.5, 0.3),("database-implementations", 1.2, 0.1), ("data-structures", 1.2, 0.1), ("command-line-utilities", 0.5, 0.)]),
        (Cond::Any(&["kv"]), &[("database", 1.1, 0.),("database-implementations", 1.1, 0.)]),
        (Cond::Any(&["sqlite", "hadoop"]), &[("web-programming", 0.7, 0.), ("web-programming::http-client", 0.8, 0.)]),
        (Cond::Any(&["database", "db", "sqlite", "sqlite3", "leveldb", "diesel", "postgres", "postgresql", "mysql", "dynamodb", "hadoop"]),
                &[("database", 1.4, 0.2), ("cryptography::cryptocurrencies", 0.5, 0.), ("cryptography", 0.7, 0.), ("text-processing", 0.7, 0.), ("rust-patterns", 0.7, 0.), ("database-implementations", 1.1, 0.),
                ("value-formatting", 0.7, 0.), ("os::macos-apis", 0.5, 0.), ("internationalization", 0.7, 0.), ("hardware-support", 0.6, 0.), ("web-programming", 0.9, 0.), ("algorithms", 0.9, 0.), ("data-structures", 0.9, 0.), ("web-programming::http-server", 0.8, 0.),
                ("command-line-interface", 0.5, 0.), ("multimedia::video", 0.5, 0.), ("command-line-utilities", 0.9, 0.), ("memory-management", 0.7, 0.), ("development-tools", 0.9, 0.)]),
        (Cond::Any(&["orm", "mongo", "mongodb", "mongo-db", "lucene", "elasticsearch", "memcached", "mariadb", "cassandra", "rocksdb", "redis", "couchdb"]),
                &[("database", 1.4, 0.2), ("database-implementations", 0.85, 0.), ("cryptography::cryptocurrencies", 0.5, 0.), ("cryptography", 0.7, 0.), ("text-processing", 0.7, 0.), ("rust-patterns", 0.7, 0.),
                ("value-formatting", 0.7, 0.), ("os::macos-apis", 0.5, 0.), ("internationalization", 0.7, 0.), ("hardware-support", 0.6, 0.), ("web-programming", 1.1, 0.), ("algorithms", 0.9, 0.), ("data-structures", 0.9, 0.),
                ("command-line-interface", 0.5, 0.), ("multimedia::video", 0.5, 0.), ("command-line-utilities", 0.9, 0.), ("memory-management", 0.7, 0.)]),
        (Cond::Any(&["distributed", "zookeeper"]), &[("text-processing", 0.4, 0.), ("network-programming", 1.1, 0.), ("encoding", 0.8, 0.), ("command-line-interface", 0.4, 0.)]),
        (Cond::All(&["kv", "distributed"]), &[("database", 1.3, 0.2), ("network-programming", 0.8, 0.), ("database-implementations", 1.2, 0.1)]),
        (Cond::Any(&["csv", "driver"]), &[("database-implementations", 0.9, 0.)]),
        (Cond::Any(&["validator"]), &[("database", 0.9, 0.)]),
        (Cond::Any(&["distributed"]), &[("asynchronous", 1.1, 0.)]),
        (Cond::Any(&["persistence", "persistent", "lsm-tree"]), &[("database", 1.1, 0.)]),
        (Cond::Any(&["search", "lsm-tree"]), &[("database", 1.2, 0.), ("algorithms", 1.2, 0.), ("config", 0.8, 0.), ("database-implementations", 1.2, 0.)]),
        (Cond::All(&["database", "embedded"]), &[("database", 1.2, 0.1), ("embedded", 0.2, 0.), ("database-implementations", 1.2, 0.1)]),
        (Cond::All(&["database", "has:is_sys"]), &[("database", 1.3, 0.1), ("database-implementations", 0.5, 0.)]),
        (Cond::All(&["elastic", "search"]), &[("database", 1.2, 0.), ("algorithms", 0.2, 0.), ("database-implementations", 0.2, 0.)]),
        (Cond::All(&["search", "engine"]), &[("database-implementations", 1.2, 0.2), ("algorithms", 0.8, 0.), ("data-structures", 0.6, 0.)]),
        (Cond::All(&["information", "retrieval"]), &[("database-implementations", 1.1, 0.1), ("algorithms", 0.9, 0.), ("data-structures", 0.9, 0.)]),
        (Cond::Any(&["search-engine"]), &[("database-implementations", 1.2, 0.2), ("algorithms", 0.8, 0.), ("data-structures", 0.6, 0.)]),
        (Cond::All(&["key", "value", "store"]), &[("database", 1.2, 0.2), ("database-implementations", 1.2, 0.2), ("multimedia::video", 0.5, 0.)]),
        (Cond::Any(&["rabbitmq", "zeromq", "amqp", "mqtt"]), &[("network-programming", 1.2, 0.), ("os::windows-apis", 0.7, 0.), ("algorithms", 0.5, 0.), ("web-programming", 1.2, 0.), ("multimedia::video", 0.5, 0.), ("multimedia", 0.5, 0.), ("asynchronous", 1.2, 0.)]),
        (Cond::Any(&["messaging"]), &[("network-programming", 1.2, 0.), ("web-programming", 1.1, 0.), ("asynchronous", 1.1, 0.)]),

        (Cond::All(&["aws", "rusoto", "nextcloud"]), &[("network-programming", 1.2, 0.1), ("multimedia::video", 0.8, 0.), ("data-structures", 0.6, 0.), ("algorithms", 0.2, 0.), ("parsing", 0.5, 0.), ("filesystem", 0.6, 0.), ("web-programming", 1.2, 0.1)]),
        (Cond::All(&["aws", "sdk"]), &[("network-programming", 1.2, 0.2), ("web-programming", 1.2, 0.1), ("data-structures", 0.6, 0.), ("algorithms", 0.8, 0.), ("filesystem", 0.5, 0.)]),
        (Cond::All(&["cloud", "google"]), &[("network-programming", 1.1, 0.), ("web-programming", 1.25, 0.25), ("algorithms", 0.8, 0.), ("data-structures", 0.6, 0.), ("no-std", 0.7, 0.), ("development-tools::build-utils", 0.6, 0.)]),
        (Cond::All(&["api", "client"]), &[("network-programming", 1.1, 0.05), ("web-programming", 1.1, 0.05), ("algorithms", 0.8, 0.), ("config", 0.8, 0.), ("value-formatting", 0.8, 0.), ("data-structures", 0.6, 0.), ("development-tools::cargo-plugins", 0.7, 0.)]),
        (Cond::All(&["api", "dep:reqwest"]), &[("web-programming", 1.1, 0.1), ("algorithms", 0.8, 0.), ("command-line-interface", 0.8, 0.), ("data-structures", 0.8, 0.)]),
        (Cond::All(&["api", "dep:url"]), &[("web-programming", 1.1, 0.1)]),
        (Cond::All(&["client", "library"]), &[("network-programming", 1.1, 0.05), ("web-programming", 1.1, 0.05), ("algorithms", 0.8, 0.), ("value-formatting", 0.8, 0.)]),
        (Cond::Any(&["dep:reqwest"]), &[("network-programming", 1.1, 0.0), ("web-programming", 1.1, 0.0), ("algorithms", 0.8, 0.), ("data-structures", 0.9, 0.), ("memory-management", 0.6, 0.)]),
        (Cond::All(&["service", "google"]), &[("network-programming", 1.1, 0.), ("algorithms", 0.7, 0.), ("web-programming", 1.22, 0.2), ("data-structures", 0.6, 0.)]),
        (Cond::Any(&["service", "daemon", "server"]), &[("algorithms", 0.7, 0.), ("data-structures", 0.7, 0.)]),
        (Cond::Any(&["dep:rusoto_core"]), &[("network-programming", 1.1, 0.1), ("web-programming", 1.1, 0.1)]),
        (Cond::Any(&["rusoto", "azure", "cloudflare", "amazon", "google-apis", "aws-lambda", "aws-lambda-functions"]),
            &[("network-programming", 1.3, 0.3), ("web-programming", 1.2, 0.1), ("algorithms", 0.8, 0.),
            ("no-std", 0.7, 0.), ("multimedia::video", 0.9, 0.), ("cryptography::cryptocurrencies", 0.6, 0.)]),

        (Cond::Any(&["compress", "compression", "rar", "archive", "archives", "zip", "gzip"]),
            &[("compression", 1.3, 0.3), ("cryptography", 0.7, 0.), ("games", 0.4, 0.), ("no-std", 0.7, 0.), ("asynchronous", 0.9, 0.),
            ("command-line-interface", 0.4, 0.), ("command-line-utilities", 0.8, 0.), ("development-tools::testing", 0.6, 0.), ("development-tools::profiling", 0.2, 0.)]),
        (Cond::Any(&["zlib", "libz", "7z", "lz4", "adler32", "brotli", "huffman", "xz", "lzma", "decompress", "deflate"]),
            &[("compression", 1.3, 0.3), ("cryptography", 0.6, 0.), ("parsing", 0.6, 0.), ("games", 0.4, 0.), ("command-line-interface", 0.4, 0.),
            ("command-line-utilities", 0.8, 0.),  ("development-tools::testing", 0.6, 0.), ("development-tools::profiling", 0.2, 0.)]),

        (Cond::Any(&["dep:sdl2", "graphics"]), &[("compression", 0.8, 0.), ("games", 1.1, 0.), ("no-std", 0.8, 0.)]),

        (Cond::NotAny(&["simulation", "simulator", "vm", "virtual", "dynamics", "nets", "sim", "particle", "city", "fluid", "systems", "real-time", "physics", "automata", "quantum"]), &[("simulation", 0.8, 0.)]),

        (Cond::Any(&["simulation", "simulator"]), &[("simulation", 1.3, 0.3), ("emulators", 1.15, 0.1), ("parser-implementations", 0.8, 0.), ("multimedia::encoding", 0.8, 0.)]),
        (Cond::Any(&["real-time", "realtime"]), &[("simulation", 1.1, 0.0), ("parser-implementations", 0.6, 0.)]),
        (Cond::All(&["software", "implementation"]), &[("simulation", 1.3, 0.), ("emulators", 1.2, 0.), ("development-tools", 0.8, 0.)]),
        (Cond::Any(&["animation", "anim"]), &[("multimedia", 1.2, 0.), ("multimedia::video", 1.2, 0.1), ("asynchronous", 0.9, 0.), ("rendering", 1.1, 0.), ("simulation", 0.7, 0.)]),
        (Cond::Any(&["dep:gstreamer"]), &[("multimedia", 1.2, 0.), ("multimedia::video", 1.3, 0.1)]),
        (Cond::Any(&["dep:image"]), &[("multimedia", 1.1, 0.), ("multimedia::images", 1.1, 0.05)]),

        (Cond::Any(&["rsync", "scp", "xmpp", "ldap", "openssh", "ssh", "socks5", "elb", "iptables", "kademlia", "bittorrent", "sctp", "docker"]),
            &[("network-programming", 1.2, 0.2), ("web-programming", 0.6, 0.), ("parsing", 0.6, 0.), ("development-tools::testing", 0.5, 0.),
            ("algorithms", 0.9, 0.), ("asynchronous", 0.9, 0.), ("os::windows-apis", 0.6, 0.)]),
        (Cond::Any(&["bot", "netsec", "waf", "curl", "net", "notification", "chat"]),
            &[("network-programming", 1.1, 0.1), ("web-programming", 1.1, 0.1), ("parsing", 0.8, 0.), ("data-structures", 0.8, 0.),
            ("development-tools::procedural-macro-helpers", 0.7, 0.), ("rendering::graphics-api", 0.9, 0.)]),
        (Cond::Any(&["rpki", "bgp"]), &[("network-programming", 1.2, 0.15)]),
        (Cond::Any(&["rpki"]), &[("network-programming", 1.1, 0.1), ("cryptography", 1.1, 0.1)]),
        (Cond::Any(&["ip", "ipv6", "ipv4", "network", "internet"]), &[("network-programming", 1.2, 0.1), ("web-programming", 1.1, 0.), ("parsing", 0.8, 0.)]),
        (Cond::Any(&["proxy", "networking", "cidr"]), &[("network-programming", 1.2, 0.1), ("web-programming", 1.1, 0.), ("parsing", 0.5, 0.), ("algorithms", 0.8, 0.), ("data-structures", 0.9, 0.)]),
        (Cond::Any(&["http2", "http/2", "http", "https", "httpd", "tcp", "icmp", "irc", "tcp-client", "multicast", "anycast", "bgp", "amazon", "aws", "amazon-s3", "cloud", "service"]),
            &[("network-programming", 1.1, 0.1), ("filesystem", 0.7, 0.), ("memory-management", 0.5, 0.), ("asynchronous", 0.8, 0.), ("algorithms", 0.8, 0.), ("text-processing", 0.8, 0.),
            ("command-line-interface", 0.5, 0.), ("development-tools::procedural-macro-helpers", 0.8, 0.), ("development-tools::build-utils", 0.6, 0.)]),
        (Cond::Any(&["ipfs", "io", "ceph"]), &[("network-programming", 1.2, 0.1), ("filesystem", 1.3, 0.1), ("cryptography", 0.8, 0.), ("text-processing", 0.7, 0.), ("command-line-interface", 0.5, 0.)]),
        (Cond::Any(&["irc", "dht", "bot", "icmp"]), &[("network-programming", 1.2, 0.1), ("parsing", 0.9, 0.), ("asynchronous", 0.8, 0.)]),
        (Cond::Any(&["pipe", "read", "write", "mtime", "atime"]), &[("filesystem", 1.1, 0.), ("development-tools::profiling", 0.6, 0.), ("science", 0.8, 0.)]),

        (Cond::Any(&["pointers", "pointer", "slices", "primitive", "primitives", "clone-on-write"]),
                &[("rust-patterns", 1.2, 0.1), ("command-line-utilities", 0.7, 0.), ("command-line-interface", 0.8, 0.), ("no-std", 0.95, 0.), ("asynchronous", 0.8, 0.),
                ("development-tools::testing", 0.9, 0.), ("internationalization", 0.7, 0.), ("template-engine", 0.8, 0.)]),
        (Cond::Any(&["references", "methods", "own", "function", "variables", "inference", "assert"]),
                &[("rust-patterns", 1.2, 0.1), ("no-std", 0.9, 0.), ("command-line-utilities", 0.7, 0.), ("command-line-interface", 0.8, 0.), ("no-std", 0.95, 0.), ("asynchronous", 0.8, 0.),
                ("development-tools::testing", 0.9, 0.), ("internationalization", 0.7, 0.), ("template-engine", 0.8, 0.)]),
        (Cond::Any(&["endianness", "derive", "float", "delegation", "floats", "floating-point", "downcasting", "initialized", "primitives", "tuple", "type-level", "panic", "literal"]),
                &[("rust-patterns", 1.2, 0.1), ("no-std", 0.8, 0.), ("science", 0.8, 0.), ("science", 0.8, 0.), ("science::ml", 0.7, 0.), ("science::math", 0.88, 0.), ("os", 0.9, 0.),
                ("command-line-utilities", 0.7, 0.), ("command-line-interface", 0.8, 0.), ("memory-management", 0.8, 0.), ("rendering", 0.8, 0.), ("rendering::graphics-api", 0.7, 0.), ("template-engine", 0.8, 0.),
                ("hardware-support", 0.5, 0.), ("development-tools::cargo-plugins", 0.3, 0.), ("development-tools::testing", 0.7, 0.)]),
        (Cond::Any(&["trait", "cow", "range", "annotation", "abstractions", "abstraction", "generics", "interning", "tailcall", "traits", "pin-api", "contravariant", "metaprogramming", "type-level", "unreachable", "oop", "type", "types", "scoped", "scope", "functions", "clone"]),
                &[("rust-patterns", 1.2, 0.1), ("no-std", 0.7, 0.), ("science", 0.8, 0.), ("authentication", 0.8, 0.), ("games", 0.8, 0.), ("science", 0.8, 0.), ("science::ml", 0.7, 0.), ("science::math", 0.88, 0.), ("os", 0.9, 0.),
                ("command-line-utilities", 0.7, 0.), ("command-line-interface", 0.8, 0.), ("memory-management", 0.8, 0.), ("rendering", 0.8, 0.), ("rendering::graphics-api", 0.7, 0.), ("template-engine", 0.8, 0.),
                ("hardware-support", 0.5, 0.), ("algorithms", 0.9, 0.), ("config", 0.7, 0.), ("development-tools", 0.6, 0.), ("development-tools::cargo-plugins", 0.3, 0.), ("development-tools::testing", 0.7, 0.)]),
        (Cond::Any(&["u128", "closure", "trait-object", "trait-objects", "visitor-pattern", "transmute", "unwrap", "fnonce", "cell", "object-safe", "byteorder", "printf", "nightly", "std",  "macro", "null", "standard-library"]),
                &[("rust-patterns", 1.2, 0.1), ("algorithms", 0.8, 0.), ("no-std", 0.8, 0.), ("science", 0.8, 0.), ("science::ml", 0.7, 0.), ("science::math", 0.88, 0.), ("os", 0.9, 0.),
                ("rendering::graphics-api", 0.9, 0.), ("command-line-utilities", 0.7, 0.), ("command-line-interface", 0.8, 0.), ("memory-management", 0.8, 0.),
                ("rendering", 0.8, 0.), ("hardware-support", 0.6, 0.), ("development-tools::cargo-plugins", 0.4, 0.), ("development-tools::testing", 0.8, 0.)]),
        (Cond::Any(&["enum", "boilerplate", "prelude", "boxing", "error", "error-handling", "println", "dsl"]),
                &[("rust-patterns", 1.2, 0.1), ("algorithms", 0.7, 0.), ("science", 0.7, 0.), ("science::ml", 0.7, 0.), ("science::math", 0.7, 0.), ("os", 0.8, 0.), ("command-line-utilities", 0.7, 0.), ("command-line-interface", 0.8, 0.), ("memory-management", 0.8, 0.), ("rendering", 0.8, 0.), ("hardware-support", 0.5, 0.), ("development-tools::cargo-plugins", 0.4, 0.), ("development-tools::ffi", 0.4, 0.), ("development-tools::testing", 0.7, 0.)]),
        (Cond::All(&["error", "handling"]), &[("rust-patterns", 1.2, 0.), ("no-std", 0.8, 0.),]),
        (Cond::Any(&["error-handling", "higher-order"]), &[("rust-patterns", 1.2, 0.1), ("data-structures", 0.8, 0.)]),
        (Cond::All(&["proc", "macro"]), &[("rust-patterns", 1.1, 0.1), ("no-std", 0.6, 0.), ("development-tools::procedural-macro-helpers", 1.2, 0.2), ("rendering::graphics-api", 0.8, 0.),]),
        (Cond::All(&["rust", "macro"]), &[("rust-patterns", 1.1, 0.1), ("development-tools::procedural-macro-helpers", 1.1, 0.1)]),
        (Cond::Any(&["dep:quote"]), &[("development-tools::procedural-macro-helpers", 1.3, 0.15), ("command-line-utilities", 0.3, 0.)]),
        (Cond::Any(&["dep:darling"]), &[("development-tools::procedural-macro-helpers", 1.2, 0.1)]),
        (Cond::Any(&["dep:proc-macro-hack"]), &[("development-tools::procedural-macro-helpers", 1.2, 0.1)]),
        (Cond::Any(&["dep:syn"]), &[("development-tools::procedural-macro-helpers", 1.2, 0.15), ("data-structures", 0.7, 0.), ("command-line-utilities", 0.3, 0.)]),
        (Cond::Any(&["dep:proc-macro2"]), &[("development-tools::procedural-macro-helpers", 1.2, 0.1), ("command-line-utilities", 0.3, 0.), ("data-structures", 0.3, 0.), ("compilers", 0.3, 0.), ("algorithms", 0.5, 0.)]),
        (Cond::All(&["implementations"]), &[("games", 0.8, 0.), ("development-tools", 0.8, 0.)]),
        (Cond::Any(&["singleton", "iterators", "newtype", "dictionary", "functor", "monad", "haskell", "mutation"]),
            &[("rust-patterns", 1.1, 0.1), ("command-line-utilities", 0.7, 0.), ("development-tools", 0.8, 0.), ("memory-management", 0.8, 0.), ("internationalization", 0.8, 0.), ("command-line-interface", 0.8, 0.), ("games", 0.5, 0.)]),
        (Cond::Any(&["rustc", "string", "strings", "num", "struct", "coproduct", "slice", "assert",]),
            &[("rust-patterns", 1.1, 0.1), ("command-line-utilities", 0.7, 0.), ("development-tools", 0.8, 0.),
            ("memory-management", 0.8, 0.), ("command-line-interface", 0.8, 0.), ("games", 0.5, 0.), ("parser-implementations", 0.7, 0.)]),
        (Cond::Any(&["monoidal", "monoid", "type-level", "bijective", "impl", "semigroup"]),
            &[("rust-patterns", 1.1, 0.1), ("command-line-utilities", 0.7, 0.), ("no-std", 0.8, 0.), ("internationalization", 0.7, 0.), ("development-tools", 0.8, 0.), ("os::macos-apis", 0.8, 0.),
            ("memory-management", 0.8, 0.), ("command-line-interface", 0.8, 0.), ("games", 0.5, 0.), ("parser-implementations", 0.7, 0.)]),
        (Cond::Any(&["iterator", "owned-string", "type-inference"]),
            &[("rust-patterns", 1.1, 0.1), ("algorithms", 1.1, 0.1), ("gui", 0.9, 0.), ("no-std", 0.8, 0.)]),
        (Cond::Any(&["stack", "builder", "nan", "zero-cost"]),
            &[("rust-patterns", 1.1, 0.1), ("algorithms", 1.1, 0.1), ("gui", 0.9, 0.)]),
        (Cond::Any(&["structure", "endian", "big-endian", "binary", "binaries", "storing-values"]),
            &[("data-structures", 1.2, 0.1), ("algorithms", 1.1, 0.), ("science", 0.8, 0.), ("multimedia::audio", 0.9, 0.), ("command-line-utilities", 0.9, 0.), ("text-editors", 0.7, 0.), ("internationalization", 0.7, 0.)]),
        (Cond::Any(&["structures", "trie", "linked-list", "incremental", "tree", "trees", "interner", "internment",  "intersection"]),
            &[("data-structures", 1.22, 0.1), ("algorithms", 1.1, 0.), ("science", 0.8, 0.), ("no-std", 0.8, 0.), ("multimedia::audio", 0.9, 0.), ("command-line-utilities", 0.9, 0.), ("text-editors", 0.7, 0.), ("internationalization", 0.7, 0.)]),
        (Cond::Any(&["data-structure", "vec2d",]),
            &[("data-structures", 1.2, 0.1), ("algorithms", 0.8, 0.), ("compilers", 0.7, 0.), ("science", 0.8, 0.), ("no-std", 0.8, 0.), ("multimedia::audio", 0.9, 0.), ("command-line-utilities", 0.9, 0.), ("text-editors", 0.7, 0.), ("internationalization", 0.7, 0.)]),
        (Cond::All(&["structures", "data"]), &[("data-structures", 1.2, 0.2), ("algorithms", 0.9, 0.)]),
        (Cond::All(&["structure", "data"]), &[("data-structures", 1.2, 0.3), ("algorithms", 0.9, 0.)]),
        (Cond::All(&["functional", "programming"]), &[("rust-patterns", 1.1, 0.05)]),
        (Cond::All(&["chunking"]), &[("algorithms", 1.1, 0.05)]),
        (Cond::All(&["algorithm"]), &[("algorithms", 1.1, 0.05)]),
        (Cond::All(&["deduplication"]), &[("algorithms", 1.1, 0.), ("filesystem", 1.1, 0.)]),
        (Cond::Any(&["convolution", "movie"]), &[("games", 0.5, 0.), ("data-structures", 0.9, 0.)]),
        (Cond::Any(&["dsp", "movies"]), &[("games", 0.5, 0.), ("parsing", 0.5, 0.), ("data-structures", 0.9, 0.)]),
        (Cond::Any(&["convolution", "dsp"]), &[("algorithms", 1.4, 0.1), ("data-structures", 0.8, 0.)]),
        (Cond::Any(&["collection", "collections", "ringbuffer"]), &[("data-structures", 1.2, 0.1), ("algorithms", 0.9, 0.)]),
        (Cond::Any(&["safe", "unsafe", "specialized", "convenience", "helper", "helpers"]),
            &[("rust-patterns", 1.1, 0.), ("science", 0.8, 0.), ("cryptography::cryptocurrencies", 0.8, 0.)]),
        (Cond::Any(&["safe", "unsafe"]), &[("multimedia::video", 0.8, 0.), ("development-tools::cargo-plugins", 0.5, 0.), ("command-line-utilities", 0.9, 0.), ("rendering::engine", 0.8, 0.)]),

        (Cond::Any(&["algorithms", "convert", "converter", "guid", "algorithm", "algorithmic", "algos", "convex", "sliding"]),
            &[("algorithms", 1.2, 0.2), ("cryptography", 0.8, 0.), ("web-programming::http-client", 0.8, 0.), ("development-tools::testing", 0.5, 0.),
            ("development-tools", 0.5, 0.), ("data-structures", 0.9, 0.), ("gui", 0.9, 0.)]),
        (Cond::Any(&["integer", "floating-point","partition", "sequences", "quadtree", "lookup",  "kernels", "sieve", "values"]),
            &[("algorithms", 1.1, 0.1), ("data-structures", 1.1, 0.1), ("no-std", 0.9, 0.), ("science::math", 0.8, 0.), ("os", 0.8, 0.), ("games", 0.5, 0.), ("memory-management", 0.75, 0.), ("multimedia::video", 0.8, 0.)]),
        (Cond::Any(&["implementation", "generator", "normalize", "random", "ordered", "set", "hierarchical", "multimap", "bitvector", "integers"]),
            &[("algorithms", 1.1, 0.1), ("data-structures", 1.1, 0.1), ("science::math", 0.8, 0.), ("os", 0.8, 0.), ("games", 0.5, 0.), ("memory-management", 0.75, 0.), ("multimedia::video", 0.8, 0.)]),
        (Cond::Any(&["bloom", "arrays", "list", "vec", "container", "octree", "binary-tree", "hashmap", "hashtable", "map"]),
            &[("data-structures", 1.2, 0.1), ("algorithms", 1.1, 0.1), ("science::math", 0.8, 0.), ("os", 0.9, 0.), ("games", 0.8, 0.), ("memory-management", 0.75, 0.), ("multimedia::video", 0.8, 0.)]),
        (Cond::Any(&["concurrent", "mpsc", "mpmc", "spsc", "producer", "condition",  "mutex", "rwlock", "futex"]), &[
            ("concurrency", 1.3, 0.15), ("algorithms", 1.1, 0.1), ("data-structures", 0.9, 0.)]),
        (Cond::All(&["concurrent", "queue"]), &[("concurrency", 1.3, 0.15), ("no-std", 0.7, 0.), ("caching", 0.8, 0.), ("data-structures", 0.8, 0.)]),
        (Cond::All(&["message", "queue"]), &[("asynchronous", 1.2, 0.1)]),
        (Cond::All(&["bloom", "filter"]), &[("data-structures", 1.3, 0.2), ("accessibility", 0.5, 0.)]),
        (Cond::Any(&["scheduler", "publisher", "lock", "deque", "channel"]), &[("concurrency", 1.3, 0.15), ("algorithms", 0.9, 0.), ("os", 1.1, 0.), ("data-structures", 0.8, 0.)]),
        (Cond::Any(&["thread"]), &[("concurrency", 1.2, 0.1), ("algorithms", 0.9, 0.), ("data-structures", 0.9, 0.)]),
        (Cond::Any(&["persistent", "immutable", "persistent-datastructures"]), &[("algorithms", 1.15, 0.1), ("data-structures", 1.3, 0.2), ("database-implementations", 1.1, 0.05)]),

        (Cond::Any(&["statistics", "statistic", "order-statistics", "svd", "markov", "cognitive"]),
            &[("science", 1.2, 0.1), ("science::ml", 1.25, 0.), ("algorithms", 1.2, 0.1), ("command-line-interface", 0.3, 0.), ("simulation", 0.75, 0.),("command-line-utilities", 0.75, 0.), ("games", 0.3, 0.), ("no-std", 0.95, 0.), ("caching", 0.8, 0.), ("development-tools::profiling", 0.6, 0.), ("internationalization", 0.7, 0.)]),
        (Cond::Any(&["variance", "units", "subsequences", "lazy", "linear", "distribution", "computation", "compute"]),
            &[("science", 1.25, 0.2), ("algorithms", 1.2, 0.1), ("data-structures", 1.2, 0.1), ("command-line-interface", 0.3, 0.), ("simulation", 0.75, 0.),("command-line-utilities", 0.75, 0.), ("games", 0.3, 0.), ("no-std", 0.95, 0.), ("caching", 0.8, 0.), ("development-tools::profiling", 0.6, 0.), ("internationalization", 0.7, 0.)]),
        (Cond::Any(&["computational", "hpc", "tries", "collection", "pathfinding", "rational", "newtonian", "scientific", "science"]),
            &[("science", 1.25, 0.2), ("algorithms", 1.2, 0.1), ("data-structures", 1.2, 0.1), ("command-line-interface", 0.3, 0.), ("simulation", 0.75, 0.),("command-line-utilities", 0.75, 0.), ("games", 0.3, 0.), ("no-std", 0.95, 0.), ("caching", 0.8, 0.), ("development-tools::profiling", 0.6, 0.), ("internationalization", 0.7, 0.)]),
        (Cond::Any(&["median", "alpha", "equations", "bigdecimal", "matrix", "proving", "matrices", "sat", "multi-dimensional", "convex", "unification"]),
            &[("science", 1.2, 0.1), ("science::math", 1.2, 0.1), ("algorithms", 1.2, 0.1), ("text-processing", 0.8, 0.), ("data-structures", 1.2, 0.1), ("command-line-interface", 0.3, 0.), ("simulation", 0.75, 0.), ("memory-management", 0.8, 0.), ("caching", 0.8, 0.),("command-line-utilities", 0.75, 0.), ("games", 0.3, 0.), ("no-std", 0.95, 0.), ("caching", 0.8, 0.), ("development-tools::profiling", 0.6, 0.), ("internationalization", 0.7, 0.)]),
        (Cond::Any(&["fibonacci", "interpolate", "interpolation", "coordinates", "permutation", "dfa", "cellular-automata", "automata", "solvers", "solver", "integral"]),
            &[("science", 1.2, 0.1), ("science::math", 1.2, 0.1), ("algorithms", 1.3, 0.1), ("text-processing", 0.8, 0.), ("cryptography", 0.9, 0.), ("data-structures", 1.1, 0.1), ("command-line-interface", 0.3, 0.), ("memory-management", 0.8, 0.), ("caching", 0.8, 0.), ("simulation", 0.75, 0.),("command-line-utilities", 0.75, 0.), ("games", 0.3, 0.), ("no-std", 0.95, 0.), ("caching", 0.8, 0.), ("development-tools::profiling", 0.6, 0.), ("internationalization", 0.7, 0.)]),
        (Cond::Any(&["graph", "sparse", "summed", "kd-tree"]),
            &[("data-structures", 1.5, 0.2), ("algorithms", 1.3, 0.1), ("science", 1.2, 0.2), ("database", 0.7, 0.), ("concurrency", 0.9, 0.), ("command-line-interface", 0.3, 0.), ("command-line-utilities", 0.75, 0.)]),

        (Cond::Any(&["procedural", "procgen"]), &[("algorithms", 1.25, 0.2), ("game-development", 1.25, 0.), ("games", 0.8, 0.), ("multimedia::images", 1.05, 0.)]),
        (Cond::All(&["finite", "state"]), &[("algorithms", 1.1, 0.), ("science::math", 0.8, 0.)]),
        (Cond::All(&["finite", "automata"]), &[("algorithms", 1.25, 0.2), ("science::math", 0.8, 0.)]),
        (Cond::All(&["machine", "state"]), &[("algorithms", 1.25, 0.2), ("science::math", 0.8, 0.)]),
        (Cond::All(&["fsm", "state"]), &[("algorithms", 1.25, 0.2), ("science::math", 0.8, 0.)]),
        (Cond::All(&["machine", "state", "logic", "fuzzy"]), &[("algorithms", 1.25, 0.2), ("science::math", 0.8, 0.)]),
        (Cond::Any(&["state-machine", "statemachine", "stateful"]), &[("algorithms", 1.25, 0.2), ("science", 1.1, 0.), ("science::math", 0.7, 0.)]),
        (Cond::Any(&["worker", "taskqueue", "a-star", "easing", "sorter", "sorting", "prng", "random", "mersenne"]),
                &[("algorithms", 1.25, 0.1), ("science::math", 0.8, 0.), ("caching", 0.8, 0.), ("no-std", 0.8, 0.), ("command-line-interface", 0.4, 0.), ("database", 0.8, 0.), ("os", 0.9, 0.), ("command-line-utilities", 0.4, 0.)]),
        (Cond::Any(&["prolog"]), &[("algorithms", 1.25, 0.1), ("cryptography", 0.5, 0.)]),
        (Cond::Any(&["lock-free"]),
                &[("data-structures", 1.25, 0.1), ("concurrency", 1.1, 0.), ("algorithms", 1.1, 0.), ("science::math", 0.8, 0.), ("caching", 0.8, 0.), ("command-line-interface", 0.4, 0.), ("os", 0.9, 0.), ("command-line-utilities", 0.4, 0.)]),
        (Cond::Any(&["queue", "collection", "sort"]),
                &[("data-structures", 1.25, 0.1), ("algorithms", 1.1, 0.), ("caching", 0.9, 0.), ("science::math", 0.8, 0.), ("caching", 0.8, 0.), ("command-line-interface", 0.4, 0.), ("os", 0.9, 0.), ("command-line-utilities", 0.4, 0.)]),
        (Cond::Any(&["hyperloglog", "index-scan"]),
                &[("data-structures", 1.1, 0.1), ("algorithms", 1.3, 0.2), ("science::math", 1.1, 0.), ("database-implementations", 1.1, 0.)]),

        (Cond::Any(&["macro", "macros", "dsl", "procedural-macros", "proc-macro", "proc-macros", "derive", "proc_macro", "custom-derive"]), &[
            ("development-tools::procedural-macro-helpers", 1.4, 0.2), ("no-std", 0.7, 0.), ("multimedia::video", 0.6, 0.), ("multimedia", 0.8, 0.), ("rust-patterns", 1.2, 0.1), ("cryptography", 0.7, 0.),
            ("memory-management", 0.7, 0.), ("internationalization", 0.7, 0.), ("algorithms", 0.8, 0.), ("science::math", 0.7, 0.),
            ("web-programming::websocket", 0.6, 0.), ("no-std", 0.8, 0.), ("compression", 0.8, 0.), ("command-line-interface", 0.5, 0.),
            ("development-tools::testing", 0.8, 0.), ("development-tools::debugging", 0.8, 0.), ("development-tools::build-utils", 0.6, 0.)]),
        (Cond::Any(&["has:proc_macro"]), &[("development-tools::procedural-macro-helpers", 1.5, 0.3), ("command-line-utilities", 0.3, 0.), ("rust-patterns", 0.7, 0.)]),
        (Cond::NotAny(&["has:proc_macro", "derive", "proc-macro", "proc-macros", "dep:syn", "dep:quote", "proc", "procmacro", "macros", "syntax"]), &[("development-tools::procedural-macro-helpers", 0.6, 0.)]),

        (Cond::Any(&["similarity", "string"]), &[("development-tools::procedural-macro-helpers", 0.9, 0.), ("rust-patterns", 0.9, 0.)]),

        (Cond::Any(&["emoji", "stemming", "highlighting", "whitespace", "uppercase", "indentation", "spellcheck"]), &[("text-processing", 1.4, 0.2), ("science::math", 0.8, 0.), ("algorithms", 0.8, 0.)]),
        (Cond::Any(&["regex", "matching"]), &[("text-processing", 1.2, 0.1), ("science::math", 0.8, 0.), ("science::math", 0.8, 0.), ("filesystem", 0.7, 0.), ("science::math", 0.8, 0.)]),
        (Cond::Any(&["memchr"]), &[("text-processing", 1.1, 0.1), ("algorithms", 1.1, 0.1), ("internationalization", 0.5, 0.)]),
        (Cond::Any(&["string"]), &[("text-processing", 1.1, 0.), ("internationalization", 0.9, 0.)]),
        (Cond::Any(&["strchr"]), &[("text-processing", 1.1, 0.), ("internationalization", 0.7, 0.)]),
        (Cond::Any(&["dep:csv"]), &[("text-processing", 1.1, 0.1)]),
        (Cond::Any(&["pulldown-cmark", "dep:pulldown-cmark", "bbcode", "ascii", "ngrams"]),
            &[("text-processing", 1.2, 0.2), ("parser-implementations", 1.2, 0.), ("no-std", 0.8, 0.), ("multimedia::video", 0.8, 0.), ("parsing", 0.9, 0.), ("development-tools::testing", 0.2, 0.), ("filesystem", 0.7, 0.), ("development-tools", 0.7, 0.), ("multimedia::images", 0.5, 0.), ("command-line-utilities", 0.8, 0.)]),
        (Cond::Any(&["markdown", "commonmark", "common-mark", "unicode-aware", "latex", "mdbook", "dep:mdbook"]),
            &[("text-processing", 1.2, 0.2), ("parser-implementations", 1.2, 0.),
            ("multimedia::video", 0.8, 0.), ("compression", 0.6, 0.), ("cryptography", 0.6, 0.), ("parsing", 0.9, 0.), ("development-tools::testing", 0.2, 0.),
            ("filesystem", 0.7, 0.), ("development-tools", 0.8, 0.), ("multimedia::images", 0.5, 0.), ("command-line-interface", 0.9, 0.)]),
        (Cond::Any(&["case-folding", "text", "character-property", "character"]),
            &[("text-processing", 1.1, 0.2), ("rendering", 0.9, 0.), ("embedded", 0.8, 0.), ("web-programming", 0.9, 0.), ("rendering::data-formats", 0.5, 0.), ("development-tools::testing", 0.6, 0.)]),
        (Cond::Any(&["unicode",  "characters", "grapheme", "crlf", "codepage", "whitespace", "utf", "utf-8", "utf8", "case"]),
            &[("text-processing", 1.1, 0.2), ("rendering", 0.9, 0.), ("embedded", 0.8, 0.), ("no-std", 0.9, 0.), ("games", 0.7, 0.), ("web-programming", 0.9, 0.), ("rendering::data-formats", 0.5, 0.), ("development-tools::testing", 0.6, 0.)]),
        (Cond::Any(&["markdown", "commonmark", "common-mark", "latex", "dep:mdbook"]),
            &[("compilers", 0.6, 0.)]),

        (Cond::Any(&["ascii", "grep"]), &[("text-processing", 1.2, 0.)]),
        (Cond::Any(&["dep:hangul", "dep:sejong", "dep:hanja"]), &[("text-processing", 1.2, 0.1), ("internationalization", 1.2, 0.1)]),
        (Cond::All(&["ascii", "convert"]), &[("text-processing", 1.2, 0.1)]),
        (Cond::Any(&["pdf", "epub", "ebook", "book", "typesetting", "xetex"]),
            &[("text-processing", 1.3, 0.2), ("science", 0.9, 0.), ("science::math", 0.8, 0.), ("no-std", 0.7, 0.), ("games", 0.8, 0.),
            ("database-implementations", 0.8, 0.), ("rendering::data-formats", 1.2, 0.),
            ("rendering", 1.05, 0.), ("web-programming::http-client", 0.5, 0.), ("parsing", 0.8, 0.), ("command-line-interface", 0.5, 0.), ("visualization", 0.7, 0.)]),
        (Cond::Any(&["dep:lopdf", "dep:pdfpdf"]), &[("text-processing", 1.2, 0.1)]),
        (Cond::All(&["auto", "correct"]), &[("text-processing", 1.2, 0.1), ("multimedia::images", 0.5, 0.)]),

        (Cond::Any(&["templating", "template", "template-engine", "handlebars"]),
            &[("template-engine", 1.4, 0.3), ("embedded", 0.2, 0.), ("games", 0.5, 0.), ("no-std", 0.8, 0.), ("internationalization", 0.6, 0.), ("development-tools::cargo-plugins", 0.7, 0.), ("command-line-interface", 0.4, 0.)]),

        (Cond::Any(&["benchmark","benchmarking","profiling"]),
            &[("development-tools::profiling", 1.2, 0.2), ("rust-patterns", 0.94, 0.), ("algorithms", 0.8, 0.), ("cryptography::cryptocurrencies", 0.7, 0.),
            ("simulation", 0.75, 0.), ("science::robotics", 0.7, 0.), ("parsing", 0.8, 0.), ("os::macos-apis", 0.9, 0.), ("authentication", 0.5, 0.)]),
        (Cond::Any(&["bench", "profiler"]),
            &[("development-tools::profiling", 1.2, 0.2), ("rust-patterns", 0.94, 0.), ("algorithms", 0.8, 0.), ("cryptography::cryptocurrencies", 0.7, 0.),
            ("simulation", 0.75, 0.), ("parsing", 0.8, 0.), ("os::macos-apis", 0.9, 0.), ("web-programming::http-client", 0.8, 0.), ("authentication", 0.5, 0.)]),
        (Cond::Any(&["perf", "performance", "optimizer"]), &[("development-tools::profiling", 1.2, 0.1), ("development-tools", 1.1, 0.)]),
        (Cond::Any(&["spdx"]), &[("development-tools", 1.1, 0.1), ("no-std", 0.8, 0.)]),
        (Cond::All(&["sampling", "profiler"]), &[("development-tools::profiling", 1.2, 0.1), ("development-tools", 1.1, 0.), ("web-programming::http-client", 0.6, 0.)]),
        (Cond::All(&["tracing", "profiler"]), &[("development-tools::profiling", 1.2, 0.1), ("development-tools", 1.1, 0.), ("web-programming::http-client", 0.8, 0.)]),
        (Cond::Any(&["version"]), &[("development-tools::profiling", 0.8, 0.)]),
        (Cond::Any(&["bump", "changelog"]), &[("development-tools", 1.2, 0.), ("development-tools::profiling", 0.6, 0.)]),
        (Cond::Any(&["git", "dep:git-object", "dep:git2"]), &[("development-tools", 1.1, 0.), ("parsing", 0.8, 0.)]),
        (Cond::Any(&["version-control", "vcs"]), &[("development-tools", 1.1, 0.1), ("parsing", 0.6, 0.)]),
        (Cond::Any(&["sourcecode", "commit", "generates"]), &[("development-tools", 1.1, 0.)]),
        (Cond::Any(&["installer", "packages"]), &[("development-tools", 1.1, 0.), ("os", 1.1, 0.)]),
        (Cond::Any(&["dep:unicode-xid"]), &[("development-tools", 1.1, 0.)]),

        (Cond::Any(&["filter", "download", "downloader"]), &[("command-line-utilities", 0.75, 0.), ("command-line-interface", 0.5, 0.), ("data-structures", 0.8, 0.)]),
        (Cond::Any(&["error"]), &[("command-line-utilities", 0.5, 0.), ("command-line-interface", 0.7, 0.), ("games", 0.7, 0.)]),
        (Cond::Any(&["serde", "avro", "apache-avro"]), &[("encoding", 1.3, 0.1), ("no-std", 0.8, 0.), ("command-line-utilities", 0.5, 0.), ("command-line-interface", 0.7, 0.), ("development-tools::cargo-plugins", 0.8, 0.)]),
        (Cond::Any(&["encoding", "encode", "encodes"]), &[("encoding", 1.3, 0.1), ("command-line-utilities", 0.5, 0.), ("command-line-interface", 0.7, 0.), ("development-tools::cargo-plugins", 0.8, 0.)]),
        (Cond::Any(&["binary", "byte"]), &[("encoding", 1.2, 0.05), ("parser-implementations", 1.1, 0.05)]),
        (Cond::Any(&["json", "base64", "toml", "semver", "punycode"]), &[
            ("encoding", 1.2, 0.1), ("parser-implementations", 1.2, 0.1), ("parsing", 0.2, 0.), ("web-programming::websocket", 0.5, 0.), ("rust-patterns", 0.8, 0.), ("multimedia::encoding", 0.1, 0.)]),
        (Cond::Any(&["hash", "hashing", "sodium"]), &[("algorithms", 1.2, 0.1), ("cryptography", 1.1, 0.1), ("cryptography::cryptocurrencies", 0.7, 0.), ("os::macos-apis", 0.8, 0.), ("no-std", 0.9, 0.), ("date-and-time", 0.5, 0.), ("memory-management", 0.7, 0.), ("development-tools", 0.7, 0.), ("command-line-utilities", 0.5, 0.)]),
        (Cond::All(&["hash", "function"]), &[("algorithms", 1.2, 0.1), ("cryptography", 1.1, 0.1)]),
        (Cond::Any(&["crc32", "fnv", "phf"]), &[("algorithms", 1.2, 0.1), ("cryptography", 0.4, 0.), ("os::macos-apis", 0.8, 0.), ("data-structures", 0.9, 0.)]),
        (Cond::Any(&["fetching"]), &[("encoding", 0.7, 0.), ("cryptography", 0.9, 0.)]),

        (Cond::Any(&["pickle", "serde"]), &[("encoding", 1.3, 0.1), ("embedded", 0.9, 0.), ("development-tools", 0.8, 0.),
            ("parsing", 0.9, 0.), ("parser-implementations", 1.2, 0.1), ("algorithms", 0.8, 0.)]),
        (Cond::Any(&["manager"]), &[("encoding", 0.6, 0.), ("parser-implementations", 0.4, 0.), ("data-structures", 0.8, 0.)]),

        (Cond::NotAny(&["crypto", "cryptography", "cryptographic", "schnorr", "pgp", "signature", "hash", "hashes", "hashing", "dep:digest", "digest", "nacl", "blake2", "libsodium", "zeroize", "merkle", "hmac",
            "constant-time", "constant_time", "ecc", "elliptic", "rsa", "ecdsa", "secp256k1", "ed25519", "curve25519", "argon2", "pbkdf2","block-cipher","cipher","signature",
             "random", "pki", "webpki", "bcrypt", "sha2", "aes", "nonce", "zero-knowledge", "entropy",
            "pem", "cert", "certificate", "certificates", "pki", "tls", "ssl", "cryptohash"]),
            &[("cryptography", 0.8, 0.)]),
        (Cond::Any(&["zero-knowledge", "elliptic-curve", "zk-snarks", "diffie-hellman", "curve25519", "zk-snark", "cryptohash"]),
            &[("cryptography", 1.3, 0.2), ("algorithms", 0.8, 0.), ("authentication", 1.1, 0.), ("no-std", 0.8, 0.), ("development-tools::cargo-plugins", 0.9, 0.), ("compilers", 0.8, 0.), ("accessibility", 0.5, 0.), ("filesystem", 0.8, 0.), ("command-line-utilities", 0.6, 0.)]),
        (Cond::Any(&["crypto", "nonce", "dep:digest", "post-quantum", "schnorr", "ecdsa","signature", "key-exchange", "entropy", "pem", "cert", "certificate", "certificates", "pki"]),
            &[("cryptography", 1.2, 0.2), ("cryptography::cryptocurrencies", 1.1, 0.), ("algorithms", 0.9, 0.), ("no-std", 0.9, 0.), ("development-tools::cargo-plugins", 0.9, 0.), ("filesystem", 0.8, 0.), ("command-line-utilities", 0.8, 0.)]),
        (Cond::Any(&["secure", "keyfile", "key", "encrypt"]), &[("cryptography", 1.2, 0.), ("cryptography::cryptocurrencies", 0.9, 0.), ("development-tools::ffi", 0.6, 0.)]),
        (Cond::All(&["elliptic", "curve"]), &[("cryptography", 1.4, 0.2)]),
        (Cond::Any(&["dep:subtle"]), &[("cryptography", 1.2, 0.2)]),

        (Cond::Any(&["command-line-tool", "coreutil", "uutils", "coreutils"]), &[("command-line-utilities", 1.2, 0.4), ("algorithms", 0.8, 0.), ("data-structures", 0.8, 0.), ("compilers", 0.8, 0.), ("no-std", 0.7, 0.)]),
        (Cond::Any(&["command-line-utility", "command-line-application"]), &[("command-line-utilities", 1.2, 0.4)]),
        (Cond::All(&["command", "line"]), &[("command-line-utilities", 1.15, 0.1), ("command-line-interface", 1.15, 0.)]),
        (Cond::Any(&["prompt"]), &[("command-line-utilities", 1.1, 0.), ("command-line-interface", 1.1, 0.)]),
        (Cond::All(&["commandline", "interface"]), &[("command-line-utilities", 1.15, 0.1), ("command-line-interface", 1.15, 0.)]),
        (Cond::Any(&["commandline", "command-line", "cmdline"]),
            &[("command-line-utilities", 1.1, 0.1), ("command-line-interface", 1.1, 0.), ("rust-patterns", 0.8, 0.), ("development-tools::ffi", 0.7, 0.)]),

        (Cond::Any(&["has:is_build", "has:is_dev"]), &[("os::windows-apis", 0.9, 0.), ("development-tools", 1.1, 0.),
            ("science", 0.8, 0.), ("science::math", 0.8, 0.), ("games", 0.8, 0.), ("value-formatting", 0.9, 0.)]),
        (Cond::Any(&["has:is_dev"]), &[("development-tools::profiling", 1.2, 0.), ("multimedia::audio", 0.7, 0.), ("rendering", 0.9, 0.), ("text-editors", 0.9, 0.), ("email", 0.9, 0.), ("rendering::graphics-api", 0.9, 0.), ("concurrency", 0.9, 0.)]),
        (Cond::Any(&["has:is_build"]), &[("concurrency", 0.9, 0.), ("rendering", 0.9, 0.), ("text-editors", 0.8, 0.), ("visualization", 0.9, 0.), ("simulation", 0.8, 0.), ("science::robotics", 0.8, 0.), ("multimedia::audio", 0.7, 0.), ("memory-management", 0.8, 0.)]),

        (Cond::Any(&["numeral", "numerals", "human-readable", "formatter", "notation", "pretty", "metric"]),
            &[("value-formatting", 1.2, 0.2), ("simulation", 0.5, 0.), ("science::robotics", 0.7, 0.), ("wasm", 0.7, 0.), ("no-std", 0.8, 0.)]),
        (Cond::Any(&["pretty-print", "pretty-printing", "punycode", "money", "timeago", "time-ago", "units", "weights", "uom"]),
            &[("value-formatting", 1.2, 0.2), ("simulation", 0.5, 0.), ("data-structures", 0.8, 0.), ("wasm", 0.7, 0.), ("no-std", 0.8, 0.)]),
        (Cond::All(&["human", "readable"]), &[("value-formatting", 1.2, 0.2), ("data-structures", 0.8, 0.)]),
        (Cond::All(&["human", "friendly"]), &[("value-formatting", 1.1, 0.)]),
        (Cond::Any(&["fpu", "simd", "comparison"]), &[("value-formatting", 0.5, 0.)]),
        (Cond::Any(&["math", "lint"]), &[("value-formatting", 0.9, 0.)]),
        (Cond::Any(&["morphological"]), &[("value-formatting", 1.1, 0.), ("text-processing", 1.1, 0.), ("internationalization", 1.1, 0.)]),

        (Cond::Any(&["roman", "phonenumber", "currency"]), &[("value-formatting", 1.2, 0.2), ("internationalization", 1.1, 0.), ("multimedia::encoding", 0.8, 0.), ("compilers", 0.5, 0.)]),
        (Cond::Any(&["numbers", "numeric", "value"]), &[("value-formatting", 1.2, 0.), ("science", 1.2, 0.), ("encoding", 1.1, 0.), ("parsing", 1.1, 0.), ("parser-implementations", 1.1, 0.)]),
        (Cond::Any(&["bytes", "byte", "metadata"]), &[("value-formatting", 0.8, 0.), ("development-tools::ffi", 0.9, 0.)]),
        (Cond::Any(&["log", "logging", "serde",  "nlp", "3d", "sdl2"]),
            &[("value-formatting", 0.7, 0.), ("database-implementations", 0.8, 0.), ("text-processing", 0.8, 0.),  ("no-std", 0.8, 0.), ("multimedia::images", 0.7, 0.),
            ("multimedia::audio", 0.7, 0.), ("multimedia::encoding", 0.8, 0.), ("cryptography::cryptocurrencies", 0.8, 0.)]),
        (Cond::Any(&["utils", "parser", "linear"]),
            &[("value-formatting", 0.7, 0.), ("database-implementations", 0.8, 0.), ("text-processing", 0.8, 0.), ("multimedia::images", 0.7, 0.),
            ("multimedia::audio", 0.7, 0.), ("multimedia::encoding", 0.8, 0.), ("cryptography::cryptocurrencies", 0.8, 0.), ("accessibility", 0.9, 0.)]),
        (Cond::Any(&["performance", "bitflags", "storage", "terminal", "rpc"]),
            &[("value-formatting", 0.25, 0.), ("development-tools", 0.8, 0.), ("network-programming", 0.7, 0.), ("science::math", 0.8, 0.)]),

        (Cond::NotAny(&["mailer", "pgp", "mime", "pop3", "ssmtp", "smtp", "imap", "email", "e-mail", "sendmail"]), &[("email", 0.7, 0.)]),
        (Cond::Any(&["mailer", "pop3", "ssmtp", "smtp", "sendmail", "imap", "email", "e-mail"]), &[
            ("email", 1.2, 0.3), ("network-programming", 0.9, 0.), ("parsing", 0.7, 0.), ("no-std", 0.7, 0.), ("algorithms", 0.8, 0.),
            ("development-tools::cargo-plugins", 0.6, 0.), ("filesystem", 0.7, 0.), ("accessibility", 0.7, 0.), ("data-structures", 0.8, 0.), ("command-line-utilities", 0.75, 0.)]),
        (Cond::Any(&["editor", "vim", "emacs", "vscode", "visual-studio", "sublime", "neovim"]), &[("text-editors", 1.2, 0.2), ("os::windows-apis", 0.7, 0.), ("compilers", 0.7, 0.), ("games", 0.4, 0.), ("rendering::engine", 0.7, 0.)]),
        (Cond::Any(&["obj", "loop", "lattice", "api", "bin", "framework", "stopwatch", "sensor", "github", "algorithm", "protocol"]),
            &[("games", 0.5, 0.), ("development-tools::profiling", 0.8, 0.), ("data-structures", 0.9, 0.)]),
        (Cond::Any(&["api", "bin", "framework", "hashmap", "trie", "protocol"]),
            &[("algorithms", 0.9, 0.), ("memory-management", 0.8, 0.)]),
        (Cond::All(&["text", "editor"]), &[("text-editors", 1.4, 0.4), ("caching", 0.8, 0.), ("text-processing", 0.8, 0.), ("parsing", 0.8, 0.), ("internationalization", 0.1, 0.)]),
        (Cond::All(&["language", "server"]), &[("text-editors", 1.2, 0.2), ("development-tools", 1.2, 0.1)]),
        (Cond::Any(&["repl", "pack"]), &[("parsing", 0.8, 0.)]),

        (Cond::Any(&["cli"]), &[("command-line-utilities", 1.1, 0.), ("command-line-interface", 1.1, 0.), ("rust-patterns", 0.6, 0.), ("os", 0.9, 0.), ("os::windows-apis", 0.7, 0.), ("os::macos-apis", 0.8, 0.)]),
        (Cond::Any(&["tui", "command-line-arguments", "cli-args", "arguments-parser", "argparser", "argparse"]), &[("command-line-interface", 1.1, 0.1), ("compilers", 0.9, 0.)]),
        (Cond::Any(&["dep:clap", "dep:docopt", "dep:structopt", "dep:ncurses", "dep:expect-exit", "dep:wild"]), &[("command-line-utilities", 1.15, 0.1), ("command-line-interface", 0.9, 0.)]),
        (Cond::All(&["curses", "interface"]), &[("command-line-interface", 1.1, 0.05)]),
        (Cond::All(&["terminal", "ui"]), &[("command-line-interface", 1.1, 0.), ("multimedia::encoding", 0.8, 0.)]),
        (Cond::Any(&["terminal", "ncurses", "tui", "curses", "ansi", "progressbar", "vt100", "ansi_term"]),
            &[("command-line-interface", 1.2, 0.1), ("multimedia::images", 0.1, 0.), ("multimedia", 0.4, 0.), ("rendering::engine", 0.7, 0.), ("no-std", 0.9, 0.), ("wasm", 0.9, 0.),
            ("science::math", 0.8, 0.), ("command-line-utilities", 0.75, 0.), ("internationalization", 0.9, 0.), ("algorithms", 0.8, 0.),
            ("development-tools::procedural-macro-helpers", 0.7, 0.), ("memory-management", 0.5, 0.), ("rust-patterns", 0.8, 0.),
            ("development-tools::cargo-plugins", 0.9, 0.), ("emulators", 0.6, 0.)]),
        (Cond::Any(&["term", "xterm", "console", "isatty", "readline", "repl", "getopts", "readline-implementation"]),
            &[("command-line-interface", 1.2, 0.1), ("multimedia::images", 0.1, 0.), ("multimedia", 0.4, 0.), ("no-std", 0.9, 0.), ("wasm", 0.9, 0.),
            ("science::math", 0.8, 0.), ("hardware-support", 0.7, 0.), ("command-line-utilities", 0.75, 0.), ("internationalization", 0.9, 0.),
            ("development-tools::procedural-macro-helpers", 0.7, 0.), ("memory-management", 0.5, 0.), ("rendering::engine", 0.8, 0.), ("emulators", 0.6, 0.)]),
        (Cond::Any(&["has:bin"]), &[("command-line-utilities", 1.1, 0.1), ("development-tools::cargo-plugins", 0.9, 0.), ("no-std", 0.7, 0.), ("game-development", 0.9, 0.),
            ("development-tools::procedural-macro-helpers", 0.7, 0.), ("memory-management", 0.4, 0.), ("os", 0.9, 0.), ("os::windows-apis", 0.8, 0.), ("algorithms", 0.5, 0.)]),
        (Cond::NotAny(&["has:bin"]), &[("games", 0.6, 0.), ("development-tools::cargo-plugins", 0.7, 0.), ("command-line-utilities", 0.2, 0.)]),

        (Cond::Any(&["hardware", "verilog", "bluetooth", "rs232","enclave", "eeprom", "adafruit", "laser", "altimeter", "sensor", "tp-link"]),
                &[("hardware-support", 1.2, 0.3), ("command-line-utilities", 0.7, 0.), ("multimedia::video", 0.7, 0.), ("multimedia::images", 0.6, 0.), ("multimedia::encoding", 0.8, 0.),
                ("os", 0.9, 0.), ("development-tools::testing", 0.8, 0.), ("development-tools::procedural-macro-helpers", 0.6, 0.), ("development-tools", 0.9, 0.)]),
        (Cond::Any(&["cpuid", "tpu", "acpi", "uefi", "bluez", "simd", "sgx", "raspberry"]),
                &[("hardware-support", 1.2, 0.3), ("command-line-utilities", 0.7, 0.), ("multimedia::images", 0.6, 0.), ("os", 0.9, 0.),
                 ("development-tools", 0.8, 0.), ("development-tools::testing", 0.8, 0.), ("parsing", 0.5, 0.), ("asynchronous", 0.6, 0.),
                 ("development-tools::procedural-macro-helpers", 0.6, 0.), ("development-tools", 0.9, 0.)]),
        (Cond::Any(&["sse3", "ssse3", "avx2", "avx512"]), &[("hardware-support", 1.2, 0.1)]),
        (Cond::Any(&["accelerometer", "magnetometer", "thermometer", "gyroscope"]), &[("hardware-support", 1.2, 0.1)]),
        (Cond::Any(&["mems"]), &[("embedded", 1.2, 0.1)]),
        (Cond::Any(&["firmware", "raspberrypi", "broadcom", "infineon", "rfcomm", "usb", "xhci", "apdu", "scsi", "hdd", "embedded-hal"]),
                &[("hardware-support", 1.2, 0.3), ("embedded", 1.1, 0.), ("no-std", 0.8, 0.), ("encoding", 0.9, 0.), ("command-line-utilities", 0.7, 0.),
                ("compression", 0.7, 0.), ("multimedia::images", 0.6, 0.), ("os", 0.85, 0.), ("development-tools", 0.8, 0.),
                ("development-tools::testing", 0.8, 0.), ("parsing", 0.6, 0.), ("development-tools::procedural-macro-helpers", 0.6, 0.),
                ("development-tools", 0.9, 0.)]),
        (Cond::Any(&["hal", "keyboard", "gamepad", "keypad", "joystick", "mouse", "enclave", "driver", "device", "device-drivers", "hardware-abstraction-layer", "embedded-hal-driver"]),
                &[("hardware-support", 1.2, 0.3), ("command-line-utilities", 0.8, 0.), ("multimedia::images", 0.5, 0.), ("compression", 0.8, 0.), ("science::robotics", 0.9, 0.), ("development-tools::testing", 0.8, 0.),
                ("development-tools::procedural-macro-helpers", 0.6, 0.), ("development-tools", 0.9, 0.), ("rendering::data-formats", 0.5, 0.)]),
        (Cond::Any(&["adafruit", "ardruino", "texas-instruments", "svd2rust"]), &[("hardware-support", 1.3, 0.1), ("embedded", 1.3, 0.1)]),
        (Cond::Any(&["dep:cortex-m-rt", "dep:atsamd-hal", "dep:bare-metal", "dep:ndless", "dep:embedded-ffi"]), &[("hardware-support", 1.2, 0.1), ("embedded", 1.2, 0.1)]),
        (Cond::All(&["hue", "light"]), &[("hardware-support", 1.2, 0.3), ("no-std", 0.9, 0.)]),
        (Cond::Any(&["libusb", "dep:libusb1-sys", "dep:usb-device", "nl80211"]), &[("hardware-support", 1.2, 0.1), ("compilers", 0.5, 0.)]),
        (Cond::All(&["controlling"]), &[("hardware-support", 1.1, 0.)]),
        (Cond::All(&["teledildonics"]), &[("hardware-support", 1.3, 0.2)]),
        (Cond::All(&["hue", "philips"]), &[("hardware-support", 1.2, 0.3), ("no-std", 0.9, 0.)]),
        (Cond::Any(&["camera", "vesa", "ddcci", "ddc"]), &[("hardware-support", 1.1, 0.2), ("multimedia::images", 1.2, 0.1), ("no-std", 0.9, 0.), ("parsing", 0.5, 0.)]),
        (Cond::Any(&["low-level"]), &[("hardware-support", 1.1, 0.), ("multimedia::encoding", 0.8, 0.)]),

        (Cond::Any(&["robotics", "roborio", "self-driving", "drone"]), &[("science::robotics", 1.2, 0.2), ("hardware-support", 1.1, 0.)]),
        (Cond::Any(&["aerospace"]), &[("science::robotics", 1.2, 0.2), ("science", 1.2, 0.1)]),
        (Cond::Any(&["autonomous", "autonomos", "robots", "robot", "robotic", "photogrammetry", "slam"]), &[("science::robotics", 1.1, 0.05), ("parsing", 0.5, 0.)]),
        (Cond::Any(&["kinematics", "motor", "kalman"]), &[("science::robotics", 1.1, 0.05), ("simulation", 1.1, 0.05), ("science", 1.1, 0.), ("compilers", 0.5, 0.)]),
        (Cond::Any(&["adafruit", "servomotor", "cnc", "stepper", "motor-controller"]), &[("science::robotics", 1.1, 0.), ("hardware-support", 1.1, 0.05), ("parsing", 0.5, 0.)]),

        (Cond::Any(&["robotstxt", "robots-txt", "crawler", "web-bot", "sitemap", "scraper"]),
            &[("web-programming", 1.2, 0.1), ("parsing", 0.5, 0.), ("science::robotics", 0.3, 0.), ("hardware-support", 0.8, 0.), ("accessibility", 0.8, 0.)]),
        (Cond::Any(&["spider", "url", "svg", "pgp", "gamepad", "interpreter", "ssl", "irc", "web", "robot36", "actor-framework"]), &[("science::robotics", 0.7, 0.), ("parsing", 0.8, 0.)]),

        (Cond::Any(&["microcontrollers", "avr", "nickel", "crt0", "bare-metal", "micropython", "6502", "sgx", "embedded", "embedded-hal-driver"]),
            &[("embedded", 1.3, 0.25), ("no-std", 0.9, 0.), ("wasm", 0.7, 0.), ("multimedia::encoding", 0.8, 0.), ("encoding", 0.9, 0.), ("multimedia::video", 0.8, 0.), ("compilers", 0.7, 0.), ("development-tools", 0.8, 0.), ("web-programming", 0.7, 0.)]),
        (Cond::All(&["metal", "bare"]), &[("embedded", 1.3, 0.2), ("os", 0.9, 0.), ("no-std", 0.9, 0.)]),
        (Cond::Any(&["iot"]), &[("embedded", 1.2, 0.1), ("network-programming", 1.1, 0.), ("hardware-support", 1.2, 0.)]),
        (Cond::All(&["pid", "control"]), &[("embedded", 1.2, 0.1), ("hardware-support", 1.2, 0.1), ("no-std", 0.9, 0.)]),
        (Cond::All(&["pid", "controler"]), &[("embedded", 1.2, 0.1), ("hardware-support", 1.2, 0.1)]),

        (Cond::Any(&["game", "json", "simulation", "turtle"]), &[("rendering::engine", 0.7, 0.), ("multimedia::encoding", 0.8, 0.), ("database", 0.9, 0.), ("internationalization", 0.8, 0.), ("asynchronous", 0.9, 0.)]),
        (Cond::Any(&["game", "games", "videogame", "videogames", "video-game", "video-games"]),
            &[("games", 1.25, 0.2), ("science::math", 0.6, 0.), ("wasm", 0.8, 0.), ("science::ml", 0.7, 0.), ("development-tools::cargo-plugins", 0.7, 0.),
            ("rendering::engine", 0.8, 0.), ("embedded", 0.75, 0.), ("filesystem", 0.5, 0.), ("web-programming::http-client", 0.5, 0.),
            ("internationalization", 0.7, 0.), ("multimedia::video", 0.8, 0.), ("date-and-time", 0.3, 0.), ("text-editors", 0.6, 0.), ("development-tools::procedural-macro-helpers", 0.6, 0.)]),
        (Cond::Any(&["rocket-league", "nintendo", "conway", "pokemon", "starcraft", "quake2", "quake3", "deathmatch", "speedrun", "roguelike", "roguelikes", "minecraft", "roblox", "sudoku"]),
            &[("games", 1.25, 0.3), ("rendering::engine", 0.8, 0.), ("wasm", 0.8, 0.), ("data-structures", 0.6, 0.), ("algorithms", 0.6, 0.), ("os::windows-apis", 0.6, 0.), ("simulation", 0.9, 0.),
            ("science::robotics", 0.9, 0.), ("internationalization", 0.8, 0.), ("compilers", 0.8, 0.),
            ("command-line-interface", 0.8, 0.), ("cryptography", 0.5, 0.), ("command-line-utilities", 0.75, 0.)]),
        (Cond::Any(&["fun", "quake", "doom", "play", "steam", "asteroids", "twitch", "shooter", "game-of-life"]), &[("games", 1.2, 0.1)]),

        (Cond::Any(&["vector", "openai", "client"]), &[("games", 0.7, 0.), ("emulators", 0.8, 0.), ("config", 0.8, 0.), ("development-tools::cargo-plugins", 0.7, 0.)]),
        (Cond::NotAny(&["gamedev", "game", "bevy", "godot", "dep:bevy", "games", "sdl", "ecs", "specs", "game-dev", "dep:allegro", "dice", "bounding", "library", "utils", "format", "polygon", "amethyst", "piston", "chess", "board", "ai"]),
            &[("game-development", 0.5, 0.)]),
        (Cond::NotAny(&["game", "games", "gamedev", "dep:bevy", "bevy", "game-dev", "tetris", "conway", "level", "dice", "roguelike", "tic-tac-toe", "board-game", "save", "fantasy", "rpg", "rts", "play", "fun", "xbox", "gamepad", "voxel", "puzzle", "toy", "cards", "sudoku", "puzzle", "bounding", "chess", "amethyst", "piston"]),
            &[("game-development", 0.8, 0.), ("games", 0.8, 0.)]),
        (Cond::All(&["imag"]), &[("game-development", 0.8, 0.)]),
        (Cond::Any(&["gamedev", "game-dev", "game-development"]), &[("game-development", 1.3, 0.2), ("games", 0.25, 0.), ("science", 0.5, 0.), ("concurrency", 0.75, 0.), ("compilers", 0.5, 0.), ("science::ml", 0.8, 0.), ("science::math", 0.9, 0.), ("parsing", 0.6, 0.), ("multimedia::video", 0.75, 0.)]),
        (Cond::All(&["game", "2d"]), &[("games", 1.1, 0.), ("game-development", 1.1, 0.)]),
        (Cond::All(&["game", "3d"]), &[("games", 1.1, 0.), ("game-development", 1.1, 0.)]),
        (Cond::All(&["game", "video"]), &[("multimedia::video", 0.5, 0.), ("multimedia", 0.8, 0.), ("parsing", 0.2, 0.), ("multimedia::encoding", 0.7, 0.)]),
        (Cond::All(&["game", "2048"]), &[("games", 1.2, 0.)]),
        (Cond::All(&["gamedev", "engine"]), &[("game-development", 1.5, 0.4), ("games", 0.1, 0.), ("multimedia::video", 0.5, 0.), ("concurrency", 0.5, 0.), ("development-tools::cargo-plugins", 0.7, 0.), ("rendering::data-formats", 0.8, 0.)]),
        (Cond::All(&["gamedev", "ecs"]), &[("game-development", 1.5, 0.4), ("games", 0.1, 0.), ("concurrency", 0.7, 0.)]),
        (Cond::All(&["games", "framework"]), &[("game-development", 1.3, 0.2), ("games", 0.6, 0.), ("gui", 0.8, 0.)]),
        (Cond::All(&["game", "library"]), &[("game-development", 1.3, 0.2), ("games", 0.6, 0.), ("gui", 0.8, 0.)]),
        (Cond::All(&["game", "ecs"]), &[("game-development", 1.2, 0.), ("games", 0.4, 0.)]),
        (Cond::All(&["game", "parser"]), &[("game-development", 1.1, 0.), ("rendering::engine", 0.1, 0.), ("games", 0.2, 0.), ("gui", 0.5, 0.)]),
        (Cond::All(&["chess", "engine"]), &[("game-development", 1.5, 0.3), ("rendering::engine", 0.4, 0.), ("games", 0.4, 0.)]),
        (Cond::All(&["game", "scripting"]), &[("game-development", 1.5, 0.3), ("rendering::engine", 0.4, 0.), ("games", 0.5, 0.), ("rendering::data-formats", 0.2, 0.)]),
        (Cond::All(&["game", "editor"]), &[("game-development", 1.3, 0.1), ("rendering::engine", 0.4, 0.), ("games", 0.8, 0.), ("rendering::engine", 0.2, 0.)]),
        (Cond::All(&["game", "graphics"]), &[("game-development", 1.3, 0.), ("games", 0.8, 0.), ("data-structures", 0.6, 0.)]),
        (Cond::All(&["game", "piston"]), &[("game-development", 1.2, 0.1), ("games", 1.19, 0.1)]),
        (Cond::Any(&["gamepad", "joystick"]), &[("game-development", 1.2, 0.), ("games", 1.2, 0.), ("hardware-support", 1.1, 0.1)]),
        (Cond::All(&["save", "files"]), &[("game-development", 1.1, 0.0), ("games", 1.1, 0.05)]),
        (Cond::Any(&["piston", "nintendo", "rolling-dice"]), &[("game-development", 1.1, 0.1), ("games", 1.1, 0.08)]),
        (Cond::Any(&["piston", "uuid", "scheduler", "countdown", "sleep"]), &[("development-tools::profiling", 0.2, 0.), ("algorithms", 0.8, 0.)]),
        (Cond::Any(&["timer"]), &[("development-tools::profiling", 0.8, 0.)]),
        (Cond::Any(&["mastercard", "master-card"]), &[("games", 0.5, 0.), ("filesystem", 0.5, 0.)]),
        (Cond::All(&["credit", "card"]), &[("games", 0.3, 0.), ("filesystem", 0.4, 0.)]),
        (Cond::All(&["visa", "card"]), &[("games", 0.5, 0.), ("parsing", 0.6, 0.), ("filesystem", 0.5, 0.)]),
        (Cond::All(&["card", "number"]), &[("games", 0.8, 0.), ("filesystem", 0.5, 0.)]),
        (Cond::All(&["validate", "numbers"]), &[("games", 0.3, 0.)]),
        (Cond::Any(&["engine", "godot", "amethyst"]), &[("game-development", 1.3, 0.1), ("games", 0.7, 0.)]),
        (Cond::Any(&["bevy", "dep:bevy"]), &[("game-development", 1.3, 0.3), ("games", 1.1, 0.08)]),
        (Cond::Any(&["game-engine", "game-engines", "ecs", "game-loop", "game-development"]), &[("game-development", 1.5, 0.2), ("rendering::engine", 0.8, 0.), ("games", 0.95, 0.), ("command-line-utilities", 0.75, 0.), ("rendering::data-formats", 0.2, 0.)]),
        (Cond::All(&["game", "engine"]), &[("game-development", 1.5, 0.3), ("games", 0.3, 0.), ("rendering::data-formats", 0.2, 0.), ("filesystem", 0.7, 0.), ("no-std", 0.8, 0.), ("filesystem", 0.8, 0.), ("command-line-interface", 0.8, 0.)]),
        (Cond::All(&["game", "development"]), &[("game-development", 1.3, 0.2), ("games", 0.3, 0.)]),
        (Cond::All(&["game", "dev"]), &[("game-development", 1.2, 0.), ("games", 0.9, 0.)]),
        (Cond::Any(&["games", "game"]), &[("gui", 0.7, 0.), ("development-tools", 0.7, 0.)]),
        (Cond::Any(&["xbox", "xbox360", "kinect"]), &[("games", 1.1, 0.), ("game-development", 1.1, 0.), ("parsing", 0.5, 0.)]),
        (Cond::All(&["rpg", "game"]), &[("games", 1.2, 0.2), ("game-development", 1.1, 0.)]),
        (Cond::All(&["rts", "game"]), &[("games", 1.2, 0.2), ("game-development", 1.1, 0.)]),
        (Cond::All(&["voxel", "game"]), &[("games", 1.2, 0.2), ("parsing", 0.6, 0.), ("game-development", 1.1, 0.), ("algorithms", 0.7, 0.)]),
        (Cond::Any(&["unreal-engine"]), &[("games", 1.2, 0.), ("game-development", 1.1, 0.)]),
        (Cond::All(&["unreal", "engine"]), &[("games", 1.2, 0.), ("game-development", 1.2, 0.1)]),
        (Cond::Any(&["boundingbox", "bounding-box", "aabb"]), &[("game-development", 1.1, 0.), ("rendering::engine", 0.8, 0.)]),
        (Cond::Any(&["texture", "fps", "gamepad"]), &[("game-development", 1.2, 0.1), ("parsing", 0.5, 0.), ("algorithms", 0.7, 0.)]),
        (Cond::All(&["rendering", "engine"]), &[("rendering::engine", 1.5, 0.3), ("rendering::data-formats", 0.2, 0.)]),
        (Cond::Any(&["storage", "gluster", "glusterfs"]), &[("filesystem", 1.2, 0.1), ("database", 1.2, 0.), ("game-development", 0.5, 0.), ("rendering::engine", 0.2, 0.), ("rendering::data-formats", 0.2, 0.)]),

        (Cond::Any(&["specs", "ecs", "http", "spider", "crawler"]), &[("command-line-utilities", 0.75, 0.), ("parsing", 0.9, 0.), ("algorithms", 0.8, 0.), ("multimedia::video", 0.8, 0.), ("compilers", 0.5, 0.)]),
        (Cond::Any(&["spider", "crawler"]), &[("web-programming::http-client", 1.2, 0.), ("web-programming::http-server", 0.75, 0.), ("parsing", 0.5, 0.), ("algorithms", 0.7, 0.)]),
        (Cond::Any(&["documentation"]), &[("rendering::data-formats", 0.2, 0.)]),

        (Cond::NotAny(&["file", "path", "file-system", "io", "fs", "ext4", "ext3", "directory", "directories", "dir", "fdisk", "folder", "basedir", "xdg", "gluster", "nfs", "samba", "disk", "xattr",
            "ionotify", "inode", "filesystem", "fuse", "temporary-files", "temp-files", "tempfile"]),
            &[("filesystem", 0.7, 0.)]),
        (Cond::Any(&["basedir", "xdg", "ext4", "ext3", "nfs", "samba", "disk", "temporary-files", "temp-files", "tempfile", "gluster"]),
             &[("filesystem", 1.25, 0.3), ("command-line-interface", 0.3, 0.), ("no-std", 0.5, 0.), ("parsing", 0.9, 0.), ("memory-management", 0.8, 0.),
             ("os", 0.95, 0.), ("gui", 0.9, 0.), ("science", 0.8, 0.), ("parsing", 0.6, 0.), ("science::math", 0.3, 0.), ("development-tools", 0.95, 0.), ("cryptography", 0.6, 0.),
             ("asynchronous", 0.8, 0.), ("algorithms", 0.7, 0.), ("development-tools::testing", 0.9, 0.), ("command-line-utilities", 0.75, 0.)]),
        (Cond::Any(&["backups", "backup",  "directories", "dir", "filesystem", "fdisk"]),
             &[("filesystem", 1.25, 0.2), ("command-line-interface", 0.4, 0.), ("no-std", 0.5, 0.), ("os", 0.95, 0.), ("gui", 0.9, 0.),
             ("science", 0.8, 0.), ("science::math", 0.5, 0.), ("development-tools", 0.95, 0.), ("cryptography", 0.6, 0.), ("asynchronous", 0.9, 0.),
             ("concurrency", 0.8, 0.), ("algorithms", 0.8, 0.), ("multimedia::video", 0.8, 0.), ("development-tools::testing", 0.9, 0.), ("command-line-utilities", 0.8, 0.)]),
        (Cond::Any(&["xattr", "ionotify", "inode", "temp-file", "file-system", "flock", "fuse"]),
             &[("filesystem", 1.25, 0.3), ("command-line-interface", 0.3, 0.), ("no-std", 0.5, 0.), ("os", 0.95, 0.), ("gui", 0.9, 0.),
             ("science", 0.8, 0.), ("science::math", 0.3, 0.), ("development-tools", 0.95, 0.), ("cryptography", 0.6, 0.), ("asynchronous", 0.8, 0.),
             ("concurrency", 0.8, 0.), ("algorithms", 0.8, 0.), ("multimedia::video", 0.6, 0.), ("encoding", 0.7, 0.), ("development-tools::testing", 0.9, 0.), ("command-line-utilities", 0.8, 0.)]),
        (Cond::Any(&["file", "vfs"]), &[("filesystem", 1.1, 0.), ("memory-management", 0.7, 0.), ("no-std", 0.8, 0.)]),
        (Cond::All(&["fuse", "filesystem"]), &[("filesystem", 1.3, 0.3), ("encoding", 0.3, 0.)]),
        (Cond::All(&["file", "path"]), &[("filesystem", 1.1, 0.)]),
        (Cond::All(&["file", "system"]), &[("filesystem", 1.2, 0.)]),
        (Cond::Any(&["path", "files", "vfs", "glob"]),
             &[("filesystem", 1.2, 0.2), ("concurrency", 0.8, 0.), ("command-line-interface", 0.7, 0.), ("no-std", 0.6, 0.), ("cryptography", 0.6, 0.), ("development-tools::testing", 0.9, 0.), ("command-line-utilities", 0.8, 0.)]),
        (Cond::All(&["disk", "image"]),
             &[("filesystem", 1.3, 0.1), ("os", 1.3, 0.1), ("multimedia::images", 0.01, 0.), ("compilers", 0.5, 0.)]),
        (Cond::All(&["file", "metadata"]),
             &[("filesystem", 1.2, 0.1), ("os", 1.1, 0.)]),

        (Cond::Any(&["consistent", "checksum", "passphrase"]), &[("algorithms", 1.15, 0.1), ("cryptography", 1.05, 0.)]),
        (Cond::Any(&["encryption", "e2e", "end-to-end", "e2ee", "keygen", "decryption", "password"]), &[("cryptography", 1.25, 0.2), ("compilers", 0.5, 0.)]),
        (Cond::Any(&["password", "passwords", "password-manager", "password-strength"]), &[("authentication", 1.25, 0.2)]),
        (Cond::Any(&["overhead", "byte", "zero-copy"]), &[("algorithms", 1.05, 0.), ("memory-management", 1.02, 0.), ("games", 0.8, 0.)]),
        (Cond::Any(&["buffer", "buffered", "ringbuffer", "clone-on-write"]), &[("algorithms", 1.25, 0.2), ("memory-management", 1.25, 0.), ("caching", 1.2, 0.), ("network-programming", 0.25, 0.)]),
        (Cond::NotAny(&["intern", "interning", "write-once", "lru", "proxy", "memoize", "memoization", "memcached", "cache", "caching", "memory-cache", "cached"]),
            &[("caching", 0.6, 0.)]),
        (Cond::Any(&["memcached", "cache", "caching", "memory-cache"]),
            &[("caching", 1.3, 0.2), ("memory-management", 1.1, 0.), ("data-structures", 0.7, 0.), ("date-and-time", 0.7, 0.),
             ("embedded", 0.9, 0.), ("cryptography", 0.6, 0.), ("encoding", 0.8, 0.), ("algorithms", 0.7, 0.)]),
        (Cond::Any(&["lru"]),
            &[("caching", 1.3, 0.2), ("memory-management", 1.1, 0.), ("date-and-time", 0.7, 0.), ("embedded", 0.9, 0.)]),
        (Cond::All(&["memory", "cache"]), &[("caching", 1.2, 0.1), ("memory-management", 1.1, 0.), ("os::windows-apis", 0.7, 0.)]),
        (Cond::All(&["lookup"]), &[("caching", 1.2, 0.)]),
        (Cond::Any(&["allocate", "allocates", "deallocate", "alloc", "mmap", "garbage-collector"]),
            &[("memory-management", 1.3, 0.2), ("caching", 0.8, 0.), ("encoding", 0.8, 0.), ("algorithms", 0.8, 0.), ("game-development", 0.7, 0.), ("development-tools", 0.8, 0.)]),
        (Cond::Any(&["allocator", "alloc", "slab", "memory-allocator"]),
            &[("memory-management", 1.3, 0.2), ("caching", 0.8, 0.), ("no-std", 0.8, 0.), ("database", 0.8, 0.), ("algorithms", 0.8, 0.), ("game-development", 0.7, 0.), ("development-tools", 0.8, 0.)]),
        (Cond::Any(&["garbage", "reclamation", "gc", "refcell", "garbage-collection"]), &[
            ("memory-management", 1.3, 0.2), ("data-structures", 0.9, 0.), ("authentication", 0.8, 0.),
            ("science::math", 0.7, 0.), ("rendering::graphics-api", 0.8, 0.), ("concurrency", 0.9, 0.),
            ("encoding", 0.7, 0.), ("development-tools::cargo-plugins", 0.8, 0.), ("development-tools::build-utils", 0.8, 0.), ("internationalization", 0.7, 0.)]),
        (Cond::Any(&["rc", "memory", "oom", "malloc", "heap"]), &[
            ("memory-management", 1.25, 0.1), ("data-structures", 0.8, 0.), ("authentication", 0.8, 0.),
            ("science::math", 0.8, 0.), ("os", 1.1, 0.), ("rendering::graphics-api", 0.8, 0.), ("concurrency", 0.8, 0.),
            ("encoding", 0.8, 0.), ("development-tools::cargo-plugins", 0.8, 0.), ("development-tools::build-utils", 0.8, 0.), ("internationalization", 0.7, 0.)]),
        (Cond::All(&["memory", "allocation"]), &[("memory-management", 1.25, 0.1), ("encoding", 0.8, 0.)]),
        (Cond::All(&["garbage", "collector"]), &[("memory-management", 1.25, 0.1), ("encoding", 0.8, 0.), ("science::math", 0.8, 0.), ("rendering::graphics-api", 0.8, 0.), ("concurrency", 0.9, 0.)]),
        (Cond::All(&["memory", "pool"]), &[("memory-management", 1.25, 0.1), ("encoding", 0.8, 0.), ("rendering::graphics-api", 0.8, 0.), ("os::windows-apis", 0.7, 0.), ("concurrency", 0.9, 0.)]),

        (Cond::All(&["vector", "clock"]), &[("games", 0.25, 0.), ("algorithms", 1.25, 0.1), ("date-and-time", 0.3, 0.), ("compilers", 0.5, 0.)]),
        (Cond::All(&["vector", "tree"]), &[("data-structures", 1.2, 0.1)]),
        (Cond::Any(&["vectorclock"]), &[("games", 0.25, 0.), ("algorithms", 1.5, 0.2), ("date-and-time", 0.3, 0.)]),
        (Cond::Any(&["phf"]), &[("date-and-time", 0.4, 0.)]),

        (Cond::Any(&["utility", "utilities", "ripgrep", "tools"]),
            &[("command-line-utilities", 1.1, 0.2), ("internationalization", 0.8, 0.), ("algorithms", 0.6, 0.), ("games", 0.01, 0.), ("filesystem", 0.8, 0.),
            ("rendering::engine", 0.6, 0.), ("science", 0.9, 0.), ("simulation", 0.75, 0.), ("os::windows-apis", 0.7, 0.), ("data-structures", 0.6, 0.)]),
        (Cond::All(&["cli", "utility"]),
            &[("command-line-utilities", 1.3, 0.3), ("command-line-interface", 0.3, 0.), ("data-structures", 0.6, 0.), ("rust-patterns", 0.7, 0.), ("algorithms", 0.8, 0.), ("games", 0.1, 0.), ("filesystem", 0.6, 0.), ("science", 0.8, 0.)]),
        (Cond::All(&["cli", "tool"]),
            &[("command-line-utilities", 1.3, 0.3), ("command-line-interface", 0.3, 0.), ("data-structures", 0.6, 0.), ("os::windows-apis", 0.7, 0.), ("rust-patterns", 0.7, 0.), ("filesystem", 0.8, 0.), ("science", 0.8, 0.)]),
        (Cond::All(&["bash", "tool"]), &[("command-line-utilities", 1.2, 0.2), ("os::windows-apis", 0.7, 0.)]),
        (Cond::All(&["commandline", "tool"]), &[("command-line-utilities", 1.2, 0.2)]),
        (Cond::All(&["cli", "command"]), &[("command-line-utilities", 1.1, 0.1)]),
        (Cond::Any(&["bash", "shell", "command", "tool", "util"]),
            &[("command-line-utilities", 1.05, 0.025), ("asynchronous", 0.9, 0.), ("rust-patterns", 0.9, 0.), ("os::windows-apis", 0.8, 0.)]),
        (Cond::Any(&["dep:colored", "dep:ansi_term"]), &[("command-line-utilities", 1.1, 0.), ("command-line-interface", 1.05, 0.)]),
        (Cond::Any(&["has:bin"]), &[("command-line-interface", 0.6, 0.), ("os::windows-apis", 0.7, 0.), ("data-structures", 0.5, 0.), ("algorithms", 0.7, 0.)]),
        (Cond::Any(&["dep:ansi_term", "dep:pager"]), &[("command-line-utilities", 1.1, 0.1)]),

        (Cond::NotAny(&["accessibility", "a11y", "atk", "screen-reader", "automation", "colorblind"]), &[("accessibility", 0.8, 0.)]),
        (Cond::Any(&["accessebility", "accessibility", "colorblind", "color-blind", "colorblindness"]), &[("accessibility", 1.4, 0.3)]),
        (Cond::Any(&["a11y", "screen-reader", "screenreader", "assistive-technology"]), &[("accessibility", 1.6, 0.4), ("compilers", 0.5, 0.)]),
        (Cond::All(&["gui", "accessibility"]), &[("accessibility", 1.1, 0.1)]),
        (Cond::All(&["ui", "accessibility"]), &[("accessibility", 1.1, 0.1)]),
        (Cond::All(&["screen", "reader"]), &[("accessibility", 1.2, 0.2)]),
        (Cond::Any(&["accessibile", "braille", "atk"]), &[("accessibility", 1.1, 0.1)]),
        (Cond::Any(&["aws", "accesscontextmanager", "accesscontext"]), &[("accessibility", 0.8, 0.)]),
        (Cond::Any(&["iam", "credentials", "rusoto", "kms", "utime", "privacy", "protection", "secrets"]), &[("accessibility", 0.7, 0.)]),
        (Cond::All(&["google", "drive"]), &[("accessibility", 0.8, 0.)]),
        (Cond::All(&["disk", "access"]), &[("accessibility", 0.8, 0.)]),
        (Cond::All(&["file", "access"]), &[("accessibility", 0.8, 0.)]),
        (Cond::All(&["linux", "mounts"]), &[("accessibility", 0.8, 0.)]),
        (Cond::All(&["cargo", "registry"]), &[("accessibility", 0.8, 0.)]),

        (Cond::NotAny(&["web", "html", "js", "javascript", "typescript", "deno", "json", "ua", "jwt", "github", "proxy", "apache", "pubsub", "rpc", "rest", "k8s", "containers", "kubernetes", "thrift", "serverless",
            "graphql", "lambda", "aws", "mime", "wordpress", "rss", "atom", "xml", "css", "xss", "rocket", "webhook", "conduit", "hyper", "nodejs", "asmjs", "browser",
            "front-end", "ipfs", "youtube", "google", "webrtc", "dep:jsonwebtoken", "jsonrpc", "streaming", "api-client", "json-rpc", "dep:cookie", "jsonapi", "http-api", "rest-api", "json-api", "webhook", "dep:rocket", "dep:actix-web", "website", "ct-logs"]),
            &[("web-programming", 0.8, 0.)]),
        (Cond::NotAny(&["web", "http", "http2", "webrtc", "api", "fetch", "http-client", "serverless", "graphql", "lambda", "s3", "aws", "api-client", "client", "rest", "thrift", "gotham", "hyper", "request", "json", "jsonrpc", "jsonapi", "rpc", "curl", "tls", "requests", "http-api", "json-api"]),
            &[("web-programming::http-client", 0.8, 0.)]),
        (Cond::NotAny(&["web", "http", "webrtc", "http2", "server", "router", "hyper", "actix", "actix-web", "apache", "conduit", "webserver", "serverless", "graphql", "aws", "lambda", "rpc", "iron",
            "tcp", "sse", "server-sent-events", "service", "middleware", "microservice", "proxy", "rest", "rest-api", "thrift", "webhook", "restful", "framework",
            "rocket", "dep:actix-web", "dep:tokio", "dep:async-std", "dep:warp", "dep:tide", "dep:rocket", "actix", "lucene", "elasticsearch"]),
            &[("web-programming::http-server", 0.8, 0.)]),
        (Cond::Any(&["hyper", "http-api", "json-api"]), &[("web-programming::http-client", 1.2, 0.), ("web-programming::http-server", 1.1, 0.), ("parsing", 0.5, 0.), ("multimedia::encoding", 0.8, 0.)]),
        (Cond::All(&["protocol", "web"]), &[("web-programming", 1.4, 0.1), ("parsing", 0.5, 0.), ("rust-patterns", 0.8, 0.), ("filesystem", 0.7, 0.), ("command-line-utilities", 0.75, 0.)]),
        (Cond::All(&["protocol", "implementation"]), &[("network-programming", 1.3, 0.1), ("web-programming", 1.1, 0.)]),
        (Cond::Any(&["flink"]), &[("network-programming", 1.2, 0.1), ("web-programming", 1.2, 0.1)]),
        (Cond::Any(&["webrtc", "rtmp"]), &[("network-programming", 1.2, 0.), ("web-programming", 1.3, 0.2), ("multimedia", 1.2, 0.2), ("multimedia::video", 1.1, 0.1)]),
        (Cond::Any(&["hls", "m3u8"]), &[("web-programming", 1.1, 0.1), ("multimedia", 1.1, 0.1), ("multimedia::video", 1.1, 0.1)]),
        (Cond::All(&["live", "streaming"]), &[("web-programming", 1.2, 0.1), ("multimedia", 1.2, 0.1), ("multimedia::video", 1.2, 0.1)]),
        (Cond::Any(&["rpc", "thrift", "fbthrift"]), &[("network-programming", 1.2, 0.05)]),
        (Cond::Any(&["rdf", "linked-data", "json-ld", "semantic-web"]), &[("web-programming", 1.2, 0.1), ("parser-implementations", 1.1, 0.), ("data-structures", 1.05, 0.)]),
        (Cond::Any(&["thrift", "fbthrift"]), &[("encoding", 1.2, 0.1)]),
        (Cond::All(&["streaming", "api"]), &[("network-programming", 1.2, 0.), ("web-programming", 1.2, 0.), ("algorithms", 1.2, 0.)]),
        (Cond::All(&["packet", "sniffing"]), &[("network-programming", 1.3, 0.1)]),
        (Cond::All(&["packet", "capture"]), &[("network-programming", 1.3, 0.1)]),
        (Cond::All(&["api", "dep:reqwest"]), &[("web-programming", 1.2, 0.05), ("compilers", 0.8, 0.), ("data-structures", 0.8, 0.), ("rust-patterns", 0.8, 0.)]),
        (Cond::All(&["wrapper", "dep:reqwest"]), &[("web-programming", 1.2, 0.05), ("compilers", 0.8, 0.), ("data-structures", 0.8, 0.), ("rust-patterns", 0.8, 0.)]),
        (Cond::All(&["sdk", "dep:reqwest"]), &[("web-programming", 1.2, 0.05), ("compilers", 0.8, 0.), ("data-structures", 0.8, 0.), ("rust-patterns", 0.8, 0.)]),
        (Cond::All(&["sdk", "dep:hyper"]), &[("web-programming", 1.2, 0.05), ("compilers", 0.8, 0.), ("data-structures", 0.8, 0.), ("rust-patterns", 0.8, 0.)]),
        (Cond::Any(&["jwt", "dep:jsonwebtoken", "api-client"]), &[("web-programming", 1.2, 0.05), ("compilers", 0.5, 0.), ("algorithms", 0.5, 0.), ("data-structures", 0.5, 0.)]),
        (Cond::Any(&["jwt", "dep:jsonwebtoken"]), &[("authentication", 1.1, 0.05)]),
        (Cond::Any(&["dep:js-sys"]), &[("web-programming", 1.15, 0.05), ("data-structures", 0.5, 0.), ("algorithms", 0.8, 0.)]),
        (Cond::Any(&["dep:wasm-bindgen"]), &[("web-programming", 1.1, 0.)]),
        (Cond::Any(&["minifier"]), &[("web-programming", 1.1, 0.), ("encoding", 1.1, 0.), ("no-std", 0.9, 0.)]),
        (Cond::All(&["web", "api"]), &[("web-programming", 1.3, 0.1), ("algorithms", 0.7, 0.), ("data-structures", 0.6, 0.)]),
        (Cond::All(&["web", "framework"]), &[("web-programming", 1.3, 0.1), ("algorithms", 0.7, 0.), ("data-structures", 0.6, 0.)]),
        (Cond::Any(&["web-framework"]), &[("web-programming", 1.2, 0.2), ("algorithms", 0.7, 0.), ("data-structures", 0.6, 0.)]),
        (Cond::All(&["cloud", "web"]), &[("web-programming", 1.4, 0.2), ("rust-patterns", 0.5, 0.), ("data-structures", 0.6, 0.), ("algorithms", 0.8, 0.), ("compilers", 0.8, 0.), ("parsing", 0.4, 0.), ("config", 0.8, 0.), ("filesystem", 0.7, 0.), ("no-std", 0.7, 0.), ("development-tools::build-utils", 0.8, 0.)]),
        (Cond::All(&["user", "agent"]), &[("web-programming", 1.1, 0.1), ("web-programming::http-client", 1.1, 0.1)]),
        (Cond::Any(&["user-agent", "useragent"]), &[("web-programming", 1.1, 0.1), ("web-programming::http-client", 1.1, 0.1), ("compilers", 0.5, 0.), ("data-structures", 0.8, 0.), ("algorithms", 0.8, 0.)]),
        (Cond::All(&["cloud", "provider"]), &[("web-programming", 1.3, 0.1), ("os::windows-apis", 0.7, 0.), ("compilers", 0.5, 0.)]),
        (Cond::All(&["web", "token"]), &[("web-programming", 1.2, 0.)]),
        (Cond::Any(&["webdev", "webhook", "blog", "webdriver", "web-scraping", "browsers", "browser", "cloud", "reqwest", "webhooks", "lucene", "elasticsearch", "web-api"]),
            &[("web-programming", 1.2, 0.1), ("embedded", 0.9, 0.), ("development-tools::cargo-plugins", 0.5, 0.), ("os::windows-apis", 0.7, 0.), ("emulators", 0.4, 0.), ("compilers", 0.9, 0.)]),

        (Cond::Any(&["csv", "writer"]), &[("encoding", 1.2, 0.1), ("command-line-interface", 0.3, 0.), ("data-structures", 0.9, 0.), ("command-line-utilities", 0.75, 0.)]),
        (Cond::Any(&["nodejs"]), &[("web-programming::http-server", 1.1, 0.), ("embedded", 0.5, 0.), ("hardware-support", 0.5, 0.), ("algorithms", 0.5, 0.), ("rust-patterns", 0.5, 0.)]),
        (Cond::Any(&["html"]), &[("web-programming", 1.11, 0.), ("template-engine", 1.12, 0.), ("text-processing", 1.1, 0.)]),
        (Cond::Any(&["mime"]), &[("web-programming", 1.2, 0.), ("email", 1.2, 0.), ("encoding", 0.8, 0.)]),
        (Cond::All(&["static", "site"]), &[("web-programming", 1.11, 0.2), ("template-engine", 1.12, 0.2), ("text-processing", 1.1, 0.1)]),
        (Cond::Any(&["ruby", "python", "pyo3", "lua", "gluon", "c", "cxx", "bytecode", "lisp", "java", "jvm", "jni"]),
            &[("development-tools", 1.2, 0.12), ("compilers", 1.2, 0.12), ("development-tools::ffi", 1.3, 0.05), ("science", 0.9, 0.), ("os", 0.9, 0.), ("parser-implementations", 0.9, 0.), ("command-line-interface", 0.3, 0.), ("command-line-utilities", 0.75, 0.)]),
        (Cond::Any(&["github"]),
            &[("development-tools", 1.2, 0.12), ("development-tools::ffi", 1.3, 0.05), ("science", 0.9, 0.), ("os", 0.9, 0.), ("parser-implementations", 0.9, 0.), ("command-line-interface", 0.3, 0.), ("command-line-utilities", 0.75, 0.)]),
        (Cond::All(&["lexical", "analysis"]), &[("compilers", 1.2, 0.12), ("parsing", 0.6, 0.)]),
        (Cond::All(&["scripting", "language"]), &[("compilers", 1.2, 0.12), ("parsing", 0.6, 0.), ("parser-implementations", 0.6, 0.)]),
        (Cond::All(&["programming", "language"]), &[("compilers", 1.2, 0.12), ("development-tools", 1.4, 0.3), ("development-tools::ffi", 1.2, 0.05)]),
        (Cond::Any(&["dep:codespan-reporting"]), &[("compilers", 1.1, 0.05)]),
        (Cond::Any(&["dep:gimli"]), &[("compilers", 1.1, 0.05), ("development-tools::debugging", 1.1, 0.05)]),
        (Cond::Any(&["dep:cranelift-codegen"]), &[("compilers", 1.2, 0.1)]),
        (Cond::Any(&["dep:schemars", "dep:reqwest", "dep:actix-web"]), &[("compilers", 0.7, 0.)]),

        (Cond::Any(&["dep:libgit2-sys", "dep:libgit2"]), &[("development-tools", 1.2, 0.1)]),
        (Cond::Any(&["runtime"]), &[("development-tools", 1.3, 0.1), ("development-tools::testing", 0.7, 0.), ("no-std", 0.8, 0.), ("encoding", 0.8, 0.), ("command-line-utilities", 0.7, 0.)]),
        (Cond::Any(&["pijul", "scripting", "rbenv", "pyenv", "pip", "lint", "linter"]), &[("development-tools", 1.2, 0.1), ("caching", 0.9, 0.), ("no-std", 0.8, 0.), ("embedded", 0.8, 0.)]),

        (Cond::Any(&["server", "server-sent", "micro-services", "rest", "webrtc", "microservices", "dep:actix-web", "dep:iron", "dep:gotham", "dep:roa", "dep:rocket"]),
            &[("web-programming::http-server", 1.2, 0.11), ("web-programming", 1.1, 0.), ("data-structures", 0.9, 0.), ("rust-patterns", 0.9, 0.), ("command-line-interface", 0.3, 0.),
            ("data-structures", 0.7, 0.),("command-line-utilities", 0.75, 0.), ("development-tools::cargo-plugins", 0.4, 0.)]),
        (Cond::Any(&["iron", "kafka", "actix-web", "wsgi", "openid", "conduit", "graphql", "restful", "http-server"]),
            &[("web-programming::http-server", 1.2, 0.11), ("web-programming", 1.1, 0.), ("rust-patterns", 0.8, 0.), ("algorithms", 0.8, 0.), ("no-std", 0.5, 0.), ("command-line-interface", 0.3, 0.),
            ("data-structures", 0.7, 0.),("command-line-utilities", 0.75, 0.), ("multimedia::video", 0.7, 0.), ("multimedia", 0.6, 0.), ("multimedia::encoding", 0.5, 0.), ("development-tools", 0.9, 0.), ("development-tools::cargo-plugins", 0.4, 0.), ("development-tools::build-utils", 0.5, 0.)]),
        (Cond::All(&["web", "routing"]), &[("web-programming::http-server", 1.2, 0.1), ("command-line-utilities", 0.75, 0.)]),
        (Cond::All(&["rest", "api"]), &[("web-programming::http-server", 1.2, 0.), ("web-programming::http-client", 1.2, 0.), ("web-programming", 1.1, 0.), ("network-programming", 0.9, 0.)]),
        (Cond::All(&["language", "server"]), &[("web-programming::http-server", 0.2, 0.), ("compilers", 0.7, 0.), ("development-tools", 1.2, 0.2), ("text-editors", 1.3, 0.)]),
        (Cond::Any(&["deno"]), &[("web-programming::http-server", 1.2, 0.1), ("development-tools", 1.1, 0.1)]),
        (Cond::All(&["lsp"]), &[("web-programming::http-server", 0.8, 0.), ("development-tools", 1.2, 0.)]),
        (Cond::All(&["web", "framework"]), &[("web-programming", 1.4, 0.2), ("web-programming::http-server", 1.2, 0.), ("command-line-utilities", 0.75, 0.)]),
        (Cond::Any(&["dep:tide", "dep:warp", "dep:actix-web", "middleware"]), &[("web-programming", 1.1, 0.), ("web-programming::http-server", 1.2, 0.1), ("command-line-utilities", 0.9, 0.)]),
        (Cond::Any(&["wamp", "nginx", "apache"]), &[("web-programming::http-server", 1.2, 0.1), ("web-programming::websocket", 0.9, 0.), ("filesystem", 0.7, 0.), ("command-line-utilities", 0.75, 0.)]),
        (Cond::Any(&["http", "dns", "dnssec", "grpc", "rpc", "json-rpc", "jsonrpc", "jsonapi", "json-api", "jwt", "statsd", "telemetry"]),
            &[("network-programming", 1.2, 0.), ("web-programming::websocket", 0.88, 0.), ("parsing", 0.7, 0.), ("encoding", 0.8, 0.),
            ("config", 0.9, 0.), ("rendering::data-formats", 0.5, 0.), ("asynchronous", 0.9, 0.),
            ("value-formatting", 0.8, 0.), ("data-structures", 0.7, 0.), ("rust-patterns", 0.8, 0.),
            ("command-line-utilities", 0.9, 0.), ("development-tools", 0.9, 0.), ("development-tools::testing", 0.9, 0.)]),
        (Cond::Any(&["backend", "server-sent"]), &[("web-programming::http-server", 1.2, 0.1), ("command-line-utilities", 0.8, 0.)]),
        (Cond::Any(&["client"]), &[("web-programming", 1.1, 0.), ("network-programming", 1.1, 0.), ("parsing", 0.7, 0.), ("web-programming::http-server", 0.9, 0.),
            ("development-tools::cargo-plugins", 0.8, 0.), ("value-formatting", 0.8, 0.)]),
        (Cond::Any(&["kubernetes", "terraform", "coreos"]), &[("web-programming", 1.1, 0.), ("web-programming::http-client", 0.9, 0.), ("network-programming", 1.2, 0.)]),
        (Cond::All(&["http", "client", "server"]), &[("web-programming", 1.2, 0.11)]),
        (Cond::All(&["http", "server"]), &[("web-programming::http-server", 1.2, 0.11)]),
        (Cond::All(&["http", "client"]), &[("web-programming::http-client", 1.2, 0.1), ("web-programming::http-server", 0.9, 0.), ("parsing", 0.8, 0.), ("algorithms", 0.8, 0.), ("data-structures", 0.8, 0.), ("rust-patterns", 0.8, 0.), ("development-tools::procedural-macro-helpers", 0.2, 0.)]),
        (Cond::All(&["rpc", "client"]), &[("web-programming::http-client", 1.2, 0.1), ("parsing", 0.8, 0.), ("algorithms", 0.8, 0.)]),
        (Cond::Any(&["firefox", "chromium"]), &[("web-programming", 1.2, 0.1), ("web-programming::http-server", 0.8, 0.)]),
        (Cond::Any(&["dep:http", "dep:mime"]), &[("web-programming", 1.1, 0.)]),
        (Cond::Any(&["dep:juniper"]), &[("web-programming", 1.1, 0.), ("web-programming::http-server", 1.1, 0.)]),
        (Cond::Any(&["dep:warp", "dep:finchers", "dep:tide"]), &[("web-programming", 1.1, 0.), ("web-programming::http-server", 1.2, 0.)]),
        (Cond::Any(&["http-client", "twitter", "vkontakte"]), &[("web-programming::http-client", 1.2, 0.1), ("web-programming::http-server", 0.8, 0.), ("no-std", 0.7, 0.),
            ("algorithms", 0.7, 0.), ("command-line-utilities", 0.75, 0.), ("development-tools::procedural-macro-helpers", 0.4, 0.), ("development-tools", 0.75, 0.)]),
        (Cond::All(&["cli", "cloud"]), &[("web-programming::http-client", 1.2, 0.1), ("command-line-utilities", 1.2, 0.2)]),
        (Cond::Any(&["javascript", "sass", "emscripten", "asmjs"]),
            &[("web-programming", 1.2, 0.2), ("gui", 0.9, 0.), ("embedded", 0.7, 0.), ("no-std", 0.7, 0.), ("os", 0.7, 0.), ("command-line-utilities", 0.75, 0.), ("development-tools::testing", 0.6, 0.)]),
        (Cond::Any(&["stdweb", "lodash", "css", "webvr"]),
            &[("web-programming", 1.2, 0.2), ("gui", 0.9, 0.), ("compilers", 0.6, 0.), ("embedded", 0.7, 0.), ("no-std", 0.7, 0.), ("os", 0.7, 0.), ("command-line-utilities", 0.75, 0.), ("development-tools::testing", 0.6, 0.)]),
        (Cond::Any(&["frontend", "slack", "url", "uri"]),
            &[("web-programming", 1.2, 0.2), ("gui", 0.9, 0.), ("embedded", 0.8, 0.), ("command-line-utilities", 0.75, 0.), ("development-tools::testing", 0.6, 0.)]),
        (Cond::Any(&["json"]), &[("web-programming", 1.1, 0.1), ("algorithms", 0.8, 0.), ("compilers", 0.7, 0.), ("no-std", 0.8, 0.), ("command-line-utilities", 0.75, 0.), ("text-processing", 0.8, 0.)]),
        (Cond::Any(&["protocol", "network", "socket", "sockets", "wifi", "wi-fi"]), &[
            ("network-programming", 1.2, 0.2), ("parsing", 0.6, 0.), ("algorithms", 0.8, 0.), ("compilers", 0.6, 0.), ("os::windows-apis", 0.9, 0.), ("rendering::data-formats", 0.4, 0.),
            ("command-line-utilities", 0.75, 0.), ("cryptography::cryptocurrencies", 0.8, 0.)]),
        (Cond::Any(&["protobuf", "netcdf", "protocol-buffers", "proto"]), &[("network-programming", 1.2, 0.2), ("compilers", 0.6, 0.), ("encoding", 1.2, 0.2), ("parsing", 0.4, 0.), ("command-line-utilities", 0.75, 0.)]),
        (Cond::Any(&["varint"]), &[("encoding", 1.1, 0.1)]),
        (Cond::All(&["protocol", "buffers"]), &[("network-programming", 1.2, 0.1), ("encoding", 1.2, 0.1), ("compilers", 0.6, 0.)]),
        (Cond::Any(&["p2p", "digitalocean"]), &[("network-programming", 1.4, 0.2), ("command-line-utilities", 0.75, 0.), ("compilers", 0.5, 0.), ("development-tools", 0.75, 0.), ("multimedia", 0.5, 0.)]),
        (Cond::All(&["ip", "address"]), &[("network-programming", 1.2, 0.1)]),
        (Cond::Any(&["ip", "dep:trust-dns-resolver"]), &[("network-programming", 1.15, 0.), ("compilers", 0.8, 0.), ("web-programming", 1.1, 0.)]),

        (Cond::All(&["graphics", "gpu"]), &[("rendering::graphics-api", 1.34, 0.1), ("parsing", 0.5, 0.), ("compilers", 0.9, 0.), ("no-std", 0.5, 0.), ("accessibility", 0.7, 0.), ("algorithms", 0.9, 0.)]),
        (Cond::Any(&["graphics", "wgpu"]), &[("rendering::graphics-api", 1.2, 0.1), ("rendering", 1.1, 0.), ("multimedia", 1.1, 0.)]),
        (Cond::Any(&["bezier"]), &[("rendering", 1.1, 0.1), ("multimedia::images", 1.1, 0.), ("compilers", 0.5, 0.)]),
        (Cond::All(&["surface", "gpu"]), &[("rendering::graphics-api", 1.34, 0.1), ("no-std", 0.5, 0.), ("parsing", 0.5, 0.)]),
        (Cond::Any(&["graphics", "2d", "canvas"]), &[("rendering::graphics-api", 1.1, 0.1), ("rendering", 1.1, 0.), ("multimedia", 1.1, 0.)]),
        (Cond::All(&["luminance"]), &[("rendering::graphics-api", 1.1, 0.), ("rendering", 1.1, 0.)]),
        (Cond::All(&["graphics", "bindings"]), &[("rendering::graphics-api", 1.34, 0.2), ("game-development", 0.9, 0.), ("accessibility", 0.7, 0.)]),
        (Cond::All(&["metal", "api"]), &[("rendering::graphics-api", 1.34, 0.2), ("game-development", 0.8, 0.)]),
        (Cond::Any(&["gfx-rs", "viewport", "sdf", "polygon", "polygons"]), &[("rendering::graphics-api", 1.15, 0.05), ("compilers", 0.6, 0.), ("games", 0.8, 0.)]),
        (Cond::Any(&["gfx-rs", "webgpu"]), &[("game-development", 0.9, 0.), ("algorithms", 0.8, 0.)]),
        (Cond::All(&["graphics", "sdk"]), &[("rendering::graphics-api", 1.2, 0.1)]),
        (Cond::All(&["vr", "sdk"]), &[("rendering::graphics-api", 1.3, 0.2), ("no-std", 0.5, 0.), ("algorithms", 0.8, 0.)]),
        (Cond::All(&["vr", "bindings"]), &[("rendering::graphics-api", 1.1, 0.1)]),
        (Cond::All(&["graphics", "api"]), &[("rendering::graphics-api", 1.3, 0.15), ("parsing", 0.5, 0.), ("compilers", 0.9, 0.), ("game-development", 0.9, 0.), ("games", 0.2, 0.)]),
        (Cond::All(&["input"]), &[("rendering::graphics-api", 0.8, 0.)]),
        (Cond::Any(&["direct2d", "glsl", "vulkan", "drawing-apis", "swsurface", "software-rendered-surface", "glium", "cairo", "freetype"]),
                &[("rendering::graphics-api", 1.3, 0.15), ("science::math", 0.8, 0.), ("no-std", 0.5, 0.), ("algorithms", 0.8, 0.), ("rendering", 1.1, 0.1), ("rendering::data-formats", 0.9, 0.),
                ("web-programming::websocket", 0.15, 0.), ("rendering::graphics-api", 1.1, 0.1),("rendering::engine", 1.05, 0.05), ("games", 0.8, 0.),
                ("hardware-support", 0.8, 0.), ("development-tools::testing", 0.6, 0.), ("no-std", 0.5, 0.), ("accessibility", 0.8, 0.), ("memory-management", 0.8, 0.)]),
        (Cond::Any(&["opengl", "opengl-es", "directwrite", "gl", "skia", "d3d12", "sdl2", "gltf", "spirv", "shader", "directx"]),
                &[("rendering::graphics-api", 1.3, 0.15), ("science::math", 0.8, 0.), ("no-std", 0.9, 0.), ("rust-patterns", 0.8, 0.), ("rendering", 1.1, 0.1), ("rendering::data-formats", 0.9, 0.),
                ("web-programming::websocket", 0.15, 0.), ("rendering::graphics-api", 1.1, 0.1),("rendering::engine", 1.05, 0.05),
                ("games", 0.8, 0.), ("hardware-support", 0.8, 0.), ("development-tools::testing", 0.6, 0.), ("memory-management", 0.8, 0.)]),
        (Cond::Any(&["render", "bresenham", "oculus", "opengl-based", "gfx", "ovr", "vr", "shader", "sprites", "nvidia", "ray", "renderer", "raytracing", "path-tracing", "ray-tracing", "ray-casting"]),
                &[("rendering", 1.2, 0.1), ("rendering::graphics-api", 1.2, 0.1), ("rendering::engine", 1.1, 0.05), ("database", 0.8, 0.), ("rendering::data-formats", 0.9, 0.), ("web-programming::websocket", 0.15, 0.),
                ("games", 0.8, 0.), ("development-tools::cargo-plugins", 0.9, 0.), ("hardware-support", 0.8, 0.), ("development-tools::testing", 0.6, 0.), ("development-tools::build-utils", 0.6, 0.)]),
        (Cond::Any(&["blender", "graphics", "image-processing"]), &[
            ("multimedia::images", 1.2, 0.05), ("rendering::graphics-api", 1.05, 0.), ("rendering", 1.1, 0.1), ("filesystem", 0.7, 0.),
            ("games", 0.8, 0.), ("simulation", 0.5, 0.), ("data-structures", 0.8, 0.)]),
        (Cond::Any(&["blender", "maya", "autocad", "3ds"]), &[("rendering", 1.2, 0.1)]),
        (Cond::Any(&["graphics", "image-processing"]), &[
            ("multimedia::images", 1.2, 0.1), ("rendering", 1.1, 0.1)]),

        (Cond::Any(&["gpgpu", "cudnn"]), &[("asynchronous", 0.2, 0.), ("concurrency", 0.8, 0.), ("accessibility", 0.8, 0.), ("no-std", 0.5, 0.), ("game-development", 0.8, 0.), ("parsing", 0.5, 0.), ("filesystem", 0.7, 0.),
            ("development-tools::testing", 0.8, 0.), ("development-tools::cargo-plugins", 0.7, 0.), ("development-tools::build-utils", 0.6, 0.)]),
        (Cond::Any(&["vk"]), &[("rendering::graphics-api", 1.1, 0.)]),
        (Cond::Any(&["dep:reqwest", "dep:rocket", "has:bin"]), &[("rendering::graphics-api", 0.8, 0.)]),
        (Cond::Any(&["validate", "windowing", "opencl"]), &[("games", 0.2, 0.), ("asynchronous", 0.2, 0.), ("concurrency", 0.8, 0.),
            ("development-tools", 0.9, 0.), ("development-tools::build-utils", 0.8, 0.)]),

        (Cond::Any(&["fontconfig", "stdout"]), &[("web-programming::websocket", 0.25, 0.)]),
        (Cond::Any(&["font", "ttf", "truetype", "opentype", "svg", "tesselation", "exporter", "mesh"]),
            &[("rendering::data-formats", 1.2, 0.1), ("gui", 0.7, 0.), ("no-std", 0.7, 0.), ("compilers", 0.7, 0.), ("parsing", 0.6, 0.), ("filesystem", 0.7, 0.), ("memory-management", 0.8, 0.),
            ("games", 0.5, 0.), ("internationalization", 0.7, 0.), ("web-programming::websocket", 0.25, 0.)]),
        (Cond::Any(&["loading", "loader", "algorithm", "gui", "git"]), &[("rendering::data-formats", 0.2, 0.), ("memory-management", 0.8, 0.)]),
        (Cond::Any(&["parsing", "game", "piston", "ascii"]), &[("rendering::data-formats", 0.7, 0.), ("data-structures", 0.9, 0.)]),
        (Cond::All(&["3d", "format"]), &[("rendering::data-formats", 1.3, 0.3), ("value-formatting", 0.5, 0.), ("parsing", 0.5, 0.), ("filesystem", 0.7, 0.), ("development-tools::ffi", 0.8, 0.)]),
        (Cond::Any(&["2d", "3d", "sprite"]), &[("rendering::graphics-api", 1.11, 0.), ("data-structures", 1.1, 0.), ("rendering::data-formats", 1.2, 0.), ("rendering", 1.1, 0.), ("games", 0.8, 0.), ("multimedia::audio", 0.8, 0.), ("rendering::graphics-api", 1.1, 0.)]),
        (Cond::NotAny(&["no-std", "no_std", "nostd", "hardware", "embedded"]), &[("no-std", 0.8, 0.)]),
        (Cond::Any(&["discord", "telegram", "twitch"]), &[("web-programming", 1.1, 0.1), ("no-std", 0.8, 0.), ("compilers", 0.8, 0.), ("accessibility", 0.8, 0.), ("asynchronous", 0.8, 0.), ("web-programming::websocket", 0.7, 0.)]),

    ].iter().copied().collect();
}

/// Based on the set of keywords, adjust relevance of given categories
///
/// Returns (weight, slug)
pub fn adjusted_relevance(mut candidates: HashMap<String, f64>, keywords: &HashSet<String>, min_category_match_threshold: f64, max_num_categories: usize) -> Vec<(f64, String)> {
    for (cond, actions) in KEYWORD_CATEGORIES.iter() {
        let matched_times = match cond {
            Cond::All(reqs) => {
                assert!(reqs.len() < 5);
                if reqs.iter().all(|&k| keywords.contains(k)) {1} else {0}
            },
            Cond::NotAny(reqs) => {
                if !reqs.iter().any(|&k| keywords.contains(k)) {1} else {0}
            },
            Cond::Any(reqs) => {
                reqs.iter().filter(|&&k| keywords.contains(k)).count()
            },
        };
        if matched_times > 0 {
            let match_relevance = (matched_times as f64).sqrt();
            for &(slug, mul, add) in actions.iter() {
                debug_assert!(CATEGORIES.from_slug(slug).1, "{}", slug);
                debug_assert!(mul >= 1.0 || add < 0.0000001, "{}", slug);
                let score = candidates.entry(slug.to_string()).or_insert(0.);
                *score *= mul.powf(match_relevance);
                *score += add * match_relevance + 0.000001;
            }
        }
    }

    if candidates.is_empty() {
        return Vec::new();
    }

    let best_candidate_before_change = candidates.iter()
        .max_by(|b,a| a.0.partial_cmp(&b.0).expect("nan"))
        .map(|(v,k)| (*k, v.clone()))
        .unwrap();

    if_this_then_not_that(&mut candidates, "cryptography::cryptocurrencies", "command-line-utilities");
    if_this_then_not_that(&mut candidates, "cryptography::cryptocurrencies", "algorithms");
    if_this_then_not_that(&mut candidates, "cryptography::cryptocurrencies", "wasm");
    if_this_then_not_that(&mut candidates, "encoding", "parsing");
    if_this_then_not_that(&mut candidates, "development-tools::procedural-macro-helpers", "development-tools");
    if_this_then_not_that(&mut candidates, "development-tools::debugging", "rust-patterns");
    if_this_then_not_that(&mut candidates, "parsing", "web-programming");
    if_this_then_not_that(&mut candidates, "concurrency", "development-tools");
    if_this_then_not_that(&mut candidates, "asynchronous", "development-tools");
    if_this_then_not_that(&mut candidates, "web-programming::websocket", "network-programming");
    if_this_then_not_that(&mut candidates, "value-formatting", "algorithms");
    if_this_then_not_that(&mut candidates, "value-formatting", "data-structures");

    relate_subcategory_candidates(&mut candidates, &CATEGORIES.root, 0.);

    either_or_category(&mut candidates, "internationalization", "text-processing");
    either_or_category(&mut candidates, "algorithms", "data-structures");
    either_or_category(&mut candidates, "algorithms", "rust-patterns");
    either_or_category(&mut candidates, "development-tools::cargo-plugins", "development-tools::build-utils");
    either_or_category(&mut candidates, "command-line-utilities", "development-tools::cargo-plugins");
    either_or_category(&mut candidates, "command-line-utilities", "command-line-interface");
    either_or_category(&mut candidates, "database", "database-implementations");
    either_or_category(&mut candidates, "development-tools", "wasm");
    either_or_category(&mut candidates, "development-tools::procedural-macro-helpers", "rust-patterns");
    either_or_category(&mut candidates, "embedded", "hardware-support");
    either_or_category(&mut candidates, "embedded", "no-std");
    either_or_category(&mut candidates, "hardware-support", "no-std");
    either_or_category(&mut candidates, "games", "game-development");
    either_or_category(&mut candidates, "games", "command-line-utilities");
    either_or_category(&mut candidates, "multimedia::encoding", "encoding");
    either_or_category(&mut candidates, "no-std", "rust-patterns");
    either_or_category(&mut candidates, "os::macos-apis", "os::unix-apis");
    either_or_category(&mut candidates, "parser-implementations", "encoding");
    either_or_category(&mut candidates, "parser-implementations", "parsing");
    either_or_category(&mut candidates, "science", "simulation");
    either_or_category(&mut candidates, "science::math", "algorithms");
    either_or_category(&mut candidates, "science::robotics", "embedded");
    either_or_category(&mut candidates, "science::robotics", "hardware-support");
    either_or_category(&mut candidates, "development-tools", "compilers");
    either_or_category(&mut candidates, "text-processing", "algorithms");
    either_or_category(&mut candidates, "text-processing", "template-engine");
    either_or_category(&mut candidates, "text-processing", "internationalization");
    either_or_category(&mut candidates, "text-processing", "text-editors");
    either_or_category(&mut candidates, "text-processing", "value-formatting");
    either_or_category(&mut candidates, "simulation", "emulators");
    either_or_category(&mut candidates, "embedded", "robotics");
    either_or_category(&mut candidates, "hardware-support", "robotics");
    either_or_category(&mut candidates, "simulation", "robotics");
    either_or_category(&mut candidates, "simulation", "algorithms");
    either_or_category(&mut candidates, "robotics", "algorithms");
    either_or_category(&mut candidates, "caching", "memory-management");
    either_or_category(&mut candidates, "config", "development-tools");
    either_or_category(&mut candidates, "rendering::engine", "game-development");
    either_or_category(&mut candidates, "internationalization", "value-formatting");
    either_or_category(&mut candidates, "concurrency", "asynchronous");
    either_or_category(&mut candidates, "filesystem", "os");
    either_or_category(&mut candidates, "filesystem", "os::unix-apis");
    either_or_category(&mut candidates, "web-programming", "network-programming");
    either_or_category(&mut candidates, "web-programming::http-server", "web-programming::http-client");
    // no-std is last resort, and cli is too generic
    if_this_then_not_that(&mut candidates, "parsing", "no-std");
    if_this_then_not_that(&mut candidates, "parser-implementations", "no-std");
    if_this_then_not_that(&mut candidates, "algorithms", "no-std");
    if_this_then_not_that(&mut candidates, "data-structures", "no-std");
    if_this_then_not_that(&mut candidates, "gui", "no-std");
    if_this_then_not_that(&mut candidates, "value-formatting", "no-std");
    if_this_then_not_that(&mut candidates, "internationalization", "no-std");
    if_this_then_not_that(&mut candidates, "encoding", "no-std");
    if_this_then_not_that(&mut candidates, "hardware-support", "no-std");
    if_this_then_not_that(&mut candidates, "embedded", "no-std");
    if_this_then_not_that(&mut candidates, "internationalization", "data-structures");
    if_this_then_not_that(&mut candidates, "email", "parsing");
    if_this_then_not_that(&mut candidates, "hardware-support", "rust-patterns");
    if_this_then_not_that(&mut candidates, "parser-implementations", "web-programming::http-server");
    if_this_then_not_that(&mut candidates, "parsing", "web-programming::http-server");
    if_this_then_not_that(&mut candidates, "embedded", "wasm");
    if_this_then_not_that(&mut candidates, "os", "filesystem");
    if_this_then_not_that(&mut candidates, "network-programming", "algorithms");
    if_this_then_not_that(&mut candidates, "network-programming", "data-structures");
    if_this_then_not_that(&mut candidates, "web-programming", "algorithms");
    if_this_then_not_that(&mut candidates, "databases", "algorithms");
    if_this_then_not_that(&mut candidates, "databases", "compilers");
    if_this_then_not_that(&mut candidates, "authentication", "algorithms");
    if_this_then_not_that(&mut candidates, "concurrency", "algorithms");
    if_this_then_not_that(&mut candidates, "compilers", "algorithms");
    if_this_then_not_that(&mut candidates, "concurrency", "data-structures");
    if_this_then_not_that(&mut candidates, "memory-management", "data-structures");
    if_this_then_not_that(&mut candidates, "web-programming", "data-structures");
    if_this_then_not_that(&mut candidates, "science::ml", "development-tools");
    if_this_then_not_that(&mut candidates, "development-tools", "command-line-utilities");
    if_this_then_not_that(&mut candidates, "development-tools", "algorithms");
    if_this_then_not_that(&mut candidates, "development-tools::debugging", "command-line-utilities");
    if_this_then_not_that(&mut candidates, "development-tools::testing", "command-line-utilities");
    if_this_then_not_that(&mut candidates, "development-tools::testing", "algorithms");
    if_this_then_not_that(&mut candidates, "development-tools::profiling", "command-line-utilities");
    if_this_then_not_that(&mut candidates, "development-tools::cargo-plugins", "command-line-utilities");
    if_this_then_not_that(&mut candidates, "database-implementations", "command-line-utilities");
    if_this_then_not_that(&mut candidates, "databases", "command-line-utilities");
    if_this_then_not_that(&mut candidates, "algorithms", "command-line-utilities");
    if_this_then_not_that(&mut candidates, "hardware-support", "algorithms");
    if_this_then_not_that(&mut candidates, "embedded", "algorithms");
    if_this_then_not_that(&mut candidates, "data-structures", "command-line-utilities");
    if_this_then_not_that(&mut candidates, "network-programming", "command-line-utilities");
    if_this_then_not_that(&mut candidates, "os", "command-line-utilities");
    if_this_then_not_that(&mut candidates, "os", "compilers");
    if_this_then_not_that(&mut candidates, "multimedia", "command-line-utilities");
    if_this_then_not_that(&mut candidates, "parsing", "command-line-utilities");
    if_this_then_not_that(&mut candidates, "parsing", "compilers");
    if_this_then_not_that(&mut candidates, "asynchronous", "compilers");
    if_this_then_not_that(&mut candidates, "web-programming::http-server", "compilers");
    if_this_then_not_that(&mut candidates, "parser-implementations", "command-line-utilities");
    if_this_then_not_that(&mut candidates, "parser-implementations", "compilers");
    if_this_then_not_that(&mut candidates, "game-development", "command-line-utilities");
    if_this_then_not_that(&mut candidates, "game-development", "compilers");
    if_this_then_not_that(&mut candidates, "encoding", "compilers");
    if_this_then_not_that(&mut candidates, "simulation", "command-line-utilities");
    if_this_then_not_that(&mut candidates, "config", "command-line-utilities");
    if_this_then_not_that(&mut candidates, "rust-patterns", "command-line-utilities");
    if_this_then_not_that(&mut candidates, "multimedia::video", "command-line-utilities");
    if_this_then_not_that(&mut candidates, "multimedia::images", "command-line-utilities");
    if_this_then_not_that(&mut candidates, "command-line-interface", "command-line-utilities");
    if_this_then_not_that(&mut candidates, "rendering::graphics-api", "accessibility");
    if_this_then_not_that(&mut candidates, "rust-patterns", "accessibility");
    if_this_then_not_that(&mut candidates, "web-programming::websocket", "accessibility");
    if_this_then_not_that(&mut candidates, "command-line-utilities", "accessibility");
    if_this_then_not_that(&mut candidates, "web-programming::http-client", "accessibility");
    if_this_then_not_that(&mut candidates, "web-programming::http-client", "compilers");
    if_this_then_not_that(&mut candidates, "config", "compilers");
    if_this_then_not_that(&mut candidates, "development-tools::procedural-macro-helpers", "accessibility");

    let best_candidate_after_change = candidates.iter()
        .max_by(|b,a| a.0.partial_cmp(&b.0).expect("nan"))
        .map(|(v,k)| (*k, v.clone()))
        .unwrap();

    if best_candidate_before_change.1 != best_candidate_after_change.1 {
        eprintln!("CHANGED §§§ {:?} -> {:?}, {:?}", best_candidate_before_change, best_candidate_after_change, candidates);
    }

    let max_score = candidates.iter()
        .map(|(_, v)| *v)
        .max_by(|a, b| a.partial_cmp(b).expect("nan"))
        .unwrap_or(0.);

    let min_category_match_threshold = min_category_match_threshold.max(max_score * 0.951);

    let mut res: Vec<_> = candidates.into_iter()
        .filter(|&(_, v)| v >= min_category_match_threshold)
        .filter(|&(ref k, _)| CATEGORIES.from_slug(k).1)
        .map(|(k, v)| (v, k))
        .collect();
    res.sort_unstable_by(|a, b| b.0.partial_cmp(&a.0).expect("nan"));
    res.truncate(max_num_categories);
    res
}

/// Keyword<>category matching works on each category independently, but the
/// categories are related.
/// propagate half of parent category's score to children,
/// and 1/6th of best child category to parent cat.
fn relate_subcategory_candidates(candidates: &mut HashMap<String, f64>, categories: &BTreeMap<String, crate::Category>, add_to_children: f64) -> f64 {
    let mut max = 0.;
    for cat in categories.values() {
        let propagage = match candidates.get_mut(&cat.slug) {
            Some(score) => {
                if *score > max {
                    max = *score;
                }
                *score = *score + score.min(add_to_children); // at most double the existing score to avoid creating false child categories
                *score / 2. // propagate half down
            },
            None => 0.,
        };
        let max_of_children = relate_subcategory_candidates(candidates, &cat.sub, propagage);
        if let Some(score) = candidates.get_mut(&cat.slug) {
            *score = *score + max_of_children / 6.;
        }
    }
    max
}

fn if_this_then_not_that(candidates: &mut HashMap<String, f64>, if_this: &str, not_that: &str) {
    if let Some(has) = candidates.get(if_this).copied() {
        if let Some(shouldnt) = candidates.get_mut(not_that) {
            *shouldnt -= has.min(*shouldnt / 3.);
        }
    }
}

/// Sometimes a crate half-fits in two categories, and this can push threshold to one of item.
/// (not for hierarchy of crates)
fn either_or_category(candidates: &mut HashMap<String, f64>, a_slug: &str, b_slug: &str) {
    if let (Some(a), Some(b)) = (candidates.get(a_slug).copied(), candidates.get(b_slug).copied()) {
        if a * 0.66 > b {
            *candidates.get_mut(a_slug).unwrap() += b/2.;
            *candidates.get_mut(b_slug).unwrap() *= 0.5;
        } else if b * 0.66 > a {
            *candidates.get_mut(b_slug).unwrap() += a/2.;
            *candidates.get_mut(a_slug).unwrap() *= 0.5;
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub(crate) enum Cond {
    Any(&'static [&'static str]),
    All(&'static [&'static str]),
    NotAny(&'static [&'static str]),
}
