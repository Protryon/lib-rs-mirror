use kitchen_sink::KitchenSink;
use kitchen_sink::Origin;
use repo_url::*;
use std::io;
use std::io::BufRead;
use std::io::Write;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let crates = KitchenSink::new_default().await?;

    for line in io::stdin().lock().lines() {
        let mut line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if !line.starts_with("https://") {
            line = format!("https://github.com/{}", line.trim_start_matches('/'));
        }
        if let Err(e) = check_repo(&line, &crates).await {
            eprintln!("{line}: {e}");
            let mut src = e.source();
            while let Some(e) = src {
                eprintln!(" {e}");
                src = e.source();
            }
        }
    }
    Ok(())
}

async fn check_repo(line: &str, crates: &KitchenSink) -> Result<(), Box<dyn std::error::Error>> {
    let repo = Repo::new(line)?;
    if let Repo::GitHub(gh) | Repo::GitLab(gh) = repo.host() {
        print!("\nFetching {}/{}…", gh.owner, gh.repo);
        std::io::stdout().flush()?;
        let manifests = crates.inspect_repo_manifests(&repo).await?;
        println!(" {} found", manifests.len());
        for f in manifests {
            let manif = f.manifest;
            let path = f.inner_path;
            if let Some(pkg) = &manif.package {
                if path.contains("example") {
                    println!("// skip {path} {}", pkg.name());
                    continue;
                }
                if crates.crate_exists(&Origin::from_github(gh.clone(), pkg.name())) {
                    print!("// GIT alredy exists! ");
                } else if crates.crate_exists(&Origin::from_crates_io_name(pkg.name())) {
                    print!("// crate alredy exists! https://lib.rs/crates/{} ", pkg.name());
                    if let Some(d) = pkg.description() {
                        print!("// {} // ", d.trim());
                    }
                } else if let Some(d) = pkg.description() {
                    println!("// {}", d.trim());
                }
                println!("github:{}/{}/{}\n,{}", gh.owner, gh.repo, pkg.name, if !path.is_empty() && path != pkg.name() {format!(" // in {path}")} else {String::new()});
            }
        }
    } else {
        eprintln!("Not supported host: {repo:?}");
    }
    Ok(())
}
