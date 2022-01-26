use crate::writer::*;
use actix_web::body::BoxBody;
use actix_web::dev::Url;
use actix_web::http::header::HeaderValue;
use actix_web::http::StatusCode;
use actix_web::middleware;
use actix_web::HttpResponse;
use actix_web::{web, App, HttpRequest, HttpServer};
use arc_swap::ArcSwap;
use cap::Cap;
use categories::Category;
use categories::CATEGORIES;
use chrono::prelude::*;
use anyhow::{anyhow, Context};
use futures::future::Future;
use futures::future::FutureExt;
use kitchen_sink::filter::ImageOptimAPIFilter;
use kitchen_sink::KitchenSink;
use kitchen_sink::Origin;
use kitchen_sink::RichCrate;
use locale::Numeric;
use render_readme::{Highlighter, Markup, Renderer};
use repo_url::SimpleRepo;
use search_index::CrateSearchIndex;
use std::convert::TryInto;
use std::env;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::runtime::Handle;
use urlencoding::decode;
use urlencoding::Encoded;

#[macro_use]
extern crate log;

mod writer;

#[global_allocator]
static ALLOCATOR: Cap<std::alloc::System> = Cap::new(std::alloc::System, 4 * 1024 * 1024 * 1024);

static HUP_SIGNAL: AtomicU32 = AtomicU32::new(0);

struct ServerState {
    markup: Renderer,
    index: CrateSearchIndex,
    crates: ArcSwap<KitchenSink>,
    page_cache_dir: PathBuf,
    data_dir: PathBuf,
    rt: Handle,
    background_job: tokio::sync::Semaphore,
    foreground_job: tokio::sync::Semaphore,
    start_time: Instant,
    last_ok_response: AtomicU32,
}

type AServerState = web::Data<ServerState>;

fn main() {
    let mut b = env_logger::Builder::from_default_env();
    b.filter_module("html5ever", log::LevelFilter::Error);
    b.filter_module("tokei", log::LevelFilter::Error);
    b.filter_module("hyper", log::LevelFilter::Warn);
    b.filter_module("tantivy", log::LevelFilter::Error);
    if cfg!(debug_assertions) {
        b.filter_level(log::LevelFilter::Debug);
    }
    b.init();

    let sys = actix_web::rt::System::new();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("server-bg")
        .build()
        .unwrap();

    let _ = std::panic::catch_unwind(|| console_subscriber::init());

    let res = sys.block_on(run_server(rt.handle().clone()));

    rt.shutdown_timeout(Duration::from_secs(1));

    if let Err(e) = res {
        for c in e.chain() {
            error!("Error: {}", c);
        }
        std::process::exit(1);
    }
}

async fn run_server(rt: Handle) -> Result<(), anyhow::Error> {
    unsafe { signal_hook::low_level::register(signal_hook::consts::SIGHUP, || HUP_SIGNAL.store(1, Ordering::SeqCst)) }?;
    unsafe { signal_hook::low_level::register(signal_hook::consts::SIGUSR1, || HUP_SIGNAL.store(1, Ordering::SeqCst)) }?;

    kitchen_sink::dont_hijack_ctrlc();

    let public_document_root: PathBuf = env::var_os("DOCUMENT_ROOT").map(From::from).unwrap_or_else(|| "../style/public".into());
    let page_cache_dir: PathBuf = "/var/tmp/crates-server".into();
    let data_dir: PathBuf = env::var_os("CRATE_DATA_DIR").map(From::from).unwrap_or_else(|| "../data".into());
    let github_token = env::var("GITHUB_TOKEN").context("GITHUB_TOKEN missing")?;

    let _ = std::fs::create_dir_all(&page_cache_dir);
    assert!(page_cache_dir.exists(), "{} does not exist", page_cache_dir.display());
    assert!(public_document_root.exists(), "DOCUMENT_ROOT {} does not exist", public_document_root.display());
    assert!(data_dir.exists(), "CRATE_DATA_DIR {} does not exist", data_dir.display());


    let crates = rt.spawn({
        let data_dir = data_dir.clone();
        let github_token = github_token.clone();
        async move {
            KitchenSink::new(&data_dir, &github_token).await
        }
    }).await??;
    let image_filter = Arc::new(ImageOptimAPIFilter::new("czjpqfbdkz", crates.main_cache_dir().join("images.db")).await?);
    let markup = Renderer::new_filter(Some(Highlighter::new()), image_filter);

    let index = CrateSearchIndex::new(&data_dir)?;

    let state = web::Data::new(ServerState {
        markup,
        index,
        crates: ArcSwap::from_pointee(crates),
        page_cache_dir,
        data_dir: data_dir.clone(),
        rt,
        background_job: tokio::sync::Semaphore::new(5),
        foreground_job: tokio::sync::Semaphore::new(32),
        start_time: Instant::now(),
        last_ok_response: AtomicU32::new(0),
    });

    let timestamp = Arc::new(AtomicU32::new(0));

    // event observer
    state.rt.spawn({
        let state = state.clone();
        let mut subscriber = state.crates.load().event_log().subscribe("server observer").unwrap();
        async move {
            loop {
                use kitchen_sink::SharedEvent::*;
                let batch = subscriber.next_batch().await.unwrap();
                debug!("Got events from the log {:?}", batch);
                for ev in batch.filter_map(|e| e.ok()) {
                    match ev {
                        CrateUpdated(origin_str) => {
                            info!("New crate updated {}", origin_str);
                            let o = Origin::from_str(&origin_str);
                            let cache_path = state.page_cache_dir.join(cache_file_name_for_origin(&o));
                            let _ = std::fs::remove_file(&cache_path);
                        },
                        CrateIndexed(origin_str) => {
                            info!("Purging local cache {}", origin_str);
                            let o = Origin::from_str(&origin_str);
                            state.crates.load().reload_indexed_crate(&o);
                            let cache_path = state.page_cache_dir.join(cache_file_name_for_origin(&o));
                            let _ = std::fs::remove_file(&cache_path);
                            background_refresh(state.clone(), cache_path, render_crate_page(state.clone(), o));
                        },
                        CrateNeedsReindexing(origin_str) => {
                            info!("Heard about outdated crate {}", origin_str);
                            let o = Origin::from_str(&origin_str);
                            let s2 = state.clone();
                            state.rt.spawn(async move {
                                // this kicks off indexing if not cached
                                let _ = s2.crates.load().rich_crate_version_async(&o).await;
                            });
                        },
                        DailyStatsUpdated => {
                            let _ = std::fs::remove_file(state.page_cache_dir.join("_stats_.html"));
                        },
                    }
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    });

    // refresher thread
    state.rt.spawn({
        let state = state.clone();
        let timestamp = timestamp.clone();
        async move {
            let mut last_reload = Instant::now();
            state.crates.load().prewarm().await;
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                let elapsed = state.start_time.elapsed().as_secs() as u32;
                timestamp.store(elapsed, Ordering::SeqCst);
                let should_reload = if 1 == HUP_SIGNAL.swap(0, Ordering::SeqCst) {
                    info!("HUP!");
                    true
                } else if last_reload.elapsed() > Duration::from_secs(30 * 60) {
                    info!("Reloading state on a timer");
                    true
                } else {
                    false
                };
                if should_reload {
                    match KitchenSink::new(&data_dir, &github_token).await {
                        Ok(k) => {
                            info!("Reloading state");
                            state.crates.load().cleanup();
                            let k = Arc::new(k);
                            let _ = tokio::task::spawn({
                                let k = k.clone();
                                async move {
                                    k.update().await;
                                    k.prewarm().await;
                                }
                            }).await;
                            last_reload = Instant::now();
                            state.crates.store(k);
                            info!("Reloaded state");
                        },
                        Err(e) => {
                            error!("Refresh failed: {}", e);
                            std::process::exit(1);
                        },
                    }
                }
            }
        }});

    // watchdog
    std::thread::spawn({
        let state = Arc::clone(&state);
        move || {
        std::thread::sleep(Duration::from_secs(30)); // give startup some time
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let expected = state.start_time.elapsed().as_secs() as u32;
            let rt_timestamp = timestamp.load(Ordering::SeqCst);
            if rt_timestamp + 8 < expected {
                warn!("Update loop is {}s behind", expected - rt_timestamp);
                if rt_timestamp + 300 < expected {
                    error!("tokio is dead");
                    std::process::exit(1);
                }
            }
            let response_timestamp = state.last_ok_response.load(Ordering::SeqCst);
            if response_timestamp + 60*5 < expected {
                warn!("no requests for 5 minutes? probably a deadlock");
                // don't exit in debug mode, because it's legitimately idling
                if !cfg!(debug_assertions) {
                    std::process::exit(2);
                }
            }
        }
    }});

    let server = HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .wrap(middleware::Compress::default())
            .wrap(middleware::DefaultHeaders::new().add(("x-powered-by", HeaderValue::from_static(concat!("actix-web lib.rs/", env!("CARGO_PKG_VERSION"))))))
            .wrap(middleware::Logger::default())
            .route("/", web::get().to(handle_home))
            .route("/search", web::get().to(handle_search))
            .route("/game-engines", web::get().to(handle_game_redirect))
            .route("/index", web::get().to(handle_search)) // old crates.rs/index url
            .route("/categories/{rest:.*}", web::get().to(handle_redirect))
            .route("/new", web::get().to(handle_new_trending))
            .route("/stats", web::get().to(handle_global_stats))
            .route("/keywords/{keyword}", web::get().to(handle_keyword))
            .route("/crates/{crate}", web::get().to(handle_crate))
            .route("/crates/{crate}/versions", web::get().to(handle_crate_all_versions))
            .route("/crates/{crate}/rev", web::get().to(handle_crate_reverse_dependencies))
            .route("/crates/{crate}/reverse_dependencies", web::get().to(handle_crate_reverse_dependencies_redir))
            .route("/crates/{crate}/crev", web::get().to(handle_crate_reviews))
            .route("/~{author}", web::get().to(handle_author))
            .route("/~{author}/dash", web::get().to(handle_maintainer_dashboard_html))
            .route("/~{author}/dash.xml", web::get().to(handle_maintainer_dashboard_xml))
            .route("/users/{author}", web::get().to(handle_author_redirect))
            .route("/install/{crate:.*}", web::get().to(handle_install))
            .route("/compat/{crate:.*}", web::get().to(handle_compat))
            .route("/{host}/{owner}/{repo}/{crate}", web::get().to(handle_repo_crate))
            .route("/{host}/{owner}/{repo}/{crate}/versions", web::get().to(handle_repo_crate_all_versions))
            .route("/atom.xml", web::get().to(handle_feed))
            .route("/sitemap.xml", web::get().to(handle_sitemap))
            .route("/{crate}/info/refs", web::get().to(handle_git_clone))
            .route("/crates/{crate}/info/refs", web::get().to(handle_git_clone))
            .service(actix_files::Files::new("/", &public_document_root))
            .default_service(web::route().to(default_handler))
    })
    .bind("127.0.0.1:32531")
    .expect("Can not bind to 127.0.0.1:32531")
    .shutdown_timeout(1);

    info!("Starting HTTP server {} on http://127.0.0.1:32531", env!("CARGO_PKG_VERSION"));
    server.run().await?;

    info!("bye!");
    Ok(())
}

fn mark_server_still_alive(state: &ServerState) {
    let elapsed = state.start_time.elapsed().as_secs() as u32;
    state.last_ok_response.store(elapsed, Ordering::SeqCst);
}

fn find_category<'a>(slugs: impl Iterator<Item = &'a str>) -> Option<&'static Category> {
    let mut found = None;
    let mut current_sub = &CATEGORIES.root;
    for slug in slugs {
        if let Some(cat) = current_sub.get(slug) {
            found = Some(cat);
            current_sub = &cat.sub;
        } else {
            return None;
        }
    }
    found
}

fn handle_static_page(state: &ServerState, path: &str) -> Result<Option<HttpResponse>, ServerError> {
    let path = &path[1..]; // remove leading /
    if !is_alnum(path) {
        return Ok(None);
    }

    let md_path = state.data_dir.as_path().join(format!("page/{}.md", path));
    if !md_path.exists() {
        return Ok(None);
    }

    let mut chars = path.chars();
    let path_capitalized = chars.next().into_iter().flat_map(|c| c.to_uppercase()).chain(chars).collect();
    let crates = state.crates.load();
    let crate_num = crates.all_crates_io_crates().len();
    let total_crate_num = crates.all_crates().count();

    let md = std::fs::read_to_string(md_path).context("reading static page")?
        .replace("$CRATE_NUM", &Numeric::english().format_int(crate_num))
        .replace("$TOTAL_CRATE_NUM", &Numeric::english().format_int(total_crate_num));
    let mut page = Vec::with_capacity(md.len() * 2);
    front_end::render_static_page(&mut page, path_capitalized, &Markup::Markdown(md), &state.markup)?;
    minify_html(&mut page);

    mark_server_still_alive(state);
    Ok(Some(HttpResponse::Ok()
        .content_type("text/html;charset=UTF-8")
        .insert_header(("Cache-Control", "public, max-age=7200, stale-while-revalidate=604800, stale-if-error=86400"))
        .no_chunking(page.len() as u64)
        .body(page)))
}

async fn default_handler(req: HttpRequest) -> Result<HttpResponse, ServerError> {
    let state: &AServerState = req.app_data().expect("appdata");
    let path = req.uri().path();
    assert!(path.starts_with('/'));
    if path.ends_with('/') {
        return Ok(HttpResponse::PermanentRedirect().insert_header(("Location", path.trim_end_matches('/'))).body(""));
    }

    if let Some(cat) = find_category(path.split('/').skip(1)) {
        return Box::pin(handle_category(req, cat)).await;
    }

    match handle_static_page(state, path) {
        Ok(None) => {},
        Ok(Some(page)) => return Ok(page),
        Err(err) => return Err(err),
    }

    let name = path.trim_matches('/').to_owned();
    let crates = state.crates.load();
    let (found_crate, found_keyword) = rt_run_timeout(&state.rt, "findcrate", 10, async move {
        let crate_maybe = match Origin::try_from_crates_io_name(&name) {
            Some(o) => Box::pin(crates.rich_crate_async(&o)).await.ok(),
            _ => None,
        };
        match crate_maybe {
            Some(c) => Ok((Some(c), None)),
            None => {
                let inverted_hyphens: String = name.chars().map(|c| if c == '-' {'_'} else if c == '_' {'-'} else {c.to_ascii_lowercase()}).collect();
                let crate_maybe = match Origin::try_from_crates_io_name(&inverted_hyphens) {
                    Some(o) => Box::pin(crates.rich_crate_async(&o)).await.ok(),
                    _ => None,
                };
                match crate_maybe {
                    Some(c) => Ok((Some(c), None)),
                    None => {
                        if crates.is_it_a_keyword(&inverted_hyphens).await {
                            Ok((None, Some(inverted_hyphens)))
                        } else {
                            Ok((None, None))
                        }
                    },
                }
            },
        }
    }).await?;

    if let Some(k) = found_crate {
        return Ok(HttpResponse::PermanentRedirect().insert_header(("Location", format!("/crates/{}", Encoded(k.name())))).body(""));
    }
    if let Some(keyword) = found_keyword {
        return Ok(HttpResponse::TemporaryRedirect().insert_header(("Location", format!("/keywords/{}", Encoded(&keyword)))).body(""));
    }

    render_404_page(state, path, "crate or category").await
}

fn render_404_page(state: &AServerState, path: &str, item_name: &str) -> impl Future<Output = Result<HttpResponse, ServerError>> {
    let item_name = item_name.to_owned();
    let decoded = decode(path).ok();
    let rawtext = decoded.as_deref().unwrap_or(path);

    let query = rawtext.chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).take(100).collect::<String>();
    let query = query.trim().to_owned();
    let state = state.clone();

    tokio::task::spawn_blocking(move || {
        let results = state.index.search(&query, 5, false).unwrap_or_default();
        let mut page: Vec<u8> = Vec::with_capacity(32000);
        front_end::render_404_page(&mut page, &query, &item_name, &results, &state.markup)?;
        Ok(page)
    }).map(|res| {
        res?.map(|page| {
            HttpResponse::NotFound()
                .content_type("text/html;charset=UTF-8")
                .no_chunking(page.len() as u64)
                .insert_header(("Cache-Control", "public, max-age=60, stale-while-revalidate=3600, stale-if-error=3600"))
                .body(page)
        })
    })
}

async fn handle_category(req: HttpRequest, cat: &'static Category) -> Result<HttpResponse, ServerError> {
    let state: &AServerState = req.app_data().expect("appdata");
    let crates = state.crates.load();
    Ok(serve_page(
        with_file_cache(state, &format!("_{}.html", cat.slug), 1800, {
            let state = state.clone();
            rt_run_timeout(&state.clone().rt, "catrender", 30, async move {
                let mut page: Vec<u8> = Vec::with_capacity(150000);
                front_end::render_category(&mut page, cat, &crates, &state.markup).await?;
                minify_html(&mut page);
                mark_server_still_alive(&state);
                Ok::<_, anyhow::Error>((page, None))
            })
        })
        .await?,
    ))
}

async fn handle_home(req: HttpRequest) -> Result<HttpResponse, ServerError> {
    let query = req.query_string().trim_start_matches('?');
    if !query.is_empty() && query.find('=').is_none() {
        return Ok(HttpResponse::TemporaryRedirect().insert_header(("Location", format!("/search?q={}", query))).finish());
    }

    let state: &AServerState = req.app_data().expect("appdata");
    Ok(serve_page(
        with_file_cache(state, "_.html", 3600, {
            let state = state.clone();
            run_timeout("homepage", 300, async move {
                let crates = state.crates.load();
                let mut page: Vec<u8> = Vec::with_capacity(32000);
                front_end::render_homepage(&mut page, &crates).await?;
                minify_html(&mut page);
                mark_server_still_alive(&state);
                Ok::<_, anyhow::Error>((page, Some(Utc::now().into())))
            })
        })
        .await?,
    ))
}

async fn handle_redirect(req: HttpRequest) -> HttpResponse {
    let inf = req.match_info();
    let rest = inf.query("rest");
    HttpResponse::PermanentRedirect().insert_header(("Location", format!("/{}", rest))).body("")
}

async fn handle_git_clone(req: HttpRequest) -> HttpResponse {
    let inf = req.match_info();
    let crate_name = inf.query("crate");
    if let Some(o) = Origin::try_from_crates_io_name(crate_name) {
        let state2: &AServerState = req.app_data().expect("appdata");
        let state = state2.clone();
        if let Ok(Ok(url)) = state2.rt.spawn(async move {
            let crates = state.crates.load();
            let k = crates.rich_crate_version_async(&o).await?;
            let r = k.repository().unwrap();

            let mut url = r.canonical_git_url().into_owned();
            if url.ends_with("/") {
                url.truncate(url.len() - 1);
            }
            if !url.ends_with(".git") {
                url.push_str(".git");
            }
            url.push_str("/info/refs?service=git-upload-pack");

            Ok::<_, ServerError>(url)
        }).await {
            return HttpResponse::PermanentRedirect()
                .insert_header(("X-Robots-Tag", "noindex, nofollow"))
                .insert_header(("Location", url))
                .body("");
        }
    }
    HttpResponse::NotFound().body("Crate not found")
}

async fn handle_crate_reverse_dependencies_redir(req: HttpRequest) -> HttpResponse {
    let inf = req.match_info();
    let rest = inf.query("crate");
    HttpResponse::PermanentRedirect().insert_header(("Location", format!("/crates/{}/rev", rest))).body("")
}

async fn handle_author_redirect(req: HttpRequest) -> HttpResponse {
    let inf = req.match_info();
    let rest = inf.query("author");
    HttpResponse::PermanentRedirect().insert_header(("Location", format!("/~{}", rest))).body("")
}

async fn handle_game_redirect(_: HttpRequest) -> HttpResponse {
    HttpResponse::PermanentRedirect().insert_header(("Location", "/game-development")).body("")
}

async fn handle_repo_crate(req: HttpRequest) -> Result<HttpResponse, ServerError> {
    let state: &AServerState = req.app_data().expect("appdata");
    let origin = match get_origin_from_req_match(&req) {
        Ok(res) => res,
        Err(crate_name) => return render_404_page(state, crate_name, "git crate").await,
    };

    if !state.crates.load().crate_exists(&origin) {
        let (repo, _) = origin.into_repo().expect("repohost");
        let url = repo.canonical_http_url("").expect("repohost");
        return Ok(HttpResponse::TemporaryRedirect().insert_header(("Location", url.into_owned())).finish());
    }

    Ok(serve_page(with_file_cache(state, &cache_file_name_for_origin(&origin), 86400, {
        render_crate_page(state.clone(), origin)
    }).await?))
}

fn get_origin_from_req_match(req: &HttpRequest) -> Result<Origin, &str> {
    let inf = req.match_info();
    let slug = inf.query("host");
    let owner = inf.query("owner");
    let repo = inf.query("repo");
    let crate_name = inf.query("crate");
    debug!("{} crate {}/{}/{}", slug, owner, repo, crate_name);
    if !is_alnum_dot(owner) || !is_alnum_dot(repo) || !is_alnum(crate_name) {
        return Err(crate_name);
    }

    let origin = match slug {
        "gh" => Origin::from_github(SimpleRepo::new(owner, repo), crate_name),
        "lab" => Origin::from_gitlab(SimpleRepo::new(owner, repo), crate_name),
        _ => return Err(crate_name),
    };
    Ok(origin)
}

fn get_origin_from_subpath(q: &actix_web::dev::Path<Url>) -> Option<Origin> {
    let parts = q.query("crate");
    let mut parts = parts.splitn(4, '/');
    let first = parts.next()?;
    match parts.next() {
        None => Origin::try_from_crates_io_name(first),
        Some(owner) => {
            let repo = parts.next()?;
            let package = parts.next()?;
            match first {
                "github" | "gh" => Some(Origin::from_github(SimpleRepo::new(owner, repo), package)),
                "gitlab" | "lab" => Some(Origin::from_gitlab(SimpleRepo::new(owner, repo), package)),
                _ => None,
            }
        },
    }
}

async fn handle_compat(req: HttpRequest) -> Result<HttpResponse, ServerError> {
    // if !cfg!(debug_assertions) {
    //     Err(failure::err_msg("off"))?
    // }
    let origin = get_origin_from_subpath(req.match_info()).ok_or(anyhow!("boo"))?;
    let state: &AServerState = req.app_data().expect("appdata");
    let crates = state.crates.load();
    let page = rt_run_timeout(&state.rt, "dbgcrate", 60, async move {
        let all = crates.rich_crate_async(&origin).await?;
        let mut page: Vec<u8> = Vec::with_capacity(32000);
        front_end::render_debug_page(&mut page, all, &crates).await?;
        Ok(page)
    }).await?;
    Ok(HttpResponse::Ok()
        .content_type("text/html;charset=UTF-8")
        .insert_header(("Cache-Control", "no-cache"))
        .no_chunking(page.len() as u64)
        .body(page))
}

async fn handle_install(req: HttpRequest) -> Result<HttpResponse, ServerError> {
    let state2: &AServerState = req.app_data().expect("appdata");
    let origin = if let Some(o) = get_origin_from_subpath(req.match_info()) {
        o
    } else {
        return render_404_page(state2, req.path().trim_start_matches("/install"), "crate").await;
    };

    let state = state2.clone();
    let (page, last_modified) = rt_run_timeout(&state2.rt, "instpage", 30, async move {
        let crates = state.crates.load();
        let ver = crates.rich_crate_version_async(&origin).await?;
        let mut page: Vec<u8> = Vec::with_capacity(32000);
        front_end::render_install_page(&mut page, &ver, &crates, &state.markup).await?;
        minify_html(&mut page);
        mark_server_still_alive(&state);
        Ok::<_, anyhow::Error>((page, None))
    }).await?;
    Ok(serve_page(Rendered {page, cache_time: 24 * 3600, refresh: false, last_modified}))
}

async fn handle_author(req: HttpRequest) -> Result<HttpResponse, ServerError> {
    let login = req.match_info().query("author");
    debug!("author page for {:?}", login);
    let state: &AServerState = req.app_data().expect("appdata");

    let aut = match rt_run_timeout(&state.rt, "aut1", 5, {
        let login = login.to_owned();
        let crates = state.crates.load();
        async move { crates.author_by_login(&login).await }
    }).await {
        Ok(aut) => aut,
        Err(e) => {
            debug!("user fetch {} failed: {}", login, e);
            return render_404_page(state, login, "user").await;
        }
    };
    if aut.github.login != login {
        return Ok(HttpResponse::PermanentRedirect().insert_header(("Location", format!("/~{}", Encoded(&aut.github.login)))).body(""));
    }
    let crates = state.crates.load();
    let aut2 = aut.clone();
    let rows = rt_run_timeout(&state.rt, "authorpage1", 60, async move { crates.crates_of_author(&aut2).await }).await?;
    if rows.is_empty() {
        return Ok(HttpResponse::TemporaryRedirect().insert_header(("Location", format!("https://github.com/{}", Encoded(&aut.github.login)))).body(""));
    }
    Ok(serve_page(
        with_file_cache(state, &format!("@{}.html", login), 3600, {
            let state = state.clone();
            run_timeout("authorpage", 60, async move {
                let crates = state.crates.load();
                let mut page: Vec<u8> = Vec::with_capacity(32000);
                front_end::render_author_page(&mut page, rows, &aut, &crates, &state.markup).await?;
                minify_html(&mut page);
                mark_server_still_alive(&state);
                Ok::<_, anyhow::Error>((page, None))
            })
        })
        .await?,
    ))
}

async fn handle_maintainer_dashboard_html(req: HttpRequest) -> Result<HttpResponse, ServerError> {
    handle_maintainer_dashboard(req, false).await
}

async fn handle_maintainer_dashboard_xml(req: HttpRequest) -> Result<HttpResponse, ServerError> {
    handle_maintainer_dashboard(req, true).await
}

async fn handle_maintainer_dashboard(req: HttpRequest, atom_feed: bool) -> Result<HttpResponse, ServerError> {
    let login = req.match_info().query("author");
    debug!("maintainer_dashboard for {:?}", login);
    let state: &AServerState = req.app_data().expect("appdata");

    let aut = match rt_run_timeout(&state.rt, "aut1", 5, {
        let login = login.to_owned();
        let crates = state.crates.load();
        async move { crates.author_by_login(&login).await }
    }).await {
        Ok(aut) => aut,
        Err(e) => {
            debug!("user fetch {} failed: {}", login, e);
            return render_404_page(state, login, "user").await;
        }
    };
    if aut.github.login != login && !atom_feed {
        return Ok(HttpResponse::PermanentRedirect().insert_header(("Location", format!("/~{}/dash", Encoded(&aut.github.login)))).body(""));
    }

    let crates = state.crates.load();
    if crates.author_shitlist.get(&aut.github.login.to_ascii_lowercase()).is_some() {
        return Ok(HttpResponse::TemporaryRedirect().insert_header(("Location", format!("/~{}", Encoded(&aut.github.login)))).body(""));
    }

    let aut2 = aut.clone();
    let rows = rt_run_timeout(&state.rt, "maintainer_dashboard1", 60, async move { crates.crates_of_author(&aut2).await }).await?;

    if rows.is_empty() {
        return Ok(HttpResponse::TemporaryRedirect().insert_header(("Location", format!("/~{}", Encoded(&aut.github.login)))).body(""));
    }

    let cache_file_name = format!("@{}.dash{}.html", login, if atom_feed { "xml" } else { "" });
    let rendered = with_file_cache(state, &cache_file_name, if atom_feed { 3 * 3600 } else { 600 }, {
        let state = state.clone();
        run_timeout("maintainer_dashboard2", 60, async move {
            let crates = state.crates.load();
            let mut page: Vec<u8> = Vec::with_capacity(32000);
            front_end::render_maintainer_dashboard(&mut page, atom_feed, rows, &aut, &crates, &state.markup).await?;
            if !atom_feed { minify_html(&mut page); }
            mark_server_still_alive(&state);
            Ok::<_, anyhow::Error>((page, None))
        })
    })
    .await?;
    if !atom_feed {
        Ok(serve_page(rendered))
    } else {
        Ok(serve_feed(rendered))
    }
}

fn cache_file_name_for_origin(origin: &Origin) -> String {
    match origin {
        Origin::CratesIo(crate_name) => {
            assert!(!crate_name.as_bytes().iter().any(|&b| b == b'/' || b == b'.'));
            format!("{}.html", crate_name)
        },
        Origin::GitHub { repo, package } | Origin::GitLab { repo, package } => {
            assert!(!package.as_bytes().iter().any(|&b| b == b'/' || b == b'.'));
            let slug = if let Origin::GitHub {..} = origin { "gh" } else { "lab" };
            format!("{},{},{},{}.html", slug, repo.owner, repo.repo, package)
        }
    }
}

async fn handle_crate(req: HttpRequest) -> Result<HttpResponse, ServerError> {
    let crate_name = req.match_info().query("crate");
    debug!("crate page for {:?}", crate_name);
    let state: &AServerState = req.app_data().expect("appdata");
    let crates = state.crates.load();
    let origin = match Origin::try_from_crates_io_name(crate_name).filter(|o| crates.crate_exists(o)) {
        Some(o) => o,
        None => return render_404_page(state, crate_name, "crate").await,
    };
    Ok(serve_page(with_file_cache(state, &cache_file_name_for_origin(&origin), 600, {
        render_crate_page(state.clone(), origin)
    }).await?))
}


async fn handle_repo_crate_all_versions(req: HttpRequest) -> Result<HttpResponse, ServerError> {
    let state: &AServerState = req.app_data().expect("appdata");
    let origin = match get_origin_from_req_match(&req) {
        Ok(res) => res,
        Err(crate_name) => return render_404_page(state, crate_name, "git crate").await,
    };

    Ok(serve_page(render_crate_all_versions(state.clone(), origin).await?))
}

async fn handle_crate_all_versions(req: HttpRequest) -> Result<HttpResponse, ServerError> {
    let crate_name = req.match_info().query("crate");
    debug!("allver for {:?}", crate_name);
    let state: &AServerState = req.app_data().expect("appdata");
    let crates = state.crates.load();
    let origin = match Origin::try_from_crates_io_name(crate_name).filter(|o| crates.crate_exists(o)) {
        Some(o) => o,
        None => return render_404_page(state, crate_name, "git crate").await,
    };
    Ok(serve_page(render_crate_all_versions(state.clone(), origin).await?))
}

async fn handle_crate_reverse_dependencies(req: HttpRequest) -> Result<HttpResponse, ServerError> {
    let crate_name = req.match_info().query("crate");
    debug!("rev deps for {:?}", crate_name);
    let state: &AServerState = req.app_data().expect("appdata");
    let crates = state.crates.load();
    let origin = match Origin::try_from_crates_io_name(crate_name).filter(|o| crates.crate_exists(o)) {
        Some(o) => o,
        None => return render_404_page(state, crate_name, "crate").await,
    };
    Ok(serve_page(render_crate_reverse_dependencies(state.clone(), origin).await?))
}

async fn handle_crate_reviews(req: HttpRequest) -> Result<HttpResponse, ServerError> {
    let crate_name = req.match_info().query("crate");
    debug!("crev for {:?}", crate_name);
    let state: &AServerState = req.app_data().expect("appdata");
    let crates = state.crates.load();
    let origin = match Origin::try_from_crates_io_name(crate_name).filter(|o| crates.crate_exists(o)) {
        Some(o) => o,
        None => return render_404_page(state, crate_name, "crate").await,
    };
    let state = state.clone();
    Ok(serve_page(
        rt_run_timeout(&state.clone().rt, "revpage", 30, async move {
            let crates = state.crates.load();
            let ver = crates.rich_crate_version_async(&origin).await?;
            let mut page: Vec<u8> = Vec::with_capacity(32000);
            let reviews = crates.reviews_for_crate(ver.origin());
            front_end::render_crate_reviews(&mut page, &reviews, &ver, &crates, &state.markup).await?;
            minify_html(&mut page);
            mark_server_still_alive(&state);
            Ok::<_, anyhow::Error>(Rendered {page, cache_time: 24 * 3600, refresh: false, last_modified: None})
        })
        .await?,
    ))
}

async fn handle_new_trending(req: HttpRequest) -> Result<HttpResponse, ServerError> {
    let state: &AServerState = req.app_data().expect("appdata");
    Ok(serve_page(
        with_file_cache(state, "_new_.html", 600, {
            let state = state.clone();
            run_timeout("trendpage", 60, async move {
                let crates = state.crates.load();
                let mut page: Vec<u8> = Vec::with_capacity(32000);
                front_end::render_trending_crates(&mut page, &crates, &state.markup).await?;
                minify_html(&mut page);
                Ok::<_, anyhow::Error>((page, None))
            })
        })
        .await?,
    ))
}

async fn handle_global_stats(req: HttpRequest) -> Result<HttpResponse, ServerError> {
    let state: &AServerState = req.app_data().expect("appdata");
    Ok(serve_page(
        with_file_cache(state, "_stats_.html", 6 * 3600, {
            let state = state.clone();
            run_timeout("trendpage", 90, async move {
                let crates = state.crates.load();
                let mut page: Vec<u8> = Vec::with_capacity(32000);
                front_end::render_global_stats(&mut page, &crates, &state.markup).await?;
                minify_html(&mut page);
                Ok::<_, anyhow::Error>((page, None))
            })
        })
        .await?,
    ))
}

const CACHE_MAGIC_TAG: &[u8; 4] = b"  <c";

/// takes path to storage, freshness in seconds, and a function to call on cache miss
/// returns (page, fresh in seconds)
async fn with_file_cache<F: Send>(state: &AServerState, cache_file_name: &str, cache_time: u32, generate: F) -> Result<Rendered, anyhow::Error>
    where F: Future<Output=Result<(Vec<u8>, Option<DateTime<FixedOffset>>), anyhow::Error>> + 'static {
    let cache_file = state.page_cache_dir.join(cache_file_name);
    if let Ok(modified) = std::fs::metadata(&cache_file).and_then(|m| m.modified()) {
        let now = SystemTime::now();
        let cache_time = cache_time as u64;
        // rebuild in debug always
        let is_fresh = !cfg!(debug_assertions) && modified > (now - Duration::from_secs(cache_time / 20 + 5));
        let is_acceptable = modified > (now - Duration::from_secs(3600 * 24 * 2 + cache_time * 8));

        let age_secs = now.duration_since(modified).ok().map(|age| age.as_secs()).unwrap_or(0);

        if let Ok(mut page_cached) = std::fs::read(&cache_file) {
            if !is_acceptable || page_cached.len() <= 8 {
                let _ = std::fs::remove_file(&cache_file); // next req will block instead of an endless refresh loop
            }
            assert!(page_cached.len() > 8);

            let trailer_pos = page_cached.len() - 8; // The worst data format :)
            if page_cached[trailer_pos .. trailer_pos + 4] != CACHE_MAGIC_TAG[..] {
                let _ = std::fs::remove_file(&cache_file);
            }
            let timestamp = u32::from_le_bytes(page_cached.get(trailer_pos + 4..).unwrap().try_into().unwrap());
            page_cached.truncate(trailer_pos);

            let last_modified = if timestamp > 0 { Some(DateTime::from_utc(NaiveDateTime::from_timestamp(timestamp as _, 0), FixedOffset::east(0))) } else { None };
            let cache_time_remaining = cache_time.saturating_sub(age_secs);

            debug!("Using cached page {} {}s fresh={:?} acc={:?}", cache_file.display(), cache_time_remaining, is_fresh, is_acceptable);

            if !is_fresh {
                background_refresh(state.clone(), cache_file, generate);
            }
            return Ok(Rendered {
                page: page_cached,
                cache_time: if !is_fresh { cache_time_remaining / 4 } else { cache_time_remaining }.max(2) as u32,
                refresh: !is_acceptable,
                last_modified,
            });
        }

        debug!("Cache miss {} {}", cache_file.display(), age_secs);
    } else {
        debug!("Cache miss {} no file", cache_file.display());
    }

    let (mut page, last_modified) = state.rt.spawn({
        let state = state.clone();
        async move {
            let _s = tokio::time::timeout(Duration::from_secs(10), state.foreground_job.acquire()).await?;
            generate.await
        }}).await??;

    let timestamp = last_modified.map(|a| a.timestamp() as u32).unwrap_or(0);
    page.extend_from_slice(&CACHE_MAGIC_TAG[..]); // The worst data format :)
    page.extend_from_slice(&timestamp.to_le_bytes()); // The worst data format :)
    if let Err(e) = std::fs::write(&cache_file, &page) {
        error!("warning: Failed writing to {}: {}", cache_file.display(), e);
    }
    page.truncate(page.len() - 8);

    Ok(Rendered {page, cache_time, refresh: false, last_modified})
}

fn background_refresh<F>(state: AServerState, cache_file: PathBuf, generate: F)
    where F: Future<Output=Result<(Vec<u8>, Option<DateTime<FixedOffset>>), anyhow::Error>> + 'static + Send {

    let _ = rt_run_timeout(&state.clone().rt, "refreshbg", 300, {
        async move {
            let start = Instant::now();
            debug!("Bg refresh of {}", cache_file.display());
            if let Ok(_s) = state.background_job.try_acquire() {
                match generate.await {
                    Ok((mut page, last_mod)) => {
                        info!("Done refresh of {} in {}ms", cache_file.display(), start.elapsed().as_millis() as u32);
                        let timestamp = last_mod.map(|a| a.timestamp() as u32).unwrap_or(0);
                        page.extend_from_slice(&CACHE_MAGIC_TAG[..]); // The worst data format :)
                        page.extend_from_slice(&timestamp.to_le_bytes());

                        if let Err(e) = std::fs::write(&cache_file, &page) {
                            error!("warning: Failed writing to {}: {}", cache_file.display(), e);
                        }
                    }
                    Err(e) => {
                        error!("Refresh err: {} {}", e.chain().map(|e| e.to_string()).collect::<Vec<_>>().join("; "), cache_file.display());
                    }
                }
            } else {
                info!("Too busy to refresh {}", cache_file.display());
            }
            Ok(())
        }
    });
}

fn render_crate_page(state: AServerState, origin: Origin) -> impl Future<Output = Result<(Vec<u8>, Option<DateTime<FixedOffset>>), anyhow::Error>> + 'static {
    run_timeout("cratepage", 30, async move {
        let crates = state.crates.load();
        let (all, ver) = futures::try_join!(crates.rich_crate_async(&origin), crates.rich_crate_version_async(&origin))?;
        let mut page: Vec<u8> = Vec::with_capacity(32000);
        let last_mod = Box::pin(front_end::render_crate_page(&mut page, &all, &ver, &crates, &state.markup)).await?;
        minify_html(&mut page);
        mark_server_still_alive(&state);
        Ok::<_, anyhow::Error>((page, last_mod))
    })
}

async fn render_crate_all_versions(state: AServerState, origin: Origin) -> Result<Rendered, anyhow::Error> {
    let s = state.clone();
    rt_run_timeout(&s.rt, "allver", 60, async move {
        let crates = state.crates.load();
        let (all, ver) = futures::try_join!(crates.rich_crate_async(&origin), crates.rich_crate_version_async(&origin))?;
        let last_modified = Some(get_last_modified(&all));
        let mut page: Vec<u8> = Vec::with_capacity(60000);
        front_end::render_all_versions_page(&mut page, all, &ver, &crates).await?;
        minify_html(&mut page);
        mark_server_still_alive(&state);
        Ok::<_, anyhow::Error>(Rendered {page, cache_time: 24*3600, refresh: false, last_modified})
    }).await
}

async fn render_crate_reverse_dependencies(state: AServerState, origin: Origin) -> Result<Rendered, anyhow::Error> {
    let s = state.clone();
    rt_run_timeout(&s.rt, "revpage2", 30, async move {
        let crates = state.crates.load();
        let ver = crates.rich_crate_version_async(&origin).await?;
        let mut page: Vec<u8> = Vec::with_capacity(32000);
        front_end::render_crate_reverse_dependencies(&mut page, &ver, &crates, &state.markup).await?;
        minify_html(&mut page);
        mark_server_still_alive(&state);
        Ok::<_, anyhow::Error>(Rendered {page, cache_time: 24*3600, refresh: false, last_modified: None})
    }).await
}

async fn handle_keyword(req: HttpRequest) -> Result<HttpResponse, ServerError> {
    let q = req.match_info().query("keyword");
    if q.is_empty() {
        return Ok(HttpResponse::TemporaryRedirect().insert_header(("Location", "/")).finish());
    }

    let query = q.to_owned();
    let state: &AServerState = req.app_data().expect("appdata");
    let state2 = state.clone();
    let (query, page) = tokio::task::spawn_blocking(move || {
        if !is_alnum(&query) {
            return Ok::<_, anyhow::Error>((query, None));
        }
        let keyword_query = format!("keywords:\"{}\"", query);
        let results = state2.index.search(&keyword_query, 150, false)?;
        if !results.is_empty() {
            let mut page: Vec<u8> = Vec::with_capacity(32000);
            front_end::render_keyword_page(&mut page, &query, &results, &state2.markup)?;
            minify_html(&mut page);
            Ok((query, Some(page)))
        } else {
            Ok((query, None))
        }
    }).await??;

    Ok(if let Some(page) = page {
        HttpResponse::Ok()
            .content_type("text/html;charset=UTF-8")
            .insert_header(("Cache-Control", "public, max-age=172800, stale-while-revalidate=604800, stale-if-error=86400"))
            .no_chunking(page.len() as u64)
            .body(page)
    } else {
        HttpResponse::TemporaryRedirect().insert_header(("Location", format!("/search?q={}", Encoded(&query)))).finish()
    })
}

#[derive(Debug)]
struct Rendered {
    page: Vec<u8>,
    // s
    cache_time: u32,
    refresh: bool,
    last_modified: Option<DateTime<FixedOffset>>,
}

fn serve_page(Rendered {page, cache_time, refresh, last_modified}: Rendered) -> HttpResponse {
    let err_max = (cache_time * 10).max(3600 * 24 * 2);

    let last_modified_secs = last_modified.map(|l| Utc::now().signed_duration_since(l).num_seconds().max(0) as u32).unwrap_or(0);
    // if no updates for a year, don't expect more, and keep old page cached for longer
    let extra_time = if last_modified_secs < 3600*24*365 { last_modified_secs / 300 } else { last_modified_secs / 40 };
    let cache_time = cache_time.max(extra_time);

    // last-modified is ambiguous, because it's modification of the content, not the whole state
    let mut hasher = blake3::Hasher::new();
    hasher.update(if refresh { b"1" } else { b"0" });
    hasher.update(&page);
    let etag = format!("\"{:.16}\"", base64::encode(hasher.finalize().as_bytes()));

    let mut h = HttpResponse::Ok();
    h.content_type("text/html;charset=UTF-8");
    h.insert_header(("etag", etag));
    if !refresh {
        h.insert_header(("Cache-Control", format!("public, max-age={}, stale-while-revalidate={}, stale-if-error={}", cache_time, cache_time * 3, err_max)));
    }
    if refresh {
        h.insert_header(("Refresh", "5"));
        h.insert_header(("Cache-Control", "no-cache, s-maxage=4, must-revalidate"));
    }
    if let Some(l) = last_modified {
        // can't give validator, because then 304 leaves refresh
        if !refresh {
            h.insert_header(("Last-Modified", l.to_rfc2822()));
        }
    }
    h.no_chunking(page.len() as u64).body(page)
}

fn serve_feed(Rendered {page, cache_time, refresh, last_modified}: Rendered) -> HttpResponse {
    // last-modified is ambiguous, because it's modification of the content, not the whole state
    let mut hasher = blake3::Hasher::new();
    hasher.update(if refresh { b"1" } else { b"0" });
    hasher.update(&page);
    let etag = format!("\"{:.16}\"", base64::encode(hasher.finalize().as_bytes()));

    let mut h = HttpResponse::Ok();
    h.content_type("application/atom+xml;charset=UTF-8");
    h.insert_header(("etag", etag));
    if !refresh {
        h.insert_header(("Cache-Control", format!("public, max-age={}", cache_time)));
        if let Some(l) = last_modified {
            h.insert_header(("Last-Modified", l.to_rfc2822()));
        }
    }
    h.no_chunking(page.len() as u64).body(page)
}

fn is_alnum(q: &str) -> bool {
    q.as_bytes().iter().copied().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}

fn is_alnum_dot(q: &str) -> bool {
    let mut chars = q.as_bytes().iter().copied();
    if !chars.next().map_or(false, |first| first.is_ascii_alphanumeric() || first == b'_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-' || c == b'.')
}

async fn handle_search(req: HttpRequest) -> Result<HttpResponse, ServerError> {
    let qs = req.query_string().replace('+', "%20");
    let qs = qstring::QString::from(qs.as_str());
    match qs.get("q").unwrap_or("") {
        q if !q.trim_start().is_empty() => {
            let state: &AServerState = req.app_data().expect("appdata");
            let query = q.to_owned();
            let page = tokio::task::spawn_blocking({
                let state = state.clone();
                move || {
                    let results = state.index.search(&query, 50, true)?;
                    let mut page = Vec::with_capacity(32000);
                    front_end::render_serp_page(&mut page, &query, &results, &state.markup)?;
                    minify_html(&mut page);
                    Ok::<_, anyhow::Error>(Rendered {page, cache_time: 600, refresh: false, last_modified: None})
                }
            }).await??;
            Ok(serve_page(page))
        },
        _ => Ok(HttpResponse::PermanentRedirect().insert_header(("Location", "/")).finish()),
    }
}

async fn handle_sitemap(req: HttpRequest) -> Result<HttpResponse, ServerError> {
    let (w, page) = writer::<_, ServerError>().await;
    let state: &AServerState = req.app_data().expect("appdata");
    let _ = state.rt.spawn({
        let state = state.clone();
        async move {
            let mut w = std::io::BufWriter::with_capacity(16000, w);
            let crates = state.crates.load();
            if let Err(e) = front_end::render_sitemap(&mut w, &crates).await {
                if let Ok(mut w) = w.into_inner() {
                    w.fail(e.into());
                }
            }
        }
    });
    Ok(HttpResponse::Ok()
        .content_type("application/xml;charset=UTF-8")
        .insert_header(("Cache-Control", "public, max-age=259200, stale-while-revalidate=72000, stale-if-error=72000"))
        .streaming::<_, ServerError>(page))
}

async fn handle_feed(req: HttpRequest) -> Result<HttpResponse, ServerError> {
    let state: &AServerState = req.app_data().expect("appdata");
    let state2 = state.clone();
    let page = rt_run_timeout(&state.rt, "feed", 60, async move {
        let crates = state2.crates.load();
        let mut page: Vec<u8> = Vec::with_capacity(32000);
        front_end::render_feed(&mut page, &crates).await?;
        Ok::<_, anyhow::Error>(page)
    }).await?;
    Ok(HttpResponse::Ok()
        .content_type("application/atom+xml;charset=UTF-8")
        .insert_header(("Cache-Control", "public, max-age=10800, stale-while-revalidate=259200, stale-if-error=72000"))
        .no_chunking(page.len() as u64)
        .body(page))
}

fn run_timeout<R, T: 'static + Send>(label: &'static str, secs: u64, fut: R) -> Pin<Box<dyn Future<Output = Result<T, anyhow::Error>> + Send>>
where
    R: 'static + Send + Future<Output = Result<T, anyhow::Error>>,
{
    let fut = kitchen_sink::NonBlock::new(label, tokio::time::timeout(Duration::from_secs(secs), fut));
    let timeout = fut.map(move |res| {
        res.map_err(|_| anyhow!("{} timed out after >{}s", label, secs))?
    });
    Box::pin(timeout)
}

fn rt_run_timeout<R, T: 'static + Send>(rt: &Handle, label: &'static str, secs: u64, fut: R) -> impl Future<Output=Result<T, anyhow::Error>> + 'static + Send
where
    R: 'static + Send + Future<Output = Result<T, anyhow::Error>>,
{
    rt.spawn(kitchen_sink::NonBlock::new(label, tokio::time::timeout(Duration::from_secs(secs), fut)))
    .map(move |res| -> Result<T, anyhow::Error> {
        res?.map_err(|_| anyhow!("{} timed out after >{}s", label, secs))?
    })
}

struct ServerError {
    pub(crate) err: anyhow::Error,
}

impl ServerError {
    pub fn new(err: anyhow::Error) -> Self {
        for cause in err.chain() {
            error!("• {}", cause);
            // The server is stuck and useless
            let s = cause.to_string();
            if s.contains("Too many open files") || s.contains("instance has previously been poisoned") ||
               s.contains("inconsistent park state") || s.contains("failed to allocate an alternative stack") {
                error!("Fatal error: {}", s);
                std::process::exit(2);
            }
        }
        Self { err }
    }
}

impl From<anyhow::Error> for ServerError {
    fn from(err: anyhow::Error) -> Self {
        Self::new(err)
    }
}

impl From<tokio::task::JoinError> for ServerError {
    fn from(err: tokio::task::JoinError) -> Self {
        Self::new(err.into())
    }
}

use std::fmt;
impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.err.fmt(f)
    }
}
impl fmt::Debug for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.err.fmt(f)
    }
}
impl std::error::Error for ServerError {
}

impl actix_web::ResponseError for ServerError {
    fn status_code(&self) -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }

    fn error_response(&self) -> HttpResponse<BoxBody> {
        let mut page = Vec::with_capacity(20000);
        front_end::render_error(&mut page, &self.err);
        HttpResponse::InternalServerError()
            .content_type("text/html;charset=UTF-8")
            .no_chunking(page.len() as u64)
            .body(page)
    }
}

fn minify_html(page: &mut Vec<u8>) {
    let mut m = html_minifier::HTMLMinifier::new();
    // digest wants bytes anyway
    if let Ok(()) = m.digest(&page) {
        let out = m.get_html();
        page.clear();
        page.extend_from_slice(out);
    }
}

fn get_last_modified(c: &RichCrate) -> DateTime<FixedOffset> {
    DateTime::parse_from_rfc3339(c.most_recent_release_date_str()).unwrap()
}
