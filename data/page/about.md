## What is lib.rs?

Lib.rs is a catalog of programs and libraries written in the [Rust programming language](https://www.rust-lang.org). It has $TOTAL_CRATE_NUM packages, including $CRATE_NUM (minus spam) crates from the [crates.io](https://crates.io) registry, and a few notable projects published only on GitHub or GitLab.

## Why use lib.rs?

 * lib.rs is _fast_. There's no JavaScript anywhere.
 * It has more complete and accurate crate information than crates.io:
   * Finds missing READMEs and pulls in documentation from `src/lib.rs`
   * Automatically categorizes crates and adds missing keywords, to improve browsing by categories and keywords.
   * Accurately shows which dependencies are out of date or deprecated.
   * Shows size of each crate and its dependencies.
   * Highlights which crates require nightly compiler or use non-Rust code.
   * Automatically finds and credits co-authors based on git history.
   * Detailed reverse dependencies page, including version fragmentation.
 * It has an advanced ranking algorithm which promotes stable, regularly updated, popular crates, and hides spam and abandoned crates.
 * It has short URLs to open a crate page `lib.rs/crate-name` and search `lib.rs?keyword`.
 * Shows similar/related crates on each crate page, which helps discovering better alternatives.
 * Has a dark theme (it's automatic — requires Firefox or Safari, and the OS set to dark).

## Ranking algorithm

Sorting of crates by their download numbers tends to favor old crates and incumbents, and makes it difficult for new, high-quality crates to gain users.

The algorithm has been designed based on research for [RFC 1824](https://github.com/rust-lang/rfcs/blob/master/text/1824-crates.io-default-ranking.md), feedback from Rust users, as well as inspired by Open Hub analysis, SourceRank, CocoaPods' QualityIndex, and npm search.

Crates are sorted by their overall quality score, which is a weighed combination of:

 * The crate's popularity measured by number of downloads, direct and indirect reverse dependencies. The numbers are filtered to reduce noise and corrected for biases that affect application crates and dependencies of dependencies.
 * The crate's usage trend — is it gaining or losing users.
 * Availability of the crate's documentation, examples, and length and quality of the README.
 * Stability estimated from release history, number of breaking versions, patch versions, and use of nightly features.
 * Presence of tests, CI, code comments.
 * Accuracy and completeness of the crate's metadata.
 * Number of authors and contributors.
 * Weight of the crate's unique dependencies (taking into account that some crates are very common and shared between projects).
 * Whether the crate is actively maintained or at least stable and done, based on release frequency, age, maintenance status, use of deprecated/outdated dependencies, non-0.x releases, etc.

The score is combined with relevance of crate's keywords to a given category or search query.

Overall, this algorithm is very good at discovering quality crates. If you find any cases where this algorithm gives wrong results, [please report them](https://forms.gle/SFntxLhGJB7xzFy19).

## Dependency freshness

Versions are considered out of date not merely when they're less than the latest, but when *more than half of users of that crate uses a newer version*. This way lib.rs only warns about crates that are really lagging behind, and doesn't complain about minor / irrelevant / experimental / unupgradable versions of dependencies.

Crate pages highlight out-of-date dependency versions:

* If there's no version number, the crate uses the latest, most popular version of the dependency.
* If there's a version number in black, the crate uses the latest version of the dependency, which is newer than the most popular version of the dependency.
* If there's a version number in orange, the crate uses slightly out-of-date version of the dependency.
* If there's a version number in bold dark red, the crate uses outdated or deprecated version of the dependency.


## Policies

lib.rs follows crates-io's policies and Rust's [Code of Conduct](https://www.rust-lang.org/policies/code-of-conduct).

