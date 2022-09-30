use categories::Category;
use heck::ToKebabCase;
use kitchen_sink::CrateAuthor;
use kitchen_sink::UserType;
use rich_crate::Origin;
use rich_crate::Repo;
use rich_crate::RepoHost;
use rich_crate::RichCrateVersion;
use rich_crate::RichDep;
use urlencoding::Encoded;

/// One thing responsible for link URL scheme on the site.
/// Should be used for every internal `<a href>`.
pub struct Urler {
    own_crate: Option<Origin>,
}

impl Urler {
    pub fn new(own_crate: Option<Origin>) -> Self {
        Self { own_crate }
    }

    /// Link to a dependency of a crate
    pub fn dependency(&self, dep: &RichDep) -> String {
        if let Some(git) = dep.dep.git() {
            if let Ok(repo) = Repo::new(git) {
                return match repo.host() {
                    RepoHost::GitHub(repo) => format!("/gh/{}/{}/{}", Encoded(&*repo.owner), Encoded(&*repo.repo), Encoded(&*dep.package)),
                    RepoHost::GitLab(repo) => format!("/lab/{}/{}/{}", Encoded(&*repo.owner), Encoded(&*repo.repo), Encoded(&*dep.package)),
                    _ => repo.canonical_http_url("").into_owned(),
                };
            }
        } else if dep.dep.detail().map_or(false, |d| d.path.is_some()) {
            if let Some(Origin::GitHub { ref repo, .. }) = self.own_crate {
                return format!("/gh/{}/{}/{}", Encoded(&*repo.owner), Encoded(&*repo.repo), Encoded(&*dep.package));
            }
            if let Some(Origin::GitLab { ref repo, .. }) = self.own_crate {
                return format!("/lab/{}/{}/{}", Encoded(&*repo.owner), Encoded(&*repo.repo), Encoded(&*dep.package));
            }
        }
        format!("/crates/{}", Encoded(&*dep.package))
    }

    /// Summary of all dependencies
    pub fn deps(&self, krate: &RichCrateVersion) -> Option<String> {
        match krate.origin() {
            Origin::CratesIo(_) => None,
            Origin::GitHub { repo, .. } => Some(format!("https://deps.rs/repo/github/{}/{}", Encoded(&*repo.owner), Encoded(&*repo.repo))),
            Origin::GitLab { repo, .. } => Some(format!("https://deps.rs/repo/gitlab/{}/{}", Encoded(&*repo.owner), Encoded(&*repo.repo))),
        }
    }

    pub fn install(&self, origin: &Origin) -> String {
        match origin {
            Origin::CratesIo(lowercase_name) => {
                format!("/install/{}", Encoded::str(&lowercase_name))
            }
            Origin::GitHub { repo, package } | Origin::GitLab { repo, package } => {
                let host = if let Origin::GitHub { .. } = origin { "gh" } else { "lab" };
                format!("/install/{}/{}/{}/{}", host, Encoded(&*repo.owner), Encoded(&*repo.repo), Encoded::str(&package))
            }
        }
    }

    pub fn all_versions(&self, origin: &Origin) -> Option<String> {
        match origin {
            Origin::CratesIo(lowercase_name) => {
                Some(format!("/crates/{}/versions", Encoded::str(&lowercase_name)))
            }
            Origin::GitHub { repo: _, package: _ } | Origin::GitLab { repo: _, package: _ } => {
                // let host = if let Origin::GitHub { .. } = origin { "gh" } else { "lab" };
                // format!("/{}/{}/{}/{}/versions", host, Encoded(&*repo.owner), Encoded(&*repo.repo), Encoded::str(&package))
                None
            }
        }
    }

    pub fn reviews(&self, origin: &Origin) -> String {
        match origin {
            Origin::CratesIo(lowercase_name) => {
                format!("/crates/{}/crev", Encoded::str(&lowercase_name))
            },
            _ => unreachable!(),
        }
    }

    pub fn reverse_deps(&self, origin: &Origin) -> Option<String> {
        match origin {
            Origin::CratesIo(lowercase_name) => Some(format!("/crates/{}/rev", Encoded::str(&lowercase_name))),
            Origin::GitHub { .. } | Origin::GitLab { .. } => None,
        }
    }

    pub fn crates_io_crate(&self, origin: &Origin) -> Option<String> {
        match origin {
            Origin::CratesIo(lowercase_name) => Some(self.crates_io_crate_by_lowercase_name(lowercase_name)),
            _ => None,
        }
    }

    pub fn crates_io_crate_at_version(&self, origin: &Origin, version: &str) -> Option<String> {
        match origin {
            Origin::CratesIo(lowercase_name) => Some(format!("https://crates.io/crates/{}/{}", Encoded::str(&lowercase_name), Encoded(version))),
            _ => None,
        }
    }

    fn crates_io_crate_by_lowercase_name(&self, crate_name: &str) -> String {
        format!("https://crates.io/crates/{}", Encoded(crate_name))
    }

    pub fn docs_rs_source(&self, crate_name: &str, version: &str) -> String {
        format!("https://docs.rs/crate/{crate_name}/{version}/source/")
    }

    pub fn git_source(&self, origin: &Origin, version: &str) -> String {
        let mut url = self.crate_abs_path_by_origin(origin);
        use std::fmt::Write;
        let _ = write!(&mut url, "/source?at={}", Encoded(version));
        url
    }

    /// Link to crate individual page
    pub fn krate(&self, krate: &RichCrateVersion) -> String {
        self.crate_by_origin(krate.origin())
    }

    pub fn crate_by_origin(&self, o: &Origin) -> String {
        if self.own_crate.as_ref() != Some(o) {
            self.crate_abs_path_by_origin(o)
        } else {
            self.crates_io_crate_by_lowercase_name(o.short_crate_name())
        }
    }

    pub fn crate_abs_path_by_origin(&self, o: &Origin) -> String {
        match o {
            Origin::CratesIo(lowercase_name) => {
                format!("/crates/{}", Encoded::str(&lowercase_name))
            }
            Origin::GitHub { repo, package } => {
                format!("/gh/{}/{}/{}", Encoded(&*repo.owner), Encoded(&*repo.repo), Encoded::str(&package))
            }
            Origin::GitLab { repo, package } => {
                format!("/lab/{}/{}/{}", Encoded(&*repo.owner), Encoded(&*repo.repo), Encoded::str(&package))
            }
        }
    }

    /// FIXME: it doesn't normalize keywords as well as the db inserter
    pub fn keyword(&self, name: &str) -> String {
        format!("/keywords/{}", Encoded(&name.to_kebab_case()))
    }

    /// First page of category listing
    pub fn category(&self, cat: &Category) -> String {
        let mut out = String::with_capacity(1 + cat.slug.len());
        for s in cat.slug.split("::") {
            out.push('/');
            out.push_str(s);
        }
        out
    }

    /// Crate author's URL
    ///
    /// This will probably change to a listing page rather than arbitrary personal URL
    pub fn author(&self, author: &CrateAuthor<'_>) -> Option<String> {
        if let Some(ref gh) = author.github {
            Some(match (gh.user_type, author.owner) {
                (UserType::User, true) => self.crates_io_user_by_github_login(&gh.login),
                (UserType::User, _) |
                (UserType::Org, _) | (UserType::Bot, _) => format!("https://github.com/{}", Encoded(&gh.login)),
            })
        } else if let Some(ref info) = author.info {
            if let Some(ref em) = info.email {
                Some(format!("mailto:{}", em)) // add name? encode?
            } else if let Some(ref url) = info.url {
                if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("mailto:") {
                    Some(url.to_owned())
                } else {
                    println!("bad info url: {:?}", author);
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn crates_io_user_by_github_login(&self, login: &str) -> String {
        format!("/~{}", Encoded(login))
    }

    pub fn crates_io_user_maintainer_dashboard_by_github_login(&self, login: &str) -> String {
        format!("/~{}/dash", Encoded(login))
    }

    pub fn crates_io_user_maintainer_dashboard_atom_by_github_login(&self, login: &str) -> String {
        format!("/~{}/dash.xml", Encoded(login))
    }

    pub fn search_crates_io(&self, query: &str) -> String {
        format!("https://crates.io/search?q={}", Encoded(query))
    }

    pub fn search_lib_rs(&self, query: &str) -> String {
        format!("/search?q={}", Encoded(query))
    }

    pub fn search_ddg(&self, query: &str) -> String {
        format!("https://duckduckgo.com/?q=site%3Alib.rs+{}", Encoded(query))
    }
}
