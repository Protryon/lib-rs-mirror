use chrono::DateTime;
use chrono::FixedOffset;
use chrono::Utc;
use feat_extractor::{is_deprecated_requirement, is_squatspam};
use kitchen_sink::CResult;
use kitchen_sink::Edition;
use kitchen_sink::KitchenSink;
use kitchen_sink::MaintenanceStatus;
use kitchen_sink::Origin;
use kitchen_sink::RichCrate;
use kitchen_sink::CrateVersion;
use kitchen_sink::RichCrateVersion;
use kitchen_sink::RichDep;
use kitchen_sink::VersionReq;
use kitchen_sink::Warning;
use semver::Version as SemVer;
use std::collections::HashSet;

pub async fn warnings_for_crate(c: &KitchenSink, k: &RichCrateVersion, all: &RichCrate) -> CResult<HashSet<Warning>> {
    if k.category_slugs().iter().any(|c| c == "cryptography::cryptocurrencies") {
        return Ok(HashSet::from([Warning::CryptocurrencyBS]));
    }

    let mut warnings = c.rich_crate_warnings(k.origin()).await?;

    let (tarball_byte_size, _) = k.crate_size();
    if tarball_byte_size > 10_000_000 {
        warnings.insert(Warning::Chonky(tarball_byte_size));
    }

    if is_squatspam(k) {
        warnings.retain(|w| !matches!(w, Warning::NoKeywords | Warning::NoCategories | Warning::NoRepositoryProperty));
        warnings.insert(Warning::Reserved);
        return Ok(warnings);
    }

    let versions = all.versions().iter().filter(|v| !v.yanked).filter_map(|v| {
        Some((v.num.parse::<SemVer>().ok()?, v))
    }).collect::<Vec<_>>();

    if k.license().map_or(false, |l| l.contains('/')) {
        warnings.insert(Warning::LicenseSpdxSyntax);
    }

    if !k.has_path_in_repo() && warnings.get(&Warning::NoRepositoryProperty).is_none() && !warnings.iter().any(|w| matches!(w, Warning::ErrorCloning(_))) {
        warnings.insert(Warning::NotFoundInRepo);
    }

    // This uses dates, not semvers, because we care about crates giving signs of life,
    // even if by patching old semvers.
    let latest_stable = find_most_recent_release(&versions, false);
    let latest_unstable = find_most_recent_release(&versions, true)
        // stabilized unstable releases are not relevant
        .filter(|(_, unstable_date)| latest_stable.as_ref().map_or(true, |(_, stable_date)| unstable_date > stable_date));

    let now = Utc::now();

    fn maintenance_status_factor(m: MaintenanceStatus) -> u32 {
        match m {
            MaintenanceStatus::Experimental => 1,
            MaintenanceStatus::ActivelyDeveloped => 2,
            MaintenanceStatus::None => 3,
            _ => 8,
        }
    }

    if k.maintenance() != MaintenanceStatus::AsIs && k.maintenance() != MaintenanceStatus::Deprecated {
        if let Some((_, reldate)) = &latest_stable {
            let days_since = now.signed_duration_since(*reldate).num_days() as u32;
            let stale_after = (5*466).min(if k.is_nightly() { 6*31 } else { 36*31 } * if latest_unstable.is_some() { 2 } else { 1 } * maintenance_status_factor(k.maintenance())/3);
            if days_since > stale_after {
               warnings.insert(Warning::StaleRelease(days_since, true, (days_since/stale_after).min(3) as u8));
            }
        }
        if let Some((_, reldate)) = &latest_unstable {
            let days_since = now.signed_duration_since(*reldate).num_days() as u32;
            let stale_after = (366).min(if k.is_nightly() { 2*31 } else { 3*31 } * if latest_stable.is_some() { 1 } else { 3 } * maintenance_status_factor(k.maintenance())/3);
            if days_since > stale_after {
               warnings.insert(Warning::StaleRelease(days_since, false, (days_since/stale_after).min(3) as u8));
            }
        }
    }

    // Some crates are internal details and don't need to be listed in a category
    let last_word = k.short_name().rsplit(|c: char| c == '_' || c == '-').next().unwrap_or("");
    if k.is_proc_macro() || last_word == "impl" || last_word == "internal" {
        warnings.remove(&Warning::NoCategories);
        warnings.remove(&Warning::NoKeywords);
    }

    if k.is_sys() && k.links().is_none() && k.short_name() != "libc" {
        warnings.insert(Warning::SysNoLinks);
    }

    if let Some(readme_raw_path) = k.readme_raw_path() {
        if readme_raw_path.starts_with("..") || readme_raw_path.starts_with('/') {
            warnings.insert(Warning::EscapingReadmePath(readme_raw_path.into()));
        }
    }

    let compat = c.rustc_compatibility(&all).await?;
    if let Some(newest_bad) = compat.values().rev().filter_map(|c| c.newest_bad).next() {
        // if it's not compatible with the old compiler, there's no point using an old-compiler edition
        if newest_bad >= 55 && k.edition() < Edition::E2021 {
            warnings.insert(Warning::EditionMSRV(k.edition(), newest_bad+1));
        }
        else if newest_bad >= 30 && k.edition() < Edition::E2018 {
            warnings.insert(Warning::EditionMSRV(k.edition(), newest_bad+1));
        }

        // rust-version should be set for msrv > 1.56
        let explicit_msrv = k.explicit_msrv()
            .and_then(|v| v.split('.').nth(1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        if newest_bad >= 56 && explicit_msrv <= newest_bad {
            warnings.insert(Warning::BadMSRV(newest_bad+1, explicit_msrv));
        }
    }
    if !k.is_app() && !c.has_docs_rs(k.origin(), k.short_name(), k.version()).await {
        warnings.insert(Warning::DocsRs);
    }
    let (runtime, dev, build) = k.direct_dependencies();
    warn_outdated_deps(&runtime, &mut warnings, &c).await;
    warn_outdated_deps(&build, &mut warnings, &c).await;
    warn_bad_requirements(&runtime, &mut warnings, &c).await;
    warn_bad_requirements(&build, &mut warnings, &c).await;
    // dev deps are very low priority, so don't warn about them unless there's nothing else to do
    if warnings.is_empty() {
        warn_outdated_deps(&dev, &mut warnings, &c).await;
    }
    Ok(warnings)
}

fn find_most_recent_release<'a>(versions: &'a [(SemVer, &CrateVersion)], pre: bool) -> Option<(&'a SemVer, DateTime<FixedOffset>)> {
    versions.iter().filter(move |(v, _)| pre == v.is_prerelease()).max_by(|a,b| a.1.created_at.cmp(&b.1.created_at))
        .and_then(|(v, c)| Some((v, DateTime::parse_from_rfc3339(&c.created_at).ok()?)))
}

async fn warn_bad_requirements(dependencies: &[RichDep], warnings: &mut HashSet<Warning>, c: &KitchenSink) {
    for richdep in dependencies {
        let req_str = richdep.dep.req().trim();
        if req_str == "*" || !richdep.dep.is_crates_io() {
            warnings.insert(Warning::BadRequirement(richdep.package.clone(), req_str.into()));
            continue;
        }

        if !req_str.contains('.') {
            let mut its_fine = false;
            let required_dep = Origin::from_crates_io_name(&richdep.package);
            if let Ok(k) = c.rich_crate_version_stale_is_ok(&required_dep).await {
                if let Ok(v) = k.version_semver() {
                    its_fine = v.minor == 0; // if there's only 'x.0.y' version, then requirement 'x' is fine
                }
            }
            if !its_fine {
                warnings.insert(Warning::LaxRequirement(richdep.package.clone(), req_str.into()));
            }
        }

        match req_str.parse::<VersionReq>() {
            Ok(req) => {
                // allow prerelease match to be exact; binary release likely needs to match
                if req.is_exact() && !req_str.split('+').next().unwrap().contains('-') && !richdep.package.contains("x86_64-") && !richdep.package.contains("aarch64-") {
                    warnings.insert(Warning::ExactRequirement(richdep.package.clone(), req_str.into()));
                }
            },
            Err(err) => {
                warnings.insert(Warning::BadRequirement(richdep.package.clone(), err.to_string().into()));
            }
        }
    }
}

async fn warn_outdated_deps(dependencies: &[RichDep], warnings: &mut HashSet<Warning>, c: &KitchenSink) {
    for richdep in dependencies {
        if let Ok(req) = richdep.dep.req().parse() {
            if is_deprecated_requirement(&richdep.package, &req) {
                warnings.insert(Warning::DeprecatedDependency(richdep.package.clone(), richdep.dep.req().into()));
                continue;
            }
            if let Ok(Some(pop)) = c.version_popularity(&richdep.package, &req).await {
                if pop.lost_popularity && pop.pop < 0.2 {
                    warnings.insert(Warning::DeprecatedDependency(richdep.package.clone(), richdep.dep.req().into()));
                    continue;
                }
                if pop.matches_latest {
                    continue;
                }
                let outdated_percent = ((1. - pop.pop) * 100.).round() as u8;
                warnings.insert(Warning::OutdatedDependency(richdep.package.clone(), richdep.dep.req().into(), outdated_percent));
            }
        }
    }
}
