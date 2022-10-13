use ahash::HashMap;
use ahash::HashSetExt;
use crate::Page;
use crate::templates;
use crate::urler::Urler;
use futures::stream::StreamExt;
use futures::Stream;
use kitchen_sink::ArcRichCrateVersion;
use kitchen_sink::CError;
use kitchen_sink::CrateOwnerRow;
use kitchen_sink::CResult;
use kitchen_sink::Edition;
use kitchen_sink::KitchenSink;
use kitchen_sink::MaintenanceStatus;
use kitchen_sink::Origin;
use kitchen_sink::RichAuthor;
use kitchen_sink::RichCrate;
use kitchen_sink::SemVer;
use kitchen_sink::UserType;
use kitchen_sink::VersionReq;
use kitchen_sink::Warning;
use render_readme::Renderer;
use std::borrow::Cow;
use std::cell::RefCell;
use std::cmp::Reverse;
use ahash::HashSet;
use std::time::Duration;
use std::time::Instant;
use tokio::time::timeout_at;
use log::warn;

/// For `maintainer_dashboard.rs.html`
pub struct MaintainerDashboard<'a> {
    pub(crate) aut: &'a RichAuthor,
    pub(crate) markup: &'a Renderer,
    pub(crate) warnings: Vec<(f32, Vec<Origin>, Vec<StructuredWarning>)>,
    pub(crate) okay_crates: Vec<Origin>,
    pub(crate) printed_extended_desc: RefCell<HashSet<&'static str>>,
}

#[derive(Default)]
pub struct StructuredWarning {
    pub(crate) title: Cow<'static, str>,
    pub(crate) desc: Cow<'static, str>,
    pub(crate) extended_desc: Option<&'static str>,
    pub(crate) url: Option<(Cow<'static, str>, Cow<'static, str>)>,
    pub(crate) severity: u8,
}

impl<'a> MaintainerDashboard<'a> {
    pub async fn new(aut: &'a RichAuthor, mut rows: Vec<CrateOwnerRow>, kitchen_sink: &'a KitchenSink, urler: &Urler, markup: &'a Renderer, make_common_groups: bool) -> CResult<MaintainerDashboard<'a>> {
        rows.sort_unstable_by(|a, b| b.latest_release.cmp(&a.latest_release));
        rows.truncate(400);

        let deadline = Instant::now() + Duration::from_secs(12);
        let tmp: Vec<_> = Self::look_up(kitchen_sink, rows, deadline)
            .map(move |(origin, crate_ranking, res)| {
                elaborate_warnings(origin, crate_ranking, res, urler, kitchen_sink, deadline.min(Instant::now() + Duration::from_secs(8)))
            })
            .buffered(8)
            .collect().await;

        let mut okay_crates = Vec::new();
        let mut warnings: Vec<_> = tmp.into_iter()
            .filter_map(|mut res| if res.2.is_empty() {
                okay_crates.push(res.0);
                None
            } else {
                res.2.sort_unstable_by(|a, b| b.severity.cmp(&a.severity).then(a.title.cmp(&b.title)));
                Some((res.1, vec![res.0], res.2))
            }).collect();

        if !warnings.is_empty() && warnings.iter().all(|(_, _, w)| w.iter().all(|w| w.title == "Internal error")) {
            return Err(anyhow::anyhow!("Temporary error fetching crates. Please try again."));
        }

        if make_common_groups {
            Self::make_common_groups(&mut warnings);
        }

        warnings.iter_mut().for_each(|(rank, _, w)| {
            // this is probably just an abandoned crate
            if w.len() >= 10 {
                *rank /= w.len() as f32 / 5.;
            }
        });

        /// Rows were sorted by most recent first. Keep few most recent crates at the top, which are more relevant than most horribly deprecated crates full of (t)errors.
        fn sort_by_severity(warnings: &mut [(f32, Vec<Origin>, Vec<StructuredWarning>)]) {
            warnings.sort_unstable_by_key(|(crate_ranking, _, w)| {
                Reverse(((w.iter().map(|w| w.severity.pow(2) as u32).sum::<u32>() * 1000 + 5000) as f32 * crate_ranking) as u32)
            });
        }

        warnings.sort_unstable_by(|a, b| b.0.total_cmp(&a.0));
        let split_point = warnings.len()/8;
        let (top, rest) = warnings.split_at_mut(split_point);
        sort_by_severity(top);
        sort_by_severity(rest);

        Ok(Self {
            aut,
            markup,
            warnings,
            okay_crates,
            printed_extended_desc: RefCell::new(HashSet::new()),
        })
    }

    fn look_up(kitchen_sink: &KitchenSink, rows: Vec<CrateOwnerRow>, deadline: Instant) -> impl Stream<Item=(Origin, f32, CResult<(ArcRichCrateVersion, RichCrate, HashSet<Warning>)>)> + '_ {
        futures::stream::iter(rows.into_iter())
            .map(move |row| async move {
                let origin = row.origin;
                let crate_ranking = row.crate_ranking * if row.invited_by_github_id.is_some() { 0.8 } else { 1. }; // move it lower if this isn't the primary maintainer
                let cw = timeout_at(deadline.into(), async {
                    let c = kitchen_sink.rich_crate_version_async(&origin).await?;
                    let all = kitchen_sink.rich_crate_async(&origin).await?;
                    let w = validator::warnings_for_crate(kitchen_sink, &c, &all).await?;
                    Ok::<_, CError>((c, all, w))
                }).await.map_err(CError::from).and_then(|x| x);
                (origin, crate_ranking, cw)
            })
            .buffered(8)
    }

    pub fn is_org(&self) -> bool {
        self.aut.github.user_type == UserType::Org
    }

    pub fn should_print_extended_description(&self, desc: &'static str) -> bool {
        self.printed_extended_desc.borrow_mut().insert(desc)
    }

    pub fn login(&self) -> &str {
        &self.aut.github.login
    }

    pub fn page(&self) -> Page {
        Page {
            title: format!("@{}'s Rust crates", self.login()),
            critical_css_data: Some(include_str!("../../style/public/maintainer_dashboard.css")),
            critical_css_dev_url: Some("/maintainer_dashboard.css"),
            noindex: true,
            ..Default::default()
        }
    }

    pub(crate) fn atom_id_for(&self, origin: &Origin, warn: &StructuredWarning) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(warn.title.as_bytes());
        hasher.update(origin.short_crate_name().as_bytes());
        let hash = hasher.finalize().to_hex();
        format!("https://lib.rs/crates/{}?atom-{hash}", origin.short_crate_name())
    }

    pub fn now(&self) -> String {
        chrono::Utc::now().to_rfc3339()
    }

    pub fn render_maybe_markdown_str(&self, s: &str) -> templates::Html<String> {
        templates::Html(self.markup.markdown_str(s, true, None))
    }

    fn make_common_groups(warnings: &mut Vec<(f32, Vec<Origin>, Vec<StructuredWarning>)>) {
        let mut common = HashMap::default();
        for (.., warnings) in &*warnings {
            for w in warnings {
                common.entry((w.title.clone(), w.desc.clone())).or_insert((0, Vec::new(), None, 0f32)).0 += if warnings.len() == 1 { 2 } else { 1 };
            }
        }
        common.retain(|_, (num, _, _, _)| *num >= 3);
        if common.is_empty() || common.len() > warnings.len()/2 {
            return;
        }
        warnings.sort_unstable_by(|a, b| b.0.total_cmp(&a.0));
        warnings.retain_mut(|(rank, origins, warnings)| {
            warnings.retain_mut(|w| {
                let dest = match common.get_mut(&(w.title.clone(), w.desc.clone())) {
                    Some(d) => d,
                    None => return true,
                };
                dest.1.extend(origins.iter().cloned());
                dest.3 = dest.3.max(*rank);
                match &mut dest.2 {
                    None => {
                        dest.2 = Some(std::mem::take(w))
                    },
                    Some(old) => {
                        if old.extended_desc.is_none() && w.extended_desc.is_some() {
                            old.extended_desc = std::mem::take(&mut w.extended_desc);
                        }
                        if old.url.is_none() && w.url.is_some() {
                            old.url = std::mem::take(&mut w.url);
                        }
                        old.severity = old.severity.max(w.severity);
                    },
                };
                false
            });
            !warnings.is_empty()
        });

        warnings.reserve(common.len());
        for (_, origins, w, rank) in common.into_values() {
            if let Some(w) = w {
                warnings.push((rank, origins, vec![w]));
            }
        }
    }
}

async fn elaborate_warnings(origin: Origin, mut crate_ranking: f32, res: CResult<(ArcRichCrateVersion, RichCrate, HashSet<Warning>)>, urler: &Urler, kitchen_sink: &KitchenSink, deadline: Instant) -> (Origin, f32, Vec<StructuredWarning>) {
    let (k, _all, w) = match res {
        Ok(res) => res,
        Err(e) => return (origin, 0., vec![StructuredWarning {
            title: Cow::Borrowed("Internal error"),
            desc: Cow::Owned(format!("We couldn't check this crate at this time, because: {e}. Please try again later.")),
            url: None,
            extended_desc: None,
            severity: 0,
        }]),
    };
    if w.is_empty() || k.is_yanked() || k.maintenance() == MaintenanceStatus::Deprecated {
        (origin, crate_ranking, vec![])
    } else {
        // Ranking is used for sorting, so focus on maintenance here
        crate_ranking *= match k.maintenance() {
            MaintenanceStatus::Experimental => 2.,
            MaintenanceStatus::ActivelyDeveloped => 2.,
            MaintenanceStatus::None => 1.,
            MaintenanceStatus::PassivelyMaintained => 0.5,
            MaintenanceStatus::AsIs => 0.11,
            MaintenanceStatus::LookingForMaintainer => 0.1,
            MaintenanceStatus::Deprecated => 0.01,
        };
        let mut warnings = Vec::with_capacity(w.len());
        for w in w.into_iter().filter(|w| !matches!(w, Warning::BrokenLink(..))) { // FIXME: BrokenLink these are unreliable ;(
            let mut extended_desc = None;
            let (severity, title, desc, url) = match w {
                Warning::NoRepositoryProperty => (3, Cow::Borrowed("No repository property"), Cow::Borrowed("Specify git repository URL in Cargo.toml to help users find more information, contribute, and for lib.rs to read more info."), None::<(Cow<'static, str>, Cow<'static, str>)>),
                Warning::NoReadmeProperty => (if k.readme().is_some() {1} else {2}, "No readme property".into(), "Specify path to a README file for the project, so that information about is included in the crates.io tarball.".into(), None),
                Warning::NoReadmePackaged => (if k.readme().is_some() {1} else {3}, "README missing from crate tarball".into(), "Cargo sometimes fails to package the README file. Ensure the path to the README in Cargo.toml is valid, and points to a file inside the crate's directory.".into(), None),
                Warning::NoReadmeInRepo(url) => (if k.readme().is_some() {1} else {3}, "README missing from the repository".into(), format!("We've searched {url} and could not find a README file there.").into(), None),
                Warning::EscapingReadmePath(path) => (if k.readme().is_some() {1} else {3}, "Buggy README path".into(), format!("The non-local path to readme specified as '{path}' exposes a bug in Cargo. Please use a path inside the crate's directory. Symlinks are okay. Please verify the change doesn't break any repo-relative URLs in the README.").into(), None),
                Warning::ErrorCloning(url) => {
                    extended_desc = Some("At the moment we only support git, and attempt fetching when we index a new release. Cloning is necessary for lib.rs to gather data that is missing on crates.io, e.g. to correctly resolve relative URLs in README files, which depend on repository layout and non-standard URL schemes of repository hosts.");
                    (2, "Could not fetch repository".into(), format!("We've had trouble cloning git repo from {url}").into(), None)
                },
                Warning::BrokenLink(kind, url) => {
                    (1, format!("Broken link to {kind}").into(), format!("We did not get a successful HTTP response from {url} (these checks are cached, so the problem may have been temporary)").into(), None)
                },
                Warning::BadCategory(name) => {
                    extended_desc = Some("lib.rs has simplified and merged some of crates.io categories. Please file a bug if we got it wrong.");
                    (if k.category_slugs().is_empty() {2} else {1}, "Incorrect category".into(), format!("Crate's categories property in Cargo.toml contains '{name}', which isn't a category we recognize").into(), Some(("List of available categories".into(), "https://crates.io/category_slugs".into())))
                },
                Warning::NoCategories => {
                    extended_desc = Some("Even if there are no categories that fit precisely, pick one that is least bad. You can also propose new categories in crates.io issue tracker.");
                    if k.has_own_categories() {
                        (if k.category_slugs().is_empty() {2} else {1}, "Needs more categories".into(), format!("Please more specific categories that describe functionality of the crate. Expand categories = [{}] in your Cargo.toml.", comma_list(k.category_slugs().iter().map(|c| &**c).chain(k.manifest_raw_categories().iter().map(|c| &**c)))).into(), Some(("List of available categories".into(), "https://crates.io/category_slugs".into())))
                    } else {
                        (if k.category_slugs().is_empty() {3} else {2}, "Missing categories".into(), format!("Categories improve browsing of lib.rs and crates.io. Add categories = [{}] to the Cargo.toml.", comma_list(k.category_slugs().iter())).into(), Some(("List of available categories".into(), "https://crates.io/category_slugs".into())))
                    }
                },
                Warning::NoKeywords => (if k.keywords().is_empty() {3} else {2}, "Missing keywords".into(), format!("Help users find your crates. Add keywords = [{}] (up to 5) to the Cargo.toml. Best keywords are alternative terms or their spellings that aren't in the name or description. Also add a keyword that precisely categorizes this crate and groups it with other similar crates.", comma_list(k.keywords().iter())).into(), None),
                Warning::EditionMSRV(ed, msrv) => {
                    extended_desc = Some("Using the latest edition helps avoid old quirks of the compiler, and ensures Rust code has consistent syntax and behavior across all projects.");
                    (1, "Using outdated edition for no reason".into(), format!("We estimate that this crate requires at least Rust 1.{msrv}, which is newer than the last {}-edition compiler. You can upgrade without breaking any compatibility. Run cargo fix --edition and update edition=\"…\" in Cargo.toml.", ed as u16).into(),
                        Some(("The Edition Guide".into(), "https://doc.rust-lang.org/edition-guide/".into())))
                },
                Warning::BadMSRV(needs, says) => {
                    let tmp;
                    (1, "Needs to specify correct MSRV".into(), format!("We estimate that this crate requires at least Rust 1.{needs}{}. Add rust-version = \"1.{needs}\" to the Cargo.toml.", if says > 0 {tmp=format!(", but specified Rust 1.{says} as the minimum version"); &tmp} else {""}).into(),
                        Some((format!("{} versions", k.short_name()).into(), urler.all_versions(k.origin()).unwrap_or_else(|| urler.crate_by_origin(k.origin())).into())))
                },
                Warning::DocsRs => {
                    extended_desc = Some("Docs.rs doesn't need to run or even link any code, so system dependencies can simply be skipped. You can also set cfg flags just for docs.rs and use them to hide problematic code.");
                    (if k.is_sys() {1} else {2}, "docs.rs build failed".into(), "docs.rs site failed to build the crate, so users will have trouble finding the documentation. Docs.rs supports multiple platforms and custom configurations, so you can make the build work even if normal crate usage has special requirements.".into(), Some(("Detecting docs.rs".into(), "https://docs.rs/about/builds".into())))
                },
                Warning::DeprecatedDependency(name, req) => {
                    let origin = Origin::from_crates_io_name(&name);
                    (3, format!("Dependency {name} {req} is deprecated").into(), "Please remove the dependency or replace it with a different crate.".into(),
                        Some((format!("{name} crate").into(), urler.crate_by_origin(&origin).into())))
                },
                Warning::OutdatedDependency(name, req, severity) => {
                    let origin = Origin::from_crates_io_name(&name);
                    let mut upgrade_version = Cow::Borrowed("the latest version");
                    if let Some(latest) = latest_version_matching_requirement(&req, deadline, kitchen_sink, &origin).await {
                        upgrade_version = Cow::Owned(latest.to_string());
                    }
                    extended_desc = Some(if upgrade_version.starts_with("0.") {
                            "In Cargo, different 0.x versions are considered incompatible, so this is a semver-major upgrade."
                        } else {
                            "Easy way to bump dependencies: cargo install cargo-edit; cargo upgrade; Also check out Dependabot service on GitHub."
                        });
                    (1+severity/40, format!("Dependency {name} {req} is {}outdated", match severity {
                        0..=10 => "slightly ",
                        11..=30 => "a bit ",
                        31..=80 => "",
                        81..=255 => "significantly ",
                    }).into(), if severity > 40 && !k.is_app() {
                        format!("Upgrade to {upgrade_version} to get all the fixes, and avoid causing duplicate dependencies in projects.")
                    } else {
                        format!("Consider upgrading to {upgrade_version} to get all the fixes and improvements.")
                    }.into(),
                    Some((
                        format!("{name} versions").into(),
                        if severity > 40 { urler.reverse_deps(&origin) } else { urler.all_versions(&origin) }.unwrap_or_else(|| urler.crate_by_origin(&origin)).into(),
                    )))
                },
                Warning::BadSemVer(ver, err) => {
                    (2, format!("Syntax error in version {ver}").into(),
                    format!("This is not a valid semver: {err}. Cargo enforces semver syntax now, so this version is unusable. Please yank it with cargo yank --vers {ver}").into(), None)
                },
                Warning::BadRequirement(name, req) => {
                    extended_desc = Some("Cargo used to be more forgiving about the semver syntax, so it's possible that an already-published crate doesn't satisfy the current rules.");
                    (3, format!("Incorrect dependency requirement {name} = {req}").into(),
                    "We could not parse it. Please check the semver syntax.".into(),
                    Some(("Cargo dependencies manual".into(), "https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#specifying-dependencies-from-cratesio".into())))
                },
                Warning::ExactRequirement(name, req) => {
                    let origin = Origin::from_crates_io_name(&name);
                    (2, format!("Locked dependency version {name} {req}").into(), "This can easily cause a dependency resolution conflict. If you must work around a semver-breaking dependency that can't be yanked, use a range of versions or fork it.".into(),
                        Some((format!("{name} versions").into(), urler.all_versions(&origin).unwrap_or_else(|| urler.crate_by_origin(&origin)).into())))
                },
                Warning::LaxRequirement(name, req, is_breaking) => {
                    let origin = Origin::from_crates_io_name(&name);
                    extended_desc = Some(if is_breaking {
                        "This crate does not follow semver, and is known to add new features in \"patch\" releases!"
                    } else {
                        "If you want to keep using truly minimal dependency requirements, please make sure you test them in CI with -Z minimal-versions Cargo option, because it's very easy to accidentally use a feature added in a later version."
                    });

                    let mut upgrade_note = String::new();
                    if let Some(latest) = latest_version_matching_requirement(&req, deadline, kitchen_sink, &origin).await {
                        upgrade_note = format!("Specify the version as {name} = \"{latest}\". ");
                    }

                    (if is_breaking {2} else {1}, format!("Imprecise dependency requirement {name} = {req}").into(),
                        format!("Cargo does not always pick latest versions of dependencies. {upgrade_note}Too-low version requirements can cause breakage, especially when combined with minimal-versions flag used by users of old Rust versions.").into(),
                        Some((format!("{name} versions").into(), urler.all_versions(&origin).unwrap_or_else(|| urler.crate_by_origin(&origin)).into())))
                },
                Warning::NotAPackage => (3, "Cargo.toml parse error".into(), w.to_string().into(), None),
                Warning::CryptocurrencyBS => {
                    extended_desc = Some("Author of this site is firmly convinced that all cryptocurrencies have a net-negative effect on the society, and asks you to reconsider your choices.");
                    (3, "Cryptocurrency crate".into(), "This crate has been classified as related to the planet-incinerating mania. If you believe this categorization is a mistake, then review crate's categories and keywords, or file a bug. If it is related, then please yank it.".into(), None)
                },
                Warning::Chonky(size) => {
                    extended_desc = Some("Check that large files weren't included by accident. Note that tarballs uploaded to crates.io can't be used to run examples or tests, so it's fine to exclude them and their data files. You can use the exclude property in Cargo.toml to minimize crate's download size. Crates.io keeps all versions of all crates forever, so this storage adds up.");
                    (1, "Big download".into(), format!("The crate is a {}MB download. You can use `cargo package` command to review crate's archive in target/package.", size/1000/1000).into(), None)
                },
                Warning::SysNoLinks => {
                    extended_desc = Some("This is also needed to protect your crate from duplicate older versions of itself. C symbols are global, and duplicate symbols can cause all sorts of breakage.");
                    (1, "*-sys crate without links property".into(), format!("If this crate uses C libraries with public symbols, consider adding links = \"{}\" to crate's Cargo.toml to avoid other libraries colliding with them. Note that the links property adds exclusivity to dependency resolution, but doesn't do any linking.", k.short_name().trim_end_matches("-sys").trim_start_matches("lib")).into(), None)
                },
                Warning::Reserved => {
                    extended_desc = Some("It's OK if you intend to publish this project in the near future. Keep in mind that even if you have good intentions, things may not go as planned. crates.io won't reclaim abandoned crates, so reserving good names may end up wasting the good names.");
                    (1, "Crate is 'reserved'".into(), "Please be respectful of crates.io and don't squat crate names. You can ensure the crate can be given to someone else by co-owners, e.g. rust-bus org maintainers: cargo owner --add rust-bus-owner".to_string().into(),
                    Some(("Rust-bus maintainers".into(), "https://users.rust-lang.org/t/bus-factor-1-for-crates/17046".into())))
                },
                Warning::StaleRelease(days, is_stable, severity) => {
                    if k.is_nightly() {
                        extended_desc = Some("Nightly crates tend to have a short lifespan. We're delisting them if they're not updated frequently.");
                    } else if is_stable && k.version_semver().map_or(true, |v| v.major == 0) {
                        extended_desc = Some("If the crate is truly stable, why not make a 1.0.0 release?");
                    } else if k.edition() == Edition::E2015 {
                        extended_desc = Some("It's an opportunity to update it to the current Rust edition.");
                    } else {
                        extended_desc = Some("Users pay attention to the latest release date. Even if the crate is perfectly fine as-is, users may not know that.");
                    }
                    let num = if days > 366*2 { days / 366 } else { days / 31 };
                    let unit = if days > 366*2 { "year" } else { "month" };
                    (severity,
                        format!("Latest {}release is old", if is_stable {"stable "} else {"pre"}).into(),
                        format!("It's been over {num} {unit}{}. {}? Make a new release, either to refresh it, or to set [badges.maintenance] status = \"deprecated\" (or \"as-is\", \"passively-maintained\").", if num != 1 {"s"} else {""},
                            if k.maintenance() == MaintenanceStatus::Experimental {"How did the experiment go"} else {"Is this crate still maintained"}).into(),
                        Some(("Maintenance status field docs".into(), "https://doc.rust-lang.org/cargo/reference/manifest.html#the-badges-section".into())))
                },
                Warning::NotFoundInRepo => {
                    let repo = k.repository_http_url();
                    let repo_url = repo.as_ref().map(|(_, url)| &**url).unwrap_or("???");
                    extended_desc = Some("If it's a newly released crate, it's possible we haven't finished indexing the repository yet.");
                    (1, "Could not find the crate in the repository".into(), format!("Make sure the main branch of {repo_url} contains the Cargo.toml for the crate. If you have forked the crate, change the repository property in Cargo.toml to your fork's URL.").into(), None)
                },
                Warning::LicenseSpdxSyntax => {
                    (1, format!("License {} is not in SPDX syntax", k.license().unwrap_or("")).into(), "Use \"OR\" instead of \"/\".".into(), Some(("SPDX license list".into(), "https://spdx.org/licenses/".into())))
                }
            };
            warnings.push(StructuredWarning {
                severity, title, desc, url, extended_desc,
            });
        }
        (origin, crate_ranking, warnings)

    }
}

async fn latest_version_matching_requirement(req: &str, deadline: Instant, kitchen_sink: &KitchenSink, origin: &Origin) -> Option<SemVer> {
    match VersionReq::parse(req) {
        Ok(req) => {
            if let Ok(Ok(current)) = timeout_at(deadline.into(), kitchen_sink.rich_crate_async(origin)).await {
                let mut versions: Vec<_> = current.versions().iter().filter_map(|v| SemVer::parse(&v.num).ok()).collect();
                if let Some(highest_matching_req) = versions.iter().filter(|v| req.matches(v)).max().cloned() {
                    versions.retain(|v| v >= &highest_matching_req);
                }
                return versions.iter().filter(|v| v.pre.is_empty()).max()
                    .or_else(|| versions.iter().filter(|v| !v.pre.is_empty()).max())
                    .cloned()
                    .map(|mut v| {
                        if v.pre.is_empty() && v.major > 0 && v.minor > 0 && v.patch < 10 {
                            v.patch = 0;
                        }
                        v
                    });
            }
        },
        Err(e) => warn!("{req} parse error: {e}"),
    }
    None
}

fn comma_list(items: impl Iterator<Item=impl std::fmt::Display>) -> String {
    let mut res = items.take(5).map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ");
    if res.is_empty() {
        res.push('…');
    }
    res
}
