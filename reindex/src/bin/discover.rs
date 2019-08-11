use std::io::BufRead;
use repo_url::*;
use std::io;
use kitchen_sink::KitchenSink;
use kitchen_sink::Origin;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let crates = KitchenSink::new_default()?;

    for line in io::stdin().lock().lines() {
        let mut line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if !line.contains("github.com") {
            line = format!("https://github.com/{}", line.trim_start_matches('/'));
        }
        if let Err(e) = check_repo(&line, &crates) {
            eprintln!("{}: {}", line, e);
            let mut src = e.source();
            while let Some(e) = src {
                eprintln!(" {}", e);
                src = e.source();
            }
        }
    }
    Ok(())
}

fn check_repo(line: &str, crates: &KitchenSink) -> Result<(), Box<dyn std::error::Error>> {
    let repo = Repo::new(line)?;
    if let RepoHost::GitHub(gh) = repo.host() {
        println!("\nFetching {}/{}", gh.owner, gh.repo);
        let manifests = crates.inspect_repo_manifests(&repo)?;
        for (path, _, manif) in manifests {
            if let Some(pkg) = &manif.package {
                if let Some(d) = &pkg.description {
                    println!("// {}", d);
                }
                if crates.crate_exists(&Origin::from_github(gh.clone(), pkg.name.as_str())) {
                    print!("// GIT alredy exists! ");
                }
                if crates.crate_exists(&Origin::from_crates_io_name(&pkg.name)) {
                    print!("// crate alredy exists! https://lib.rs/crates/{} ", pkg.name);
                }
                println!("\"github:{}/{}/{}\",{}", gh.owner, gh.repo, pkg.name, if path != "" && path != pkg.name {format!(" // in {}", path)} else {String::new()});
            }
        }
    } else {
        eprintln!("Not supported host: {:?}", repo);
    }
    Ok(())
}
