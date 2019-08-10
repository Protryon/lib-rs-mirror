## What is lib.rs?

Lib.rs is a catalog of programs and libraries written in the [Rust programming language](https://www.rust-lang.org). It has all $CRATE_NUM crates (minus spam) from the [crates.io](https://crates.io) registry.

## Why use lib.rs?

 * lib.rs is _fast_. There's no JavaScript anywhere.

 * It has more complete and accurate crate information than crates.io:

   * Finds missing READMEs and pulls in documentation from `src/lib.rs`
   * Automatically categorizes crates and adds missing keywords, to improve browsing by categories and keywords.
   * Accurately shows which dependencies are out of date or deprecated.
   * Shows size of each crate and its dependencies.
   * Highlights which crates require nightly compiler or use non-Rust code.
   * Automatically finds and credits co-authors based on git history.

 * It has an advanced ranking algorithm which promotes stable, regularly updated, popuplar crates, and hides spam and abandoned crates.

 * It has short URLs to open a crate page `lib.rs/crate-name` and search `lib.rs?keyword`.

 * Shows similar/related crates on each crate page, so you can discover alternatives.

 * Has a dark theme (it's automatic — requires Firefox or Safari, and the OS set to dark).

## How to install the crates?

### Library crates

Run once:

```sh
cargo install cargo-edit
```

and then to add a library to your project, run:

```sh
cargo add library-name-here
```

### Application crates

Install [Rust via Rustup](https://www.rust-lang.org/tools/install). It's important to use `rustup` — Rust bundled with Linux distros (like Debian) is outdated and generally won't work. If you have installed Rust already, run `rustup update`.

To install an app, run:

```sh
cargo install -f app-name-here
```

This will install it in `~/.cargo/bin` and make it available from the command line.
