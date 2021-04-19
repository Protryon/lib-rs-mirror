use crate_db::builddb::Compat;
use regex::Regex;
use serde_derive::*;
use std::collections::HashSet;

pub const DIVIDER: &str = "---XBdt8MQTMWYwcSsH---";


#[derive(Deserialize)]
pub struct CompilerMessageInner {
    level: String,
    message: Option<String>,
}

#[derive(Deserialize)]
pub struct CompilerMessageTarget {
    #[serde(default)]
    // kind: Vec<String>,
    edition: Option<String>,
}

#[derive(Deserialize)]
pub struct CompilerMessage {
    target: Option<CompilerMessageTarget>,
    message: Option<CompilerMessageInner>,
    reason: Option<String>,
    package_id: String,
    #[serde(default)]
    filenames: Vec<String>,
}

#[derive(Default, Debug)]
pub struct Findings {
    pub crates: HashSet<(Option<&'static str>, String, String, Compat)>,
    pub rustc_version: Option<String>,
    pub check_time: Option<f32>,
}

pub fn parse_analyses(stdout: &str, stderr: &str) -> Vec<Findings> {
    let divider = format!("{}\n", DIVIDER);

    stdout.split(&divider).zip(stderr.split(&divider))
        .filter_map(|(out, err)| parse_analysis(out, err)).collect()
}

fn parse_package_id(id: &str) -> Option<(String, String)> {
    let mut parts = id.splitn(3, " ");
    let name = parts.next()?.to_owned();
    let ver = parts.next()?.to_owned();
    let rest = parts.next()?;
    if !rest.starts_with('(') {
        return None;
    }
    Some((name, ver))
}

fn parse_analysis(stdout: &str, stderr: &str) -> Option<Findings> {
    let stdout = stdout.trim();
    if stdout == "" {
        return None;
    }

    let mut findings = Findings::default();
    let user_time = Regex::new(r"^user\s+(\d+)m(\d+\.\d+)s$").expect("regex");

    let mut lines = stdout.split('\n');
    let first_line = lines.next()?;
    let mut fl = first_line.split(' ');
    if fl.next().unwrap() != "CHECKING" {
        eprintln!("----------\nBad first line {}", first_line);
        return None;
    }
    findings.rustc_version = Some(fl.next()?.to_owned());

    for line in lines.filter(|l| l.starts_with('{')) {
        let line = line
            .trim_start_matches("unknown line ")
            .trim_start_matches("failure-note ")
            .trim_start_matches("compiler-message ");

        if let Ok(msg) = serde_json::from_str::<CompilerMessage>(line) {
            if let Some((name, ver)) = parse_package_id(&msg.package_id) {
                if name == "______" || name == "_____" || name == "build-script-build" {
                    continue;
                }
                let level = msg.message.as_ref().map(|m| m.level.as_str()).unwrap_or("");
                let reason = msg.reason.as_deref().unwrap_or("");
                // not an achievement, ignore
                if msg.filenames.iter().any(|f| f.contains("/build-script-build")) {
                    continue;
                }

                let mut printed = HashSet::new();
                let desc = msg.message.as_ref().and_then(|m| m.message.as_deref());
                if let Some(desc) = desc {
                    if desc.starts_with("associated constants are experimental") {
                        findings.crates.insert((Some("1.19.0"), name.clone(), ver.clone(), Compat::Incompatible));
                    }
                    else if desc.starts_with("no method named `trim_start`") {
                        findings.crates.insert((Some("1.29.0"), name.clone(), ver.clone(), Compat::Incompatible));
                    }
                    else if desc.starts_with("use of unstable library feature 'split_ascii_whitespace") ||
                        desc.starts_with("unresolved import `core::convert::Infallible`") ||
                        desc.starts_with("cannot find type `NonZeroI") ||
                        desc.starts_with("use of unstable library feature 'try_from'") {
                        findings.crates.insert((Some("1.33.0"), name.clone(), ver.clone(), Compat::Incompatible));
                    }
                    else if desc.starts_with("cannot find trait `Unpin` in this scope") ||
                        desc.starts_with("use of unstable library feature 'transpose_result'") ||
                        desc.starts_with("use of unstable library feature 'pin'") {
                        findings.crates.insert((Some("1.32.0"), name.clone(), ver.clone(), Compat::Incompatible));
                    }
                    else if desc.starts_with("const fn is unstable") {
                        findings.crates.insert((Some("1.30.0"), name.clone(), ver.clone(), Compat::Incompatible));
                    }
                    else if desc.starts_with("use of unstable library feature 'int_to_from_bytes") ||
                        desc.starts_with("`core::mem::size_of` is not yet stable as a const fn") {
                        findings.crates.insert((Some("1.31.0"), name.clone(), ver.clone(), Compat::Incompatible));
                    }
                    else if desc.starts_with("unresolved import `std::ops::RangeBounds`") ||
                        desc.starts_with("unresolved import `std::alloc::Layout") {
                        findings.crates.insert((Some("1.27.0"), name.clone(), ver.clone(), Compat::Incompatible));
                    }
                    else if desc.starts_with("`dyn Trait` syntax is unstable") {
                        findings.crates.insert((Some("1.26.0"), name.clone(), ver.clone(), Compat::Incompatible));
                    }
                    else if desc.starts_with("128-bit type is unstable") ||
                        desc.starts_with("128-bit integers are not stable") ||
                        desc.starts_with("`..=` syntax in patterns is experimental") {
                        findings.crates.insert((Some("1.25.0"), name.clone(), ver.clone(), Compat::Incompatible));
                    }
                    else if desc.starts_with("unresolved import `std::ptr::NonNull`") {
                        findings.crates.insert((Some("1.24.0"), name.clone(), ver.clone(), Compat::Incompatible));
                    }
                    else if desc.starts_with("use of unstable library feature 'maybe_uninit'") ||
                        desc.starts_with("no function or associated item named `uninit` found for type `core::me") ||
                        desc.starts_with("no function or associated item named `uninit` found for type `std::me") ||
                        desc.starts_with("cannot find type `IoSliceMut`") ||
                        desc.starts_with("use of unstable library feature 'futures_api'") ||
                        desc.starts_with("unresolved import `std::io::IoSlice") {
                        findings.crates.insert((Some("1.35.0"), name.clone(), ver.clone(), Compat::Incompatible));
                    }
                    else if desc.starts_with("use of unstable library feature 'matches_macro'") ||
                        desc.starts_with("cannot find macro `matches!`") ||
                        desc.starts_with("no associated item named `MAX` found for type `u") {
                        findings.crates.insert((Some("1.41.0"), name.clone(), ver.clone(), Compat::Incompatible));
                    }
                    else if desc.starts_with("no associated item named `MAX` found for type `u") {
                        findings.crates.insert((Some("1.42.0"), name.clone(), ver.clone(), Compat::Incompatible));
                    }
                    else if desc.starts_with("the `#[track_caller]` attribute is an experimental") {
                        findings.crates.insert((Some("1.45.0"), name.clone(), ver.clone(), Compat::Incompatible));
                    }
                    else if desc.starts_with("use of unstable library feature 'ptr_cast") ||
                       desc.starts_with("use of unstable library feature 'duration_float") ||
                       desc.starts_with("unresolved import `core::any::type_name") ||
                       desc.starts_with("unresolved import `std::any::type_name") ||
                       desc.starts_with("no method named `cast` found for type `*") ||
                       desc.starts_with("use of unstable library feature 'euclidean_division") {
                        findings.crates.insert((Some("1.37.0"), name.clone(), ver.clone(), Compat::Incompatible));
                    }
                    else if desc.starts_with("use of unstable library feature 'option_flattening") ||
                        desc.starts_with("cannot find function `take` in module `mem") ||
                        desc.starts_with("subslice patterns are unstable") ||
                        desc.starts_with("`cfg(doctest)` is experimental and subject to change") ||
                        desc.starts_with("the `#[non_exhaustive]` attribute is an experimental") ||
                        desc.starts_with("syntax for subslices in slice patterns is not yet stabilized") ||
                        desc.starts_with("non exhaustive is an experimental feature") {
                        findings.crates.insert((Some("1.39.0"), name.clone(), ver.clone(), Compat::Incompatible));
                    }
                    else if desc.starts_with("cannot bind by-move into a pattern") ||
                        desc.starts_with("async/await is unstable") ||
                        desc.starts_with("async blocks are unstable") ||
                        desc.starts_with("async fn is unstable") {
                        findings.crates.insert((Some("1.38.0"), name.clone(), ver.clone(), Compat::Incompatible));
                    }
                    else if desc.starts_with("use of unstable library feature 'copy_within") ||
                       desc.starts_with("naming constants with `_` is unstable") ||
                       desc.starts_with("use of unstable library feature 'option_xor'") ||
                       desc.starts_with("enum variants on type aliases are experimental") {
                        findings.crates.insert((Some("1.36.0"), name.clone(), ver.clone(), Compat::Incompatible));
                    } else {
                        if printed.insert(desc) {
                            eprintln!("• err: {}", desc);
                        }
                    }
                }

                if msg.target.as_ref().and_then(|t| t.edition.as_ref()).map_or(false, |e| e == "2018") {
                    findings.crates.insert((Some("1.30.1"), name.clone(), ver.clone(), Compat::Incompatible));
                }
                if level == "error" {
                    // eprintln!("{}: {}", );
                    findings.crates.insert((None, name, ver, Compat::Incompatible));
                } else if reason == "compiler-artifact" {
                    findings.crates.insert((None, name, ver, Compat::VerifiedWorks));
                } else if level != "warning" && reason != "build-script-executed" && !(level == "" && reason == "compiler-message") {
                    eprintln!("unknown line {} {} {}", level, reason, line);
                }
            }
        } else {
            eprintln!("Does not parse as JSON: {}", line);
        }
    }
    for line in stderr.split('\n') {
        if let Some(c) = user_time.captures(line) {
            let m: u32 = c[1].parse().expect("time");
            let s: f32 = c[2].parse().expect("time");
            findings.check_time = Some((m * 60) as f32 + s);
        }
    }
    if findings.crates.is_empty() {
        return None;
    }
    Some(findings)
}

#[test]
fn parse_test() {
    let out = r##"

garbage
---XBdt8MQTMWYwcSsH---
CHECKING 1.37.0 wat ever

{"reason":"compiler-artifact","package_id":"proc_vector2d 1.0.2 (registry+https://github.com/rust-lang/crates.io-index)","target":{"kind":["proc-macro"],"crate_types":["proc-macro"],"name":"proc_vector2d","src_path":"/usr/local/cargo/registry/src/-18c1fa267ed022ff/proc_vector2d-1.0.2/src/lib.rs","edition":"2018","doctest":true},"profile":{"opt_level":"0","debuginfo":2,"debug_assertions":true,"overflow_checks":true,"test":false},"features":[],"filenames":["/tmp/cargo-target-dir/debug/deps/libproc_vector2d-a0e1c737778cdd0d.so"],"executable":null,"fresh":false}
{"reason":"compiler-artifact","package_id":"vector2d 2.2.0 (path+file:///crate)","target":{"kind":["lib"],"crate_types":["lib"],"name":"vector2d","src_path":"/crate/src/lib.rs","edition":"2018","doctest":true},"profile":{"opt_level":"0","debuginfo":2,"debug_assertions":true,"overflow_checks":true,"test":false},"features":[],"filenames":["/tmp/cargo-target-dir/debug/deps/libvector2d-f9ac6cbd40409fbe.rmeta"],"executable":null,"fresh":false}
---XBdt8MQTMWYwcSsH---
CHECKING 1.34.2 wat ever

{"reason":"compiler-artifact","package_id":"proc_vector2d 1.0.2 (registry+https://github.com/rust-lang/crates.io-index)","target":{"kind":["proc-macro"],"crate_types":["proc-macro"],"name":"proc_vector2d","src_path":"/usr/local/cargo/registry/src/-18c1fa267ed022ff/proc_vector2d-1.0.2/src/lib.rs","edition":"2018"},"profile":{"opt_level":"0","debuginfo":2,"debug_assertions":true,"overflow_checks":true,"test":false},"features":[],"filenames":["/tmp/cargo-target-dir/debug/deps/libproc_vector2d-9470d66afa730e34.so"],"executable":null,"fresh":false}
{"reason":"compiler-artifact","package_id":"vector2d 2.2.0 (path+file:///crate)","target":{"kind":["lib"],"crate_types":["lib"],"name":"vector2d","src_path":"/crate/src/lib.rs","edition":"2018"},"profile":{"opt_level":"0","debuginfo":2,"debug_assertions":true,"overflow_checks":true,"test":false},"features":[],"filenames":["/tmp/cargo-target-dir/debug/deps/libvector2d-59c2022ebc0120a6.rmeta"],"executable":null,"fresh":false}
---XBdt8MQTMWYwcSsH---
CHECKING 1.24.1 wat ever

{"message":{"children":[],"code":null,"level":"error","message":"function-like proc macros are currently unstable (see issue #38356)","rendered":"error: function-like proc macros are currently unstable (see issue #38356)\n --> /usr/local/cargo/registry/src/-18c1fa267ed022ff/proc_vector2d-1.0.2/src/lib.rs:4:1\n  |\n4 | #[proc_macro]\n  | ^^^^^^^^^^^^^\n\n","spans":[{"byte_end":68,"byte_start":55,"column_end":14,"column_start":1,"expansion":null,"file_name":"/usr/local/cargo/registry/src/-18c1fa267ed022ff/proc_vector2d-1.0.2/src/lib.rs","is_primary":true,"label":null,"line_end":4,"line_start":4,"suggested_replacement":null,"text":[{"highlight_end":14,"highlight_start":1,"text":"#[proc_macro]"}]}]},"package_id":"proc_vector2d 1.0.2 (registry+https://github.com/rust-lang/crates.io-index)","reason":"compiler-message","target":{"crate_types":["proc-macro"],"kind":["proc-macro"],"name":"proc_vector2d","src_path":"/usr/local/cargo/registry/src/-18c1fa267ed022ff/proc_vector2d-1.0.2/src/lib.rs"}}
{"message":{"children":[],"code":null,"level":"error","message":"function-like proc macros are currently unstable (see issue #38356)","rendered":"error: function-like proc macros are currently unstable (see issue #38356)\n  --> /usr/local/cargo/registry/src/-18c1fa267ed022ff/proc_vector2d-1.0.2/src/lib.rs:18:1\n   |\n18 | #[proc_macro]\n   | ^^^^^^^^^^^^^\n\n","spans":[{"byte_end":360,"byte_start":347,"column_end":14,"column_start":1,"expansion":null,"file_name":"/usr/local/cargo/registry/src/-18c1fa267ed022ff/proc_vector2d-1.0.2/src/lib.rs","is_primary":true,"label":null,"line_end":18,"line_start":18,"suggested_replacement":null,"text":[{"highlight_end":14,"highlight_start":1,"text":"#[proc_macro]"}]}]},"package_id":"proc_vector2d 1.0.2 (registry+https://github.com/rust-lang/crates.io-index)","reason":"compiler-message","target":{"crate_types":["proc-macro"],"kind":["proc-macro"],"name":"proc_vector2d","src_path":"/usr/local/cargo/registry/src/-18c1fa267ed022ff/proc_vector2d-1.0.2/src/lib.rs"}}
{"message":{"children":[],"code":null,"level":"error","message":"aborting due to 2 previous errors","rendered":"error: aborting due to 2 previous errors\n\n","spans":[]},"package_id":"proc_vector2d 1.0.2 (registry+https://github.com/rust-lang/crates.io-index)","reason":"compiler-message","target":{"crate_types":["proc-macro"],"kind":["proc-macro"],"name":"proc_vector2d","src_path":"/usr/local/cargo/registry/src/-18c1fa267ed022ff/proc_vector2d-1.0.2/src/lib.rs"}}
"##;

    let err = r##"WARNING: Your kernel does not support swap limit capabilities or the cgroup is not mounted. Memory limited without swap.
---XBdt8MQTMWYwcSsH---
+ rustup show
+ cargo check --locked --message-format=json
   Compiling proc_vector2d v1.0.2
    Checking vector2d v2.2.0 (/crate)
    Finished dev [unoptimized + debuginfo] target(s) in 1.39s

real    0m1.413s
user    0m0.880s
sys 0m0.376s
---XBdt8MQTMWYwcSsH---
+ rustup default 1.34.2
info: using existing install for '1.34.2-x86_64-unknown-linux-gnu'
info: default toolchain set to '1.34.2-x86_64-unknown-linux-gnu'
+ cargo check --locked --message-format=json
    Updating `/crate/.cargo/lts-repo-at-c2f8becb5afbc616061cd4e8fffd4a1b50931d3c` index
   Compiling proc_vector2d v1.0.2
    Checking vector2d v2.2.0 (/crate)
    Finished dev [unoptimized + debuginfo] target(s) in 1.63s

real    0m1.660s
user    0m1.060s
sys 0m0.412s
---XBdt8MQTMWYwcSsH---
+ rustup default 1.24.1
info: using existing install for '1.24.1-x86_64-unknown-linux-gnu'
info: default toolchain set to '1.24.1-x86_64-unknown-linux-gnu'
+ cargo check --locked --message-format=json
warning: unused manifest key: package.edition
   Compiling proc_vector2d v1.0.2
error: Could not compile `proc_vector2d`.

Caused by:
  process didn't exit successfully: `rustc --crate-name proc_vector2d /usr/local/cargo/registry/src/-18c1fa267ed022ff/proc_vector2d-1.0.2/src/lib.rs --error-format json --crate-type proc-macro --emit=dep-info,link -C prefer-dynamic -C debuginfo=2 -C metadata=991e439ea4bc3c99 -C extra-filename=-991e439ea4bc3c99 --out-dir /tmp/cargo-target-dir/debug/deps -L dependency=/tmp/cargo-target-dir/debug/deps --cap-lints allow` (exit code: 101)

real    0m0.978s
user    0m0.648s
sys 0m0.180s

exit failure
"##;

    let res = parse_analyses(out, err);
    assert!(res[0].crates.get(&(None, "vector2d".into(), "2.2.0".into(), Compat::VerifiedWorks)).is_some());
    assert!((res[0].check_time.unwrap() - 0.880) < 0.001);
    assert!(res[0].crates.get(&(Some("1.30.1"), "proc_vector2d".into(), "1.0.2".into(), Compat::Incompatible)).is_some());
    assert!(res[1].crates.get(&(None, "vector2d".into(), "2.2.0".into(), Compat::VerifiedWorks)).is_some());
    assert!(res[1].crates.get(&(Some("1.30.1"), "proc_vector2d".into(), "1.0.2".into(), Compat::Incompatible)).is_some());
    assert!(res[2].crates.get(&(None, "proc_vector2d".into(), "1.0.2".into(), Compat::Incompatible)).is_some());
}
