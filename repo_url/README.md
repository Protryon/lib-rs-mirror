# Understanding URLs to git repositories

This is a helper library to parse git URLs, with some special knowledge of GitHub and GitLab URLs (more URLs schemes welcome).

It's needed, because Cargo allows aribitrary URLs in the metadata, and people put all kinds of stuff in there. crates.rs needs to have canonical Git URLs and be able to query GitHub API about them.

