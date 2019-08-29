use std::collections::HashSet;
use regex::Regex;
use crate::db::Compat;
use serde_derive::*;

#[derive(Deserialize)]
pub struct CompilerMessageInner {
    level: String,
}

#[derive(Deserialize)]
pub struct CompilerMessage {
    message: Option<CompilerMessageInner>,
    reason: Option<String>,
    package_id: String,
}

#[derive(Default, Debug)]
pub struct Findings {
    pub crates: HashSet<(String, String, Compat)>,
    pub rustc_version: Option<String>,
    pub check_time: Option<f32>,
}

pub fn parse_analyses(stdout: &str, stderr: &str) -> Vec<Findings> {
    let stdout = stdout.replace("echo ----SNIP----", "");
    let stderr = stderr.replace("echo ----SNIP----", "");
    stdout.split("----SNIP----\n").zip(stderr.split("----SNIP----\n")).filter_map(|(out, err)| parse_analysis(out, err)).collect()
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
    let mut findings = Findings::default();
    let user_time = Regex::new(r"^user +(\d+)m(\d+\.\d+)s$").expect("regex");
    let compiler = Regex::new(r"(?:^|.*unchanged - )rustc (1\.\d+\.\d+) \([a-f0-9]+ 20").expect("regex");

    for line in stdout.split('\n') {
        if line.starts_with('{') {
            if let Ok(msg) = serde_json::from_str::<CompilerMessage>(line) {
                if let Some((name, ver)) = parse_package_id(&msg.package_id) {
                    let level = msg.message.as_ref().map(|m| m.level.as_str()).unwrap_or("");
                    let reason = msg.reason.as_ref().map(|s| s.as_str()).unwrap_or("");
                    if level == "error" {
                        findings.crates.insert((name, ver, Compat::Incompatible));
                    }
                    else if reason == "compiler-artifact" {
                        findings.crates.insert((name, ver, Compat::VerifiedWorks));
                    } else {
                        if level != "warning" && reason != "build-script-executed" && !(level == "" && reason == "compiler-message") {
                            eprintln!("unknown line {} {} {}", level, reason, line);
                        }
                    }
                }
            } else {
                eprintln!("Does not parse as JSON: {}", line);
            }
        } else if let Some(c) = compiler.captures(line) {
            findings.rustc_version = Some(c[1].to_owned());
        }
    }
    for line in stderr.split('\n') {
        if let Some(c) = user_time.captures(line) {
            let m: u32 = c[1].parse().expect("time");
            let s: f32 = c[2].parse().expect("time");
            findings.check_time = Some((m * 60) as f32 + s);
        }
    }
    Some(findings)
}

#[test]
fn parse_test() {
    let out = r##"----SNIP----
Default host: x86_64-unknown-linux-gnu

installed toolchains
--------------------

1.24.1-x86_64-unknown-linux-gnu
1.34.2-x86_64-unknown-linux-gnu
1.37.0-x86_64-unknown-linux-gnu

active toolchain
----------------

1.37.0-x86_64-unknown-linux-gnu (default)
rustc 1.37.0 (eae3437df 2019-08-13)

{"reason":"compiler-artifact","package_id":"proc_vector2d 1.0.2 (registry+https://github.com/rust-lang/crates.io-index)","target":{"kind":["proc-macro"],"crate_types":["proc-macro"],"name":"proc_vector2d","src_path":"/usr/local/cargo/registry/src/-18c1fa267ed022ff/proc_vector2d-1.0.2/src/lib.rs","edition":"2018","doctest":true},"profile":{"opt_level":"0","debuginfo":2,"debug_assertions":true,"overflow_checks":true,"test":false},"features":[],"filenames":["/tmp/cargo-target-dir/debug/deps/libproc_vector2d-a0e1c737778cdd0d.so"],"executable":null,"fresh":false}
{"reason":"compiler-artifact","package_id":"vector2d 2.2.0 (path+file:///crate)","target":{"kind":["lib"],"crate_types":["lib"],"name":"vector2d","src_path":"/crate/src/lib.rs","edition":"2018","doctest":true},"profile":{"opt_level":"0","debuginfo":2,"debug_assertions":true,"overflow_checks":true,"test":false},"features":[],"filenames":["/tmp/cargo-target-dir/debug/deps/libvector2d-f9ac6cbd40409fbe.rmeta"],"executable":null,"fresh":false}
----SNIP----

  1.34.2-x86_64-unknown-linux-gnu unchanged - rustc 1.34.2 (6c2484dc3 2019-05-13)

{"reason":"compiler-artifact","package_id":"proc_vector2d 1.0.2 (registry+https://github.com/rust-lang/crates.io-index)","target":{"kind":["proc-macro"],"crate_types":["proc-macro"],"name":"proc_vector2d","src_path":"/usr/local/cargo/registry/src/-18c1fa267ed022ff/proc_vector2d-1.0.2/src/lib.rs","edition":"2018"},"profile":{"opt_level":"0","debuginfo":2,"debug_assertions":true,"overflow_checks":true,"test":false},"features":[],"filenames":["/tmp/cargo-target-dir/debug/deps/libproc_vector2d-9470d66afa730e34.so"],"executable":null,"fresh":false}
{"reason":"compiler-artifact","package_id":"vector2d 2.2.0 (path+file:///crate)","target":{"kind":["lib"],"crate_types":["lib"],"name":"vector2d","src_path":"/crate/src/lib.rs","edition":"2018"},"profile":{"opt_level":"0","debuginfo":2,"debug_assertions":true,"overflow_checks":true,"test":false},"features":[],"filenames":["/tmp/cargo-target-dir/debug/deps/libvector2d-59c2022ebc0120a6.rmeta"],"executable":null,"fresh":false}
----SNIP----

  1.24.1-x86_64-unknown-linux-gnu unchanged - rustc 1.24.1 (d3ae9a9e0 2018-02-27)

{"message":{"children":[],"code":null,"level":"error","message":"function-like proc macros are currently unstable (see issue #38356)","rendered":"error: function-like proc macros are currently unstable (see issue #38356)\n --> /usr/local/cargo/registry/src/-18c1fa267ed022ff/proc_vector2d-1.0.2/src/lib.rs:4:1\n  |\n4 | #[proc_macro]\n  | ^^^^^^^^^^^^^\n\n","spans":[{"byte_end":68,"byte_start":55,"column_end":14,"column_start":1,"expansion":null,"file_name":"/usr/local/cargo/registry/src/-18c1fa267ed022ff/proc_vector2d-1.0.2/src/lib.rs","is_primary":true,"label":null,"line_end":4,"line_start":4,"suggested_replacement":null,"text":[{"highlight_end":14,"highlight_start":1,"text":"#[proc_macro]"}]}]},"package_id":"proc_vector2d 1.0.2 (registry+https://github.com/rust-lang/crates.io-index)","reason":"compiler-message","target":{"crate_types":["proc-macro"],"kind":["proc-macro"],"name":"proc_vector2d","src_path":"/usr/local/cargo/registry/src/-18c1fa267ed022ff/proc_vector2d-1.0.2/src/lib.rs"}}
{"message":{"children":[],"code":null,"level":"error","message":"function-like proc macros are currently unstable (see issue #38356)","rendered":"error: function-like proc macros are currently unstable (see issue #38356)\n  --> /usr/local/cargo/registry/src/-18c1fa267ed022ff/proc_vector2d-1.0.2/src/lib.rs:18:1\n   |\n18 | #[proc_macro]\n   | ^^^^^^^^^^^^^\n\n","spans":[{"byte_end":360,"byte_start":347,"column_end":14,"column_start":1,"expansion":null,"file_name":"/usr/local/cargo/registry/src/-18c1fa267ed022ff/proc_vector2d-1.0.2/src/lib.rs","is_primary":true,"label":null,"line_end":18,"line_start":18,"suggested_replacement":null,"text":[{"highlight_end":14,"highlight_start":1,"text":"#[proc_macro]"}]}]},"package_id":"proc_vector2d 1.0.2 (registry+https://github.com/rust-lang/crates.io-index)","reason":"compiler-message","target":{"crate_types":["proc-macro"],"kind":["proc-macro"],"name":"proc_vector2d","src_path":"/usr/local/cargo/registry/src/-18c1fa267ed022ff/proc_vector2d-1.0.2/src/lib.rs"}}
{"message":{"children":[],"code":null,"level":"error","message":"aborting due to 2 previous errors","rendered":"error: aborting due to 2 previous errors\n\n","spans":[]},"package_id":"proc_vector2d 1.0.2 (registry+https://github.com/rust-lang/crates.io-index)","reason":"compiler-message","target":{"crate_types":["proc-macro"],"kind":["proc-macro"],"name":"proc_vector2d","src_path":"/usr/local/cargo/registry/src/-18c1fa267ed022ff/proc_vector2d-1.0.2/src/lib.rs"}}
"##;

    let err = r##"WARNING: Your kernel does not support swap limit capabilities or the cgroup is not mounted. Memory limited without swap.
+ echo ----SNIP----
+ echo ----SNIP----
----SNIP----
+ rustup show
+ cargo check --locked --message-format=json
   Compiling proc_vector2d v1.0.2
    Checking vector2d v2.2.0 (/crate)
    Finished dev [unoptimized + debuginfo] target(s) in 1.39s

real    0m1.413s
user    0m0.880s
sys 0m0.376s
+ echo ----SNIP----
+ echo ----SNIP----
----SNIP----
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
+ echo ----SNIP----
+ echo ----SNIP----
----SNIP----
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
    panic!("{:#?}", res);
}
