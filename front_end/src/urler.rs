use categories::Category;
use heck::KebabCase;
use kitchen_sink::CrateAuthor;
use kitchen_sink::UserType;
use rich_crate::Origin;
use rich_crate::Repo;
use rich_crate::RepoHost;
use rich_crate::RichCrateVersion;
use rich_crate::RichDep;
use urlencoding::encode;

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
                    RepoHost::GitHub(repo) => format!("/gh/{}/{}/{}", encode(&repo.owner), encode(&repo.repo), encode(&dep.package)),
                    RepoHost::GitLab(repo) => format!("/lab/{}/{}/{}", encode(&repo.owner), encode(&repo.repo), encode(&dep.package)),
                    _ => repo.canonical_http_url("").into_owned(),
                }
            }
        } else if dep.dep.detail().map_or(false, |d| d.path.is_some()) {
            if let Some(Origin::GitHub{ref repo,..}) = self.own_crate {
                return format!("/gh/{}/{}/{}", encode(&repo.owner), encode(&repo.repo), encode(&dep.package))
            }
            if let Some(Origin::GitLab{ref repo,..}) = self.own_crate {
                return format!("/lab/{}/{}/{}", encode(&repo.owner), encode(&repo.repo), encode(&dep.package))
            }
        }
        format!("/crates/{}", encode(&dep.package))
    }

    /// Summary of all dependencies
    pub fn deps(&self, krate: &RichCrateVersion) -> String {
        match krate.origin() {
            Origin::CratesIo(_) => {
                format!("https://deps.rs/crate/{}/{}", encode(&krate.short_name()), encode(krate.version()))
            },
            Origin::GitHub {repo, ..} => {
                format!("https://deps.rs/repo/github/{}/{}", encode(&repo.owner), encode(&repo.repo))
            },
            Origin::GitLab {repo, ..} => {
                format!("https://deps.rs/repo/gitlab/{}/{}", encode(&repo.owner), encode(&repo.repo))
            },
        }
    }

    pub fn install(&self, origin: &Origin) -> String {
        match origin {
            Origin::CratesIo(lowercase_name) => {
                format!("/install/{}", encode(lowercase_name))
            },
            Origin::GitHub {repo, package} | Origin::GitLab {repo, package} => {
                let host = if let Origin::GitHub {..} = origin {"gh"} else {"lab"};
                format!("/install/{}/{}/{}/{}", host, encode(&repo.owner), encode(&repo.repo), encode(package))
            },
        }
    }

    pub fn reverse_deps(&self, krate: &RichCrateVersion) -> String {
        format!("https://crates.io/crates/{}/reverse_dependencies", encode(krate.short_name()))
    }

    pub fn crates_io_crate(&self, origin: &Origin) -> Option<String> {
        match origin {
            Origin::CratesIo(lowercase_name) => Some(self.crates_io_crate_by_lowercase_name(lowercase_name)),
            _ => None,
        }
    }

    fn crates_io_crate_by_lowercase_name(&self, crate_name: &str) -> String {
        format!("https://crates.io/crates/{}", encode(crate_name))
    }

    /// Link to crate individual page
    pub fn krate(&self, krate: &RichCrateVersion) -> String {
        self.crate_by_origin(krate.origin())
    }

    pub fn crate_by_origin(&self, o: &Origin) -> String {
        match o {
            Origin::CratesIo(lowercase_name) => {
                match self.own_crate {
                    Some(Origin::CratesIo(ref own)) if own == lowercase_name => {
                        self.crates_io_crate_by_lowercase_name(lowercase_name)
                    },
                    _ => format!("/crates/{}", encode(lowercase_name)),
                }
            },
            Origin::GitHub {repo, package} => {
                format!("/gh/{}/{}/{}", encode(&repo.owner), encode(&repo.repo), encode(package))
            },
            Origin::GitLab {repo, package} => {
                format!("/lab/{}/{}/{}", encode(&repo.owner), encode(&repo.repo), encode(package))
            },
        }
    }


    pub fn keyword(&self, name: &str) -> String {
        format!("/keywords/{}", encode(&name.to_kebab_case()))
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

    /// n-th page (1-indexed) of category listing
    pub fn category_page(&self, cat: &Category, p: usize) -> String {
        if p < 2 {
            return self.category(cat);
        }
        format!("https://crates.io/categories/{}?page={}", cat.slug, p)
    }

    /// Crate author's URL
    ///
    /// This will probably change to a listing page rather than arbitrary personal URL
    pub fn author(&self, author: &CrateAuthor<'_>) -> Option<String> {
        if let Some(ref gh) = author.github {
            Some(match gh.user_type {
                UserType::User => format!("https://crates.io/users/{}", encode(&gh.login)),
                UserType::Org | UserType::Bot => format!("https://github.com/{}", encode(&gh.login)),
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

    pub fn search_crates_io(&self, query: &str) -> String {
        format!("https://crates.io/search?q={}", encode(query))
    }

    pub fn search_crates_rs(&self, query: &str) -> String {
        format!("https://lib.rs/search?q={}", encode(query))
    }

    pub fn search_ddg(&self, query: &str) -> String {
        format!("https://duckduckgo.com/?q=site%3Alib.rs+{}", encode(query))
    }
}
