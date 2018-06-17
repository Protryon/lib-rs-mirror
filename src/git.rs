use cargo_toml::TomlPackage;
use render_readme::Markup;
use render_readme::Readme;
use repo_url::Repo;
use std::fs::create_dir;
use std::fs::read_dir;
use std::fs::read_to_string;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use is_readme_filename;
use readme_from_repo;
use Result;
use UnarchiverError;

pub fn checkout(repo: &Repo, data_path: &Path, name: &str) -> Result<PathBuf> {
    let checkout = data_path.join("git").join(name); // FIXME: bad version
    if !checkout.exists() {
        let url = &*repo.canonical_git_url();
        println!("Fallback git clone of {}", url);
        let output = Command::new("git")
            .arg("clone")
            .arg("--depth=1")
            .arg("--config").arg("core.askPass=true")
            .arg("--")
            .arg(url)
            .arg(&checkout)
            .output()?;
        if !output.status.success() {
            create_dir(&checkout).ok(); // yes, make even in error, so it's not retried over and over again
            Err(UnarchiverError::GitCheckoutFailed(String::from_utf8_lossy(&output.stderr).to_string()))?;
        }
    }
    Ok(checkout)
}

pub fn find_readme(git_checkout: &Path, package: &TomlPackage) -> Result<Option<Readme>> {
    for e in read_dir(git_checkout)? {
        let e = e?;
        if is_readme_filename(e.file_name().as_ref(), Some(package)) {
            let text = read_to_string(e.path())?;
            let markup = if e.path().extension().map_or(false, |e| e == "rst") {
                Markup::Rst(text)
            } else {
                Markup::Markdown(text)
            };
            return Ok(Some(readme_from_repo(markup, &package.repository)));
        }
    }
    Ok(None)
}
