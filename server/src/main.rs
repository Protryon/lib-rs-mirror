use tokio::runtime::Handle;
use actix_web::body::Body;
use actix_web::dev::Url;
use actix_web::http::header::HeaderValue;
use actix_web::http::StatusCode;
use actix_web::HttpResponse;
use actix_web::middleware;
use actix_web::{web, App, HttpRequest, HttpServer};
use arc_swap::ArcSwap;
use cap::Cap;
use categories::CATEGORIES;
use categories::Category;
use chrono::prelude::*;
use crate::writer::*;
use env_logger;
use failure::ResultExt;
use front_end;
use futures::future::Future;
use futures::future::FutureExt;
use kitchen_sink::filter::ImageOptimAPIFilter;
use kitchen_sink::KitchenSink;
use kitchen_sink::Origin;
use kitchen_sink;
use locale::Numeric;
use render_readme::{Highlighter, Markup, Renderer};
use repo_url::SimpleRepo;
use search_index::CrateSearchIndex;
use std::convert::TryInto;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant, SystemTime};
use urlencoding::decode;
use urlencoding::encode;

mod writer;

#[global_allocator]
static ALLOCATOR: Cap<std::alloc::System> = Cap::new(std::alloc::System, 1 * 1024 * 1024 * 1024);

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
    let mut sys = actix_rt::System::new("actix-server");

    let rt = tokio::runtime::Builder::new()
        .threaded_scheduler()
        .enable_all()
        .core_threads(2)
        .max_threads(8)
        .thread_name("server-bg")
        .build()
        .unwrap();

    let res = sys.block_on(run_server(rt.handle().clone()));

    rt.shutdown_timeout(Duration::from_secs(1));

    if let Err(e) = res {
        for c in e.iter_chain() {
            eprintln!("Error: {}", c);
        }
        std::process::exit(1);
    }
}

async fn run_server(rt: Handle) -> Result<(), failure::Error> {
    unsafe {
        signal_hook::register(signal_hook::SIGHUP, || HUP_SIGNAL.store(1, Ordering::SeqCst))
    }?;
    unsafe {
        signal_hook::register(signal_hook::SIGUSR1, || HUP_SIGNAL.store(1, Ordering::SeqCst))
    }?;

    env_logger::init();
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
        background_job: tokio::sync::Semaphore::new(4),
        foreground_job: tokio::sync::Semaphore::new(32),
        start_time: Instant::now(),
        last_ok_response: AtomicU32::new(0),
    });

    let timestamp = Arc::new(AtomicU32::new(0));

    // refresher thread
    state.rt.spawn({
        let state = state.clone();
        let timestamp = timestamp.clone();
        async move {
            let mut last_reload = Instant::now();
            state.crates.load().prewarm().await;
            loop {
                tokio::time::delay_for(Duration::from_secs(1)).await;
                let elapsed = state.start_time.elapsed().as_secs() as u32;
                timestamp.store(elapsed, Ordering::SeqCst);
                let should_reload = if 1 == HUP_SIGNAL.swap(0, Ordering::SeqCst) {
                    println!("HUP!");
                    true
                } else if last_reload.elapsed() > Duration::from_secs(30*60) {
                    println!("Reloading state on a timer");
                    true
                } else {
                    false
                };
                if should_reload {
                    last_reload = Instant::now();
                    match KitchenSink::new(&data_dir, &github_token).await {
                        Ok(k) => {
                            state.crates.load().cleanup();
                            let k = Arc::new(k);
                            let _ = tokio::task::spawn({
                                let k = k.clone();
                                async move {
                                    k.update().await
                                }
                            }).await;
                            state.crates.store(k);
                            println!("Reloaded state");
                            state.crates.load().prewarm().await;
                        },
                        Err(e) => {
                            eprintln!("Refresh failed: {}", e);
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
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let expected = state.start_time.elapsed().as_secs() as u32;
            let rt_timestamp = timestamp.load(Ordering::SeqCst);
            let response_timestamp = state.last_ok_response.load(Ordering::SeqCst);
            if rt_timestamp > expected + 2 {
                eprintln!("Update loop is {}s behind", rt_timestamp - expected);
                if rt_timestamp - expected > 10 {
                    eprintln!("tokio is dead");
                    std::process::exit(1);
                }
            }
            if response_timestamp > expected + 60*5 {
                eprintln!("no requests for 5 minutes? probably a deadlock");
                std::process::exit(2);
            }
        }
    }});

    let server = HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .wrap(middleware::Compress::default())
            .wrap(middleware::DefaultHeaders::new().header("x-powered-by", HeaderValue::from_static(concat!("actix-web/2 lib.rs/", env!("CARGO_PKG_VERSION")))))
            .wrap(middleware::Logger::default())
            .route("/", web::get().to(handle_home))
            .route("/search", web::get().to(handle_search))
            .route("/game-engines", web::get().to(handle_game_redirect))
            .route("/index", web::get().to(handle_search)) // old crates.rs/index url
            .route("/categories/{rest:.*}", web::get().to(handle_redirect))
            .route("/new", web::get().to(handle_new_trending))
            .route("/keywords/{keyword}", web::get().to(handle_keyword))
            .route("/crates/{crate}", web::get().to(handle_crate))
            .route("/crates/{crate}/rev", web::get().to(handle_crate_reverse_dependencies))
            .route("/crates/{crate}/reverse_dependencies", web::get().to(handle_crate_reverse_dependencies_redir))
            .route("/crates/{crate}/crev", web::get().to(handle_crate_reviews))
            .route("/~{author}", web::get().to(handle_author))
            .route("/users/{author}", web::get().to(handle_author_redirect))
            .route("/install/{crate:.*}", web::get().to(handle_install))
            .route("/debug/{crate:.*}", web::get().to(handle_debug))
            .route("/gh/{owner}/{repo}/{crate}", web::get().to(handle_github_crate))
            .route("/lab/{owner}/{repo}/{crate}", web::get().to(handle_gitlab_crate))
            .route("/atom.xml", web::get().to(handle_feed))
            .route("/sitemap.xml", web::get().to(handle_sitemap))
            .service(actix_files::Files::new("/", &public_document_root))
            .default_service(web::route().to(default_handler))
    })
    .bind("127.0.0.1:32531")
    .expect("Can not bind to 127.0.0.1:32531")
    .shutdown_timeout(1);

    println!("Starting HTTP server {} on http://127.0.0.1:32531", env!("CARGO_PKG_VERSION"));
    server.run().await?;

    println!("bye!");
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

    mark_server_still_alive(&state);
    Ok(Some(HttpResponse::Ok()
        .content_type("text/html;charset=UTF-8")
        .header("Cache-Control", "public, max-age=7200, stale-while-revalidate=604800, stale-if-error=86400")
        .content_length(page.len() as u64)
        .body(page)))
}

async fn default_handler(req: HttpRequest) -> Result<HttpResponse, ServerError> {
    let state: &AServerState = req.app_data().expect("appdata");
    let path = req.uri().path();
    assert!(path.starts_with('/'));
    if path.ends_with('/') {
        return Ok(HttpResponse::PermanentRedirect().header("Location", path.trim_end_matches('/')).body(""));
    }

    if let Some(cat) = find_category(path.split('/').skip(1)) {
        return handle_category(req, cat).await;
    }

    match handle_static_page(state, path) {
        Ok(None) => {},
        Ok(Some(page)) => return Ok(page),
        Err(err) => return Err(err),
    }

    let name = path.trim_matches('/').to_owned();
    let crates = state.crates.load();
    let (found_crate, found_keyword) = rt_run_timeout(&state.rt, 10, async move {
        let crate_maybe = match Origin::try_from_crates_io_name(&name) {
            Some(o) => crates.rich_crate_async(&o).await.ok(),
            _ => None,
        };
        match crate_maybe {
            Some(c) => Ok((Some(c), None)),
            None => {
                let inverted_hyphens: String = name.chars().map(|c| if c == '-' {'_'} else if c == '_' {'-'} else {c.to_ascii_lowercase()}).collect();
                let crate_maybe = match Origin::try_from_crates_io_name(&inverted_hyphens) {
                    Some(o) => crates.rich_crate_async(&o).await.ok(),
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
        return Ok(HttpResponse::PermanentRedirect().header("Location", format!("/crates/{}", encode(k.name()))).body(""));
    }
    if let Some(keyword) = found_keyword {
        return Ok(HttpResponse::TemporaryRedirect().header("Location", format!("/keywords/{}", encode(&keyword))).body(""));
    }

    render_404_page(state, path, "crate or category")
}

fn render_404_page(state: &AServerState, path: &str, item_name: &str) -> Result<HttpResponse, ServerError> {
    let decoded = decode(path).ok();
    let rawtext = decoded.as_ref().map(|d| d.as_str()).unwrap_or(path);

    let query = rawtext.chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).take(100).collect::<String>();
    let query = query.trim();
    let results = state.index.search(query, 5, false).unwrap_or_default();
    let mut page: Vec<u8> = Vec::with_capacity(32000);
    front_end::render_404_page(&mut page, query, item_name, &results, &state.markup)?;

    Ok(HttpResponse::NotFound()
        .content_type("text/html;charset=UTF-8")
        .content_length(page.len() as u64)
        .header("Cache-Control", "public, max-age=60, stale-while-revalidate=3600, stale-if-error=3600")
        .body(page))
}

async fn handle_category(req: HttpRequest, cat: &'static Category) -> Result<HttpResponse, ServerError> {
    let state: &AServerState = req.app_data().expect("appdata");
    let crates = state.crates.load();
    let cache_file = state.page_cache_dir.join(format!("_{}.html", cat.slug));
    Ok(serve_cached(with_file_cache(state, cache_file, 1800, {
        let state = state.clone();
        run_timeout(30, async move {
            let mut page: Vec<u8> = Vec::with_capacity(150000);
            front_end::render_category(&mut page, cat, &crates, &state.markup).await?;
            minify_html(&mut page);
            mark_server_still_alive(&state);
            Ok::<_, failure::Error>((page, None))
        })
    }).await?))
}

async fn handle_home(req: HttpRequest) -> Result<HttpResponse, ServerError> {
    println!("home route");
    let query = req.query_string().trim_start_matches('?');
    if !query.is_empty() && query.find('=').is_none() {
        return Ok(HttpResponse::TemporaryRedirect().header("Location", format!("/search?q={}", query)).finish());
    }

    let state: &AServerState = req.app_data().expect("appdata");
    let cache_file = state.page_cache_dir.join("_.html");
    Ok(serve_cached(with_file_cache(&state, cache_file, 3600, {
        let state = state.clone();
        run_timeout(300, async move {
            let crates = state.crates.load();
            let mut page: Vec<u8> = Vec::with_capacity(32000);
            front_end::render_homepage(&mut page, &crates).await?;
            minify_html(&mut page);
            mark_server_still_alive(&state);
            Ok::<_, failure::Error>((page, Some(Utc::now().into())))
        })
    }).await?))
}

async fn handle_github_crate(req: HttpRequest) -> Result<HttpResponse, ServerError> {
    handle_git_crate(req, "gh").await
}
async fn handle_gitlab_crate(req: HttpRequest) -> Result<HttpResponse, ServerError> {
    handle_git_crate(req, "lab").await
}

async fn handle_redirect(req: HttpRequest) -> HttpResponse {
    let inf = req.match_info();
    let rest = inf.query("rest");
    HttpResponse::PermanentRedirect().header("Location", format!("/{}", rest)).body("")
}

async fn handle_crate_reverse_dependencies_redir(req: HttpRequest) -> HttpResponse {
    let inf = req.match_info();
    let rest = inf.query("crate");
    HttpResponse::PermanentRedirect().header("Location", format!("/crates/{}/rev", rest)).body("")
}

async fn handle_author_redirect(req: HttpRequest) -> HttpResponse {
    let inf = req.match_info();
    let rest = inf.query("author");
    HttpResponse::PermanentRedirect().header("Location", format!("/~{}", rest)).body("")
}

async fn handle_game_redirect(_: HttpRequest) -> HttpResponse {
    HttpResponse::PermanentRedirect().header("Location", "/game-development").body("")
}

async fn handle_git_crate(req: HttpRequest, slug: &'static str) -> Result<HttpResponse, ServerError> {
    let inf = req.match_info();
    let state: &AServerState = req.app_data().expect("appdata");
    let owner = inf.query("owner");
    let repo = inf.query("repo");
    let crate_name = inf.query("crate");
    println!("{} crate {}/{}/{}", slug, owner, repo, crate_name);
    if !is_alnum_dot(&owner) || !is_alnum_dot(&repo) || !is_alnum(&crate_name) {
        return render_404_page(state, &crate_name, "git crate");
    }

    let cache_file = state.page_cache_dir.join(format!("{},{},{},{}.html", slug, owner, repo, crate_name));
    let origin = match slug {
        "gh" => Origin::from_github(SimpleRepo::new(owner, repo), crate_name),
        _ => Origin::from_gitlab(SimpleRepo::new(owner, repo), crate_name),
    };
    if !state.crates.load().crate_exists(&origin) {
        let (repo, _) = origin.into_repo().expect("repohost");
        let url = repo.canonical_http_url("").expect("repohost");
        return Ok(HttpResponse::TemporaryRedirect().header("Location", url.into_owned()).finish());
    }

    Ok(serve_cached(with_file_cache(&state, cache_file, 86400, {
        render_crate_page(state.clone(), origin)
    }).await?))
}

fn get_origin_from_subpath(q: &actix_web::dev::Path<Url>) -> Option<Origin> {
    let parts = q.query("crate");
    let mut parts = parts.splitn(4, '/');
    let first = parts.next()?;
    match parts.next() {
        None => Origin::try_from_crates_io_name(&first),
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

async fn handle_debug(req: HttpRequest) -> Result<HttpResponse, ServerError> {
    if !cfg!(debug_assertions) {
        Err(failure::err_msg("off"))?
    }
    let origin = get_origin_from_subpath(req.match_info()).ok_or(failure::format_err!("boo"))?;
    let state: &AServerState = req.app_data().expect("appdata");
    let crates = state.crates.load();
    let crates2 = Arc::clone(&crates);
    let ver = rt_run_timeout(&state.rt, 10, async move {
        crates2.rich_crate_version_async(&origin).await
    }).await?;
    let mut page: Vec<u8> = Vec::with_capacity(32000);
    front_end::render_debug_page(&mut page, &ver, &crates)?;
    Ok(HttpResponse::Ok()
        .content_type("text/html;charset=UTF-8")
        .header("Cache-Control", "no-cache")
        .content_length(page.len() as u64)
        .body(page))
}

async fn handle_install(req: HttpRequest) -> Result<HttpResponse, ServerError> {
    let state2: &AServerState = req.app_data().expect("appdata");
    let origin = if let Some(o) = get_origin_from_subpath(req.match_info()) {o}
    else {
        return render_404_page(&state2, req.path().trim_start_matches("/install"), "crate");
    };

    let state = state2.clone();
    let (page, last_mod) = rt_run_timeout(&state2.rt, 30, async move {
        let crates = state.crates.load();
        let ver = crates.rich_crate_version_async(&origin).await?;
        let mut page: Vec<u8> = Vec::with_capacity(32000);
        front_end::render_install_page(&mut page, &ver, &crates, &state.markup).await?;
        minify_html(&mut page);
        mark_server_still_alive(&state);
        Ok::<_, failure::Error>((page, None))
    }).await?;
    Ok(serve_cached((page, 7200, false, last_mod)))
}

async fn handle_author(req: HttpRequest) -> Result<HttpResponse, ServerError> {
    let login = req.match_info().query("author");
    println!("author page for {:?}", login);
    let state: &AServerState = req.app_data().expect("appdata");
    let crates = state.crates.load();
    let aut = match crates.author_by_login(&login).await {
        Ok(aut) => aut,
        Err(_) => {
            return render_404_page(state, login, "user");
        }
    };
    if aut.github.login != login {
        return Ok(HttpResponse::PermanentRedirect().header("Location", format!("/~{}", encode(&aut.github.login))).body(""));
    }
    let cache_file = state.page_cache_dir.join(format!("@{}.html", login));
    Ok(serve_cached(
        with_file_cache(state, cache_file, 3600, {
            let state = state.clone();
            run_timeout(60, async move {
                let crates = state.crates.load();
                let mut page: Vec<u8> = Vec::with_capacity(32000);
                front_end::render_author_page(&mut page, &aut, &crates, &state.markup).await?;
                minify_html(&mut page);
                mark_server_still_alive(&state);
                Ok::<_, failure::Error>((page, None))
            })
        })
        .await?,
    ))
}

async fn handle_crate(req: HttpRequest) -> Result<HttpResponse, ServerError> {
    let crate_name = req.match_info().query("crate");
    println!("crate page for {:?}", crate_name);
    let state: &AServerState = req.app_data().expect("appdata");
    let crates = state.crates.load();
    let origin = match Origin::try_from_crates_io_name(&crate_name).filter(|o| crates.crate_exists(o)) {
        Some(o) => o,
        None => return render_404_page(state, &crate_name, "crate"),
    };
    let cache_file = state.page_cache_dir.join(format!("{}.html", crate_name));
    Ok(serve_cached(with_file_cache(state, cache_file, 600, {
        render_crate_page(state.clone(), origin)
    }).await?))
}

async fn handle_crate_reverse_dependencies(req: HttpRequest) -> Result<HttpResponse, ServerError> {
    let crate_name = req.match_info().query("crate");
    println!("rev deps for {:?}", crate_name);
    let state: &AServerState = req.app_data().expect("appdata");
    let crates = state.crates.load();
    let origin = match Origin::try_from_crates_io_name(&crate_name).filter(|o| crates.crate_exists(o)) {
        Some(o) => o,
        None => return render_404_page(state, &crate_name, "crate"),
    };
    Ok(serve_cached(render_crate_reverse_dependencies(state.clone(), origin).await?))
}

async fn handle_crate_reviews(req: HttpRequest) -> Result<HttpResponse, ServerError> {
    let crate_name = req.match_info().query("crate");
    println!("crev for {:?}", crate_name);
    let state: &AServerState = req.app_data().expect("appdata");
    let crates = state.crates.load();
    let origin = match Origin::try_from_crates_io_name(&crate_name).filter(|o| crates.crate_exists(o)) {
        Some(o) => o,
        None => return render_404_page(state, &crate_name, "crate"),
    };
    let state = state.clone();
    Ok(serve_cached(rt_run_timeout(&state.clone().rt, 30, async move {
        let crates = state.crates.load();
        let ver = crates.rich_crate_version_async(&origin).await?;
        let mut page: Vec<u8> = Vec::with_capacity(32000);
        let reviews = crates.reviews_for_crate(ver.origin());
        front_end::render_crate_reviews(&mut page, &reviews, &ver, &crates, &state.markup).await?;
        minify_html(&mut page);
        mark_server_still_alive(&state);
        Ok::<_, failure::Error>((page, 24*3600, false, None))
    }).await?))
}

async fn handle_new_trending(req: HttpRequest) -> Result<HttpResponse, ServerError> {
    let state: &AServerState = req.app_data().expect("appdata");
    Ok(serve_cached(with_file_cache(state, state.page_cache_dir.join("_new_.html"), 600, {
        let state = state.clone();
        run_timeout(60, async move {
            let crates = state.crates.load();
            let mut page: Vec<u8> = Vec::with_capacity(32000);
            front_end::render_trending_crates(&mut page, &crates, &state.markup).await?;
            minify_html(&mut page);
            Ok::<_, failure::Error>((page, None))
    })}).await?))
}

/// takes path to storage, freshness in seconds, and a function to call on cache miss
/// returns (page, fresh in seconds)
async fn with_file_cache<F: Send>(state: &AServerState, cache_file: PathBuf, cache_time: u32, generate: F) -> Result<(Vec<u8>, u32, bool, Option<DateTime<FixedOffset>>), failure::Error>
    where F: Future<Output=Result<(Vec<u8>, Option<DateTime<FixedOffset>>), failure::Error>> + 'static {
    if let Ok(modified) = std::fs::metadata(&cache_file).and_then(|m| m.modified()) {
        let now = SystemTime::now();
        // rebuild in debug always
        let is_fresh = !cfg!(debug_assertions) && modified > (now - Duration::from_secs((cache_time / 20 + 5).into()));
        let is_acceptable = modified > (now - Duration::from_secs((3600 * 24 * 7 + cache_time * 5).into()));

        let age_secs = now.duration_since(modified).ok().map(|age| age.as_secs() as u32).unwrap_or(0);

        if let Ok(mut page_cached) = std::fs::read(&cache_file) {
            if !is_acceptable {
                let _ = std::fs::remove_file(&cache_file); // next req will block instead of an endless refresh loop
            }

            assert!(page_cached.len() > 4);
            let trailer_pos = page_cached.len() - 4; // The worst data format :)
            let timestamp = u32::from_le_bytes(page_cached.get(trailer_pos..).unwrap().try_into().unwrap());
            page_cached.truncate(trailer_pos);

            let last_mod = if timestamp > 0 {Some(DateTime::from_utc(NaiveDateTime::from_timestamp(timestamp as _, 0), FixedOffset::east(0)))} else {None};
            let cache_time_remaining = cache_time.saturating_sub(age_secs);

            println!("Using cached page {} {}s fresh={:?} acc={:?}", cache_file.display(), cache_time_remaining, is_fresh, is_acceptable);

            if !is_fresh {
                let _ = state.rt.spawn({
                    let state = state.clone();
                    async move {
                    if let Ok(_s) = state.background_job.try_acquire() {
                        match generate.await {
                            Ok((mut page, last_mod)) => {
                                eprintln!("Done refresh of {}", cache_file.display());
                                let timestamp = last_mod.map(|a| a.timestamp() as u32).unwrap_or(0);
                                page.extend_from_slice(&timestamp.to_le_bytes()); // The worst data format :)

                                if let Err(e) = std::fs::write(&cache_file, &page) {
                                    eprintln!("warning: Failed writing to {}: {}", cache_file.display(), e);
                                }
                            },
                            Err(e) => {
                                eprintln!("Refresh err: {} {}", e.iter_chain().map(|e| e.to_string()).collect::<Vec<_>>().join("; "), cache_file.display());
                            },
                        }
                    } else {
                        eprintln!("Skipped refresh of {}", cache_file.display());
                    }
                }});
            }
            return Ok((page_cached, if !is_fresh { cache_time_remaining / 4 } else { cache_time_remaining }.max(2), !is_acceptable, last_mod));
        }

        println!("Cache miss {} {}", cache_file.display(), age_secs);
    } else {
        println!("Cache miss {} no file", cache_file.display());
    }

    let (page, last_mod) = state.rt.spawn({
        let state = state.clone();
        async move {
            let _s = tokio::time::timeout(Duration::from_secs(10), state.foreground_job.acquire()).await?;
            Ok::<_, failure::Error>(generate.await?)
        }}).await??;
    if let Err(e) = std::fs::write(&cache_file, &page) {
        eprintln!("warning: Failed writing to {}: {}", cache_file.display(), e);
    }
    Ok((page, cache_time, false, last_mod))
}

fn render_crate_page(state: AServerState, origin: Origin) -> impl Future<Output = Result<(Vec<u8>, Option<DateTime<FixedOffset>>), failure::Error>> + 'static {
    run_timeout(30, async move {
        let crates = state.crates.load();
        let (all, ver) = futures::try_join!(crates.rich_crate_async(&origin), crates.rich_crate_version_async(&origin))?;
        let mut page: Vec<u8> = Vec::with_capacity(32000);
        let last_mod = front_end::render_crate_page(&mut page, &all, &ver, &crates, &state.markup).await?;
        minify_html(&mut page);
        mark_server_still_alive(&state);
        Ok::<_, failure::Error>((page, last_mod))
    })
}

async fn render_crate_reverse_dependencies(state: AServerState, origin: Origin) -> Result<(Vec<u8>, u32, bool, Option<DateTime<FixedOffset>>), failure::Error> {
    let s = state.clone();
    rt_run_timeout(&s.rt, 30, async move {
        let crates = state.crates.load();
        let ver = crates.rich_crate_version_async(&origin).await?;
        let mut page: Vec<u8> = Vec::with_capacity(32000);
        front_end::render_crate_reverse_dependencies(&mut page, &ver, &crates, &state.markup).await?;
        minify_html(&mut page);
        mark_server_still_alive(&state);
        Ok::<_, failure::Error>((page, 24*3600, false, None))
    }).await
}

async fn handle_keyword(req: HttpRequest) -> Result<HttpResponse, ServerError> {
    let q = req.match_info().query("keyword");
    if q.is_empty() {
        return Ok(HttpResponse::TemporaryRedirect().header("Location", "/").finish());
    }

    let query = q.to_owned();
    let state: &AServerState = req.app_data().expect("appdata");
    let state2 = state.clone();
    let (query, page) = rt_run_timeout(&state.rt, 15, async move {
        if !is_alnum(&query) {
            return Ok::<_, failure::Error>((query, None));
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
    }).await?;

    Ok(if let Some(page) = page {
        HttpResponse::Ok()
            .content_type("text/html;charset=UTF-8")
            .header("Cache-Control", "public, max-age=172800, stale-while-revalidate=604800, stale-if-error=86400")
            .content_length(page.len() as u64)
            .body(page)
    } else {
        HttpResponse::TemporaryRedirect().header("Location", format!("/search?q={}", urlencoding::encode(&query))).finish()
    })
}

fn serve_cached((page, cache_time, refresh, last_modified): (Vec<u8>, u32, bool, Option<DateTime<FixedOffset>>)) -> HttpResponse {
    let err_max = (cache_time * 10).max(3600 * 24 * 2);

    // last-modified is ambiguous, because it's modification of the content, not the whole state
    let mut hasher = blake3::Hasher::new();
    hasher.update(if refresh {b"1"} else {b"0"});
    hasher.update(&page);
    let etag = format!("\"{:.16}\"", base64::encode(hasher.finalize().as_bytes()));

    HttpResponse::Ok()
        .content_type("text/html;charset=UTF-8")
        .header("etag", etag)
        .if_true(!refresh, |h| {
            h.header("Cache-Control", format!("public, max-age={}, stale-while-revalidate={}, stale-if-error={}", cache_time, cache_time*3, err_max));
        })
        .if_true(refresh, |h| {
            h.header("Refresh", "5");
            h.header("Cache-Control", "no-cache, s-maxage=4, must-revalidate");
        })
        .if_some(last_modified, |l, h| {
            // can't give validator, because then 304 leaves refresh
            if !refresh {
                h.header("Last-Modified", l.to_rfc2822());
            }
        })
        .content_length(page.len() as u64)
        .body(page)
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
    let qs = req.query_string().replace('+',"%20");
    let qs = qstring::QString::from(qs.as_str());
    match qs.get("q").unwrap_or("") {
        q if !q.trim_start().is_empty() => {
            let state: &AServerState = req.app_data().expect("appdata");
            let query = q.to_owned();
            let page = state.rt.spawn({
                let state = state.clone();
                async move {
                    tokio::task::spawn_blocking(move || {
                        let results = state.index.search(&query, 50, true)?;
                        let mut page = Vec::with_capacity(32000);
                        front_end::render_serp_page(&mut page, &query, &results, &state.markup)?;
                        minify_html(&mut page);
                        Ok::<_, failure::Error>((page, 600u32, false, None))
                    }).await
                }
            }).await???;
            Ok(serve_cached(page))
        },
        _ => Ok(HttpResponse::PermanentRedirect().header("Location", "/").finish()),
    }
}

async fn handle_sitemap(req: HttpRequest) -> Result<HttpResponse, ServerError> {
    let (w, page) = writer::<_, failure::Error>().await;
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
        .header("Cache-Control", "public, max-age=259200, stale-while-revalidate=72000, stale-if-error=72000")
        .streaming(page))
}

async fn handle_feed(req: HttpRequest) -> Result<HttpResponse, ServerError> {
    let state: &AServerState = req.app_data().expect("appdata");
    let state2 = state.clone();
    let page = rt_run_timeout(&state.rt, 60, async move {
        let crates = state2.crates.load();
        let mut page: Vec<u8> = Vec::with_capacity(32000);
        front_end::render_feed(&mut page, &crates).await?;
        Ok::<_, failure::Error>(page)
    }).await?;
    Ok(HttpResponse::Ok()
    .content_type("application/atom+xml;charset=UTF-8")
    .header("Cache-Control", "public, max-age=10800, stale-while-revalidate=259200, stale-if-error=72000")
    .content_length(page.len() as u64)
    .body(page))
}

fn run_timeout<R, T: 'static + Send>(secs: u64, fut: R) -> impl Future<Output=Result<T, failure::Error>> where R: Future<Output=Result<T, failure::Error>> {
    Box::pin(tokio::time::timeout(Duration::from_secs(secs), fut).map(|res| res?))
}

async fn rt_run_timeout<R, T: 'static + Send>(rt: &Handle, secs: u64, fut: R) -> Result<T, failure::Error> where R: 'static + Send + Future<Output=Result<T, failure::Error>> {
    rt.spawn(tokio::time::timeout(Duration::from_secs(secs), fut)).await??
}

struct ServerError {
    err: failure::Error,
}

impl ServerError {
    pub fn new(err: failure::Error) -> Self {
        for cause in err.iter_chain() {
            eprintln!("• {}", cause);
            // The server is stuck and useless
            let s = cause.to_string();
            if s.contains("Too many open files") || s.contains("instance has previously been poisoned") ||
               s.contains("inconsistent park state") || s.contains("failed to allocate an alternative stack") {
                eprintln!("Fatal error: {}", s);
                std::process::exit(2);
            }
        }
        Self { err }
        }
    }

impl From<failure::Error> for ServerError {
    fn from(err: failure::Error) -> Self {
        Self::new(err)
    }
}

impl<T: Send + Sync + fmt::Display> From<failure::Context<T>> for ServerError {
    fn from(err: failure::Context<T>) -> Self {
        Self::new(err.into())
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

impl actix_web::ResponseError for ServerError {
    fn status_code(&self) -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }

    fn error_response(&self) -> HttpResponse<Body> {
        let mut page = Vec::with_capacity(20000);
        front_end::render_error(&mut page, &self.err);
        HttpResponse::InternalServerError()
            .content_type("text/html;charset=UTF-8")
            .content_length(page.len() as u64)
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
