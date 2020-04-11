# Async client for GitHub API v3 (`application/vnd.github.v3+json`)

Written for [https://lib.rs](https://lib.rs). Supports only `get()` requests, because I didn't need more. [PR's welcome](https://gitlab.com/crates.rs/crates.rs/-/tree/master/github_v3).

* Uses `async`/`await` and `std::futures`.

* Supports streaming of GitHub's paged responses.

* Automatically waits for responses that GitHub processes asynchronously in the background.

* Automatically waits when hitting rate limit.

* It's tiny, around 200 lines of code.

It relies on [serde](https://lib.rs/serde) for parsing responses, so bring your own data model.

