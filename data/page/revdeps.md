## Reverse dependencies — legend

The [reverse dependencies page](/crates/libc/rev) shows a list of crates (dependers) that depend on a specific library crate (dependency).

"Active direct dependers over time" shows number of published crates that require any version of the library. Each column is a summary of changes during one month. A depender stops being counted as "active" one year after its last release. Top of the chart is a running total of active dependers. Green means it's gaining dependers, red means losing (or dependers becoming inactive). Below the totals is a small chart with more precise breakdown of the gains and losses.

"Number of dependers" chart shows number of published crates that end up requiring each version of the library. This takes into account version resolution according to semver ranges (the way Cargo would pick versions), so it will pick latest matching version of the library even if dependers didn't specify it explicitly (e.g. crates requring `"0.1.*"` may pick v0.1.23).

"Downloads/month" chart is based on download counts from crates.io. This includes downloads from projects that are not public, as well as noise from continuous integration (CI) servers and bot traffic. This data may be skewed towards slightly older verisons of the library, because it's sampled from up to a month in the past. Difference between these two charts is also caused by uneven popularity of the dependers.

"Depender - Version" table shows which version is required by the _latest stable version_ of each depender. Older versions of these crates may require older versions of the library.

