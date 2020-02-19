use actix_web::dev::Url;
use actix_web::http::header::HeaderValue;
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
use kitchen_sink::KitchenSink;
use kitchen_sink::Origin;
use kitchen_sink;
use locale::Numeric;
use render_readme::{Highlighter, ImageOptimAPIFilter, Renderer, Markup};
use repo_url::SimpleRepo;
use search_index::CrateSearchIndex;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime};
use urlencoding::decode;
use urlencoding::encode;
use tokio::runtime::Handle;

mod writer;

#[global_allocator]
static ALLOCATOR: Cap<std::alloc::System> = Cap::new(std::alloc::System, 1*1024*1024*1024);

static HUP_SIGNAL: AtomicU32 = AtomicU32::new(0);

struct ServerState {
    markup: Renderer,
    index: CrateSearchIndex,
    crates: ArcSwap<KitchenSink>,
    page_cache_dir: PathBuf,
    data_dir: PathBuf,
}

type AServerState = web::Data<ServerState>;

fn main() {
    let mut sys = actix_rt::System::new("actix-server");
    let res = sys.block_on(run_server());

    if let Err(e) = res {
        for c in e.iter_chain() {
            eprintln!("Error: {}", c);
        }
        std::process::exit(1);
    }
}

async fn run_server() -> Result<(), failure::Error> {
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

    let crates = KitchenSink::new(&data_dir, &github_token).await?;
    let image_filter = Arc::new(ImageOptimAPIFilter::new("czjpqfbdkz", crates.main_cache_dir().join("images.db"))?);
    let markup = Renderer::new_filter(Some(Highlighter::new()), image_filter);

    let index = CrateSearchIndex::new(&data_dir)?;

    let state = web::Data::new(ServerState {
        markup,
        index,
        crates: ArcSwap::from_pointee(crates),
        page_cache_dir,
        data_dir: data_dir.clone(),
    });

    // refresher thread
    let handle = Handle::current();
    handle.spawn({
        let state = state.clone();
        async move {
            state.crates.load().prewarm();
            loop {
                tokio::time::delay_for(std::time::Duration::from_secs(1)).await;
                if 1 == HUP_SIGNAL.swap(0, Ordering::SeqCst) {
                    println!("HUP!");
                    match KitchenSink::new(&data_dir, &github_token).await {
                        Ok(k) => {
                            let k = Arc::new(k);
                            k.update();
                            state.crates.store(k);
                            state.crates.load().prewarm();
                        },
                        Err(e) => {
                            eprintln!("Refresh failed: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
            }
        }});

    let server = HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .wrap(middleware::DefaultHeaders::new().header("Server", HeaderValue::from_static(concat!("actix-web/2.0 lib.rs/", env!("CARGO_PKG_VERSION")))))
            .wrap(middleware::Logger::default())
            .route("/", web::get().to(handle_home))
            .route("/search", web::get().to(handle_search))
            .route("/index", web::get().to(handle_search)) // old crates.rs/index url
            .route("/keywords/{keyword}", web::get().to(handle_keyword))
            .route("/crates/{crate}", web::get().to(handle_crate))
            .route("/crates/{crate}/rev", web::get().to(handle_crate_reverse_dependencies))
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

fn find_category<'a>(slugs: impl Iterator<Item=&'a str>) -> Option<&'static Category> {
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

fn handle_static_page(state: &ServerState, path: &str) -> Result<Option<HttpResponse>, failure::Error> {
    let path = &path[1..]; // remove leading /
    if !is_alnum(path) {
        return Ok(None);
    }

    let md_path = state.data_dir.as_path().join(format!("page/{}.md", path));
    if !md_path.exists() {
        return Ok(None)
    }

    let mut chars = path.chars();
    let path_capitalized = chars.next().into_iter().flat_map(|c| c.to_uppercase()).chain(chars).collect();
    let crates = state.crates.load();
    let crate_num = crates.all_crates_io_crates().len();
    let total_crate_num = crates.all_crates().count();

    let md = std::fs::read_to_string(md_path)?
        .replace("$CRATE_NUM", &Numeric::english().format_int(crate_num))
        .replace("$TOTAL_CRATE_NUM", &Numeric::english().format_int(total_crate_num));
    let mut page = Vec::with_capacity(md.len()*2);
    front_end::render_static_page(&mut page, path_capitalized, &Markup::Markdown(md), &state.markup)?;
    Ok(Some(HttpResponse::Ok()
        .content_type("text/html;charset=UTF-8")
        .header("Cache-Control", "public, max-age=7200, stale-while-revalidate=604800, stale-if-error=86400")
        .content_length(page.len() as u64)
        .body(page)))
}

async fn default_handler(req: HttpRequest) -> Result<HttpResponse, failure::Error> {
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

    let crates = state.crates.load();
    let name = path.trim_matches('/');
    if let Ok(k) = crates.rich_crate(&Origin::from_crates_io_name(name)) {
        return Ok(HttpResponse::PermanentRedirect().header("Location", format!("/crates/{}", encode(k.name()))).body(""));
    }
    let inverted_hyphens: String = name.chars().map(|c| if c == '-' {'_'} else if c == '_' {'-'} else {c.to_ascii_lowercase()}).collect();
    if let Ok(k) = crates.rich_crate(&Origin::from_crates_io_name(&inverted_hyphens)) {
        return Ok(HttpResponse::TemporaryRedirect().header("Location", format!("/crates/{}", encode(k.name()))).body(""));
    }
    if crates.is_it_a_keyword(&inverted_hyphens) {
        return Ok(HttpResponse::TemporaryRedirect().header("Location", format!("/keywords/{}", encode(&inverted_hyphens))).body(""));
    }

    render_404_page(state, path)
}

fn render_404_page(state: &AServerState, path: &str) -> Result<HttpResponse, failure::Error> {
    let decoded = decode(path).ok();
    let rawtext = decoded.as_ref().map(|d| d.as_str()).unwrap_or(path);

    let query = rawtext.chars().map(|c| if c.is_alphanumeric() {c} else {' '}).take(100).collect::<String>();
    let query = query.trim();
    let results = state.index.search(query, 5, false).unwrap_or_default();
    let mut page: Vec<u8> = Vec::with_capacity(50000);
    front_end::render_404_page(&mut page, query, &results, &state.markup)?;

    Ok(HttpResponse::NotFound()
        .content_type("text/html;charset=UTF-8")
        .content_length(page.len() as u64)
        .header("Cache-Control", "public, max-age=60, stale-while-revalidate=3600, stale-if-error=3600")
        .body(page))
}

async fn handle_category(req: HttpRequest, cat: &'static Category) -> Result<HttpResponse, failure::Error> {
    let state: &AServerState = req.app_data().expect("appdata");
    let state = state.clone();
    let crates = state.crates.load();
    crates.prewarm();
    let cache_file = state.page_cache_dir.join(format!("_{}.html", cat.slug));
    Ok(serve_cached(with_file_cache(cache_file, 1800, move || {
        run_timeout_async(30, async move {
            let mut page: Vec<u8> = Vec::with_capacity(150000);
            front_end::render_category(&mut page, cat, &crates, &state.markup).await?;
            Ok::<_, failure::Error>((page, None))
        })
    }).await?))
}

async fn handle_home(req: HttpRequest) -> Result<HttpResponse, failure::Error> {
    println!("home route");
    let query = req.query_string().trim_start_matches('?');
    if !query.is_empty() && query.find('=').is_none() {
        return Ok(HttpResponse::TemporaryRedirect().header("Location", format!("/search?q={}", query)).finish());
    }

    let state: &AServerState = req.app_data().expect("appdata");
    let state = state.clone();
    let cache_file = state.page_cache_dir.join("_.html");
    Ok(serve_cached(with_file_cache(cache_file, 3600, move || {
        run_timeout_async(300, async move {
            let crates = state.crates.load();
            crates.prewarm();
            let mut page: Vec<u8> = Vec::with_capacity(50000);
            front_end::render_homepage(&mut page, &crates)?;
            Ok::<_, failure::Error>((page, None))
        })
    }).await?))
}

async fn handle_github_crate(req: HttpRequest) -> Result<HttpResponse, failure::Error> {
    handle_git_crate(req, "gh").await
}
async fn handle_gitlab_crate(req: HttpRequest) -> Result<HttpResponse, failure::Error> {
    handle_git_crate(req, "lab").await
}

async fn handle_git_crate(req: HttpRequest, slug: &'static str) -> Result<HttpResponse, failure::Error> {
    let inf = req.match_info();
    let state: &AServerState = req.app_data().expect("appdata");
    let state = state.clone();
    let owner = inf.query("owner");
    let repo = inf.query("repo");
    let crate_name = inf.query("crate");
    println!("{} crate {}/{}/{}", slug, owner, repo, crate_name);
    if !is_alnum(&owner) || !is_alnum_dot(&repo) || !is_alnum(&crate_name) {
        return render_404_page(&state, &crate_name);
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

    Ok(serve_cached(with_file_cache(cache_file, 86400, move || {
        render_crate_page(state, origin)
    }).await?))
}

fn get_origin_from_subpath(q: &actix_web::dev::Path<Url>) -> Option<Origin> {
    let parts = q.query("crate");
    let mut parts = parts.splitn(4, '/');
    let first = parts.next()?;
    match parts.next() {
        None => Some(Origin::from_crates_io_name(&first)),
        Some(owner) => {
            let repo = parts.next()?;
            let package = parts.next()?;
            match first {
                "github" | "gh" => Some(Origin::from_github(SimpleRepo::new(owner, repo), package)),
                "gitlab" | "lab" => Some(Origin::from_gitlab(SimpleRepo::new(owner, repo), package)),
                _ => None,
            }
        }
    }
}

async fn handle_debug(req: HttpRequest) -> Result<HttpResponse, failure::Error> {
    if !cfg!(debug_assertions) {
        Err(failure::err_msg("off"))?
    }

    let state: &AServerState = req.app_data().expect("appdata");
    let origin = get_origin_from_subpath(req.match_info()).ok_or(failure::format_err!("boo"))?;
    let mut page: Vec<u8> = Vec::with_capacity(50000);
    let crates = state.crates.load();
    let ver = crates.rich_crate_version(&origin)?;
    front_end::render_debug_page(&mut page, &ver, &crates)?;
    Ok(HttpResponse::Ok()
        .content_type("text/html;charset=UTF-8")
        .header("Cache-Control", "no-cache")
        .content_length(page.len() as u64)
        .body(page))
}

async fn handle_install(req: HttpRequest) -> Result<HttpResponse, failure::Error> {
    let state: &AServerState = req.app_data().expect("appdata");
    let origin = if let Some(o) = get_origin_from_subpath(req.match_info()) {o}
    else {
        return render_404_page(&state, req.path().trim_start_matches("/install"));
    };

    let state = state.clone();
    let (page, last_mod) = run_timeout(30, move || {
        let crates = state.crates.load();
        let ver = crates.rich_crate_version(&origin)?;
        let mut page: Vec<u8> = Vec::with_capacity(50000);
        front_end::render_install_page(&mut page, &ver, &crates, &state.markup)?;
        Ok::<_, failure::Error>((page, None))
    }).await?;
    Ok(serve_cached((page, 3600, false, last_mod)))
}

async fn handle_crate(req: HttpRequest) -> Result<HttpResponse, failure::Error> {
    let crate_name = req.match_info().query("crate");
    println!("crate page for {:?}", crate_name);
    let state: &AServerState = req.app_data().expect("appdata");
    let state = state.clone();
    let crates = state.crates.load();
    let origin = Origin::from_crates_io_name(&crate_name);
    if !is_alnum(&crate_name) || !crates.crate_exists(&origin) {
        return render_404_page(&state, &crate_name);
    }
    let cache_file = state.page_cache_dir.join(format!("{}.html", crate_name));
    Ok(serve_cached(with_file_cache(cache_file, 900, move || {
        render_crate_page(state, origin)
    }).await?))
}

async fn handle_crate_reverse_dependencies(req: HttpRequest) -> Result<HttpResponse, failure::Error> {
    let crate_name = req.match_info().query("crate");
    println!("rev deps for {:?}", crate_name);
    let state: &AServerState = req.app_data().expect("appdata");
    let crates = state.crates.load();
    let origin = Origin::from_crates_io_name(&crate_name);
    if !is_alnum(&crate_name) || !crates.crate_exists(&origin) {
        return render_404_page(&state, &crate_name);
    }
    Ok(serve_cached(render_crate_reverse_dependencies(state.clone(), origin).await?))
}

/// takes path to storage, freshness in seconds, and a function to call on cache miss
/// returns (page, fresh in seconds)
async fn with_file_cache<F>(cache_file: PathBuf, cache_time: u32, generate: impl FnOnce() -> F + 'static) -> Result<(Vec<u8>, u32, bool, Option<DateTime<FixedOffset>>), failure::Error>
    where F: Future<Output=Result<(Vec<u8>, Option<DateTime<FixedOffset>>), failure::Error>> + 'static {
    if let Ok(modified) = std::fs::metadata(&cache_file).and_then(|m| m.modified()) {
        let now = SystemTime::now();
        // rebuild in debug always
        let is_fresh = !cfg!(debug_assertions) && modified > (now - Duration::from_secs((cache_time/20+5).into()));
        let is_acceptable = modified > (now - Duration::from_secs((3600*24*7 + cache_time*5).into()));

        let age_secs = now.duration_since(modified).ok().map(|age| age.as_secs() as u32).unwrap_or(0);

        if let Ok(page_cached) = std::fs::read(&cache_file) {
            let timestamp = modified.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
            let last_mod = DateTime::from_utc(NaiveDateTime::from_timestamp(timestamp as _, 0), FixedOffset::east(0));
            let cache_time_remaining = cache_time.saturating_sub(age_secs);

            println!("Using cached page {} {}s fresh={:?} acc={:?}", cache_file.display(), cache_time_remaining, is_fresh, is_acceptable);

            if !is_fresh {
                actix_rt::spawn(async move {
                    match generate().await {
                        Ok((page, _last_mod)) => { // FIXME: set cache file timestamp?
                            if let Err(e) = std::fs::write(&cache_file, &page) {
                                eprintln!("warning: Failed writing to {}: {}", cache_file.display(), e);
                            }
                        },
                        Err(e) => {
                            eprintln!("Cache pre-warm: {}", e);
                        }
                    }
                });
            }
            return Ok((page_cached, if !is_fresh {cache_time_remaining/4} else {cache_time_remaining}.max(2), !is_acceptable, Some(last_mod)));
        }

        println!("Cache miss {} {}", cache_file.display(), age_secs);
    } else {
        println!("Cache miss {} no file", cache_file.display());
    }

    let (page, last_mod) = generate().await?;
    if let Err(e) = std::fs::write(&cache_file, &page) {
        eprintln!("warning: Failed writing to {}: {}", cache_file.display(), e);
    }
    Ok((page, cache_time, false, last_mod))
}

fn render_crate_page(state: AServerState, origin: Origin) -> impl Future<Output=Result<(Vec<u8>, Option<DateTime<FixedOffset>>), failure::Error>> + 'static {
    run_timeout(30, move || {
        let crates = state.crates.load();
        crates.prewarm();
        let all = crates.rich_crate(&origin)?;
        let ver = crates.rich_crate_version(&origin)?;
        let mut page: Vec<u8> = Vec::with_capacity(50000);
        let last_mod = front_end::render_crate_page(&mut page, &all, &ver, &crates, &state.markup)?;
        Ok::<_, failure::Error>((page, last_mod))
    })
}

async fn render_crate_reverse_dependencies(state: AServerState, origin: Origin) -> Result<(Vec<u8>, u32, bool, Option<DateTime<FixedOffset>>), failure::Error> {
    run_timeout(30, move || {
        let crates = state.crates.load();
        crates.prewarm();
        let ver = crates.rich_crate_version(&origin)?;
        let mut page: Vec<u8> = Vec::with_capacity(50000);
        front_end::render_crate_reverse_dependencies(&mut page, &ver, &crates, &state.markup)?;
        Ok::<_, failure::Error>((page, 24*3600, false, None))
    }).await
}

async fn handle_keyword(req: HttpRequest) -> Result<HttpResponse, failure::Error> {
    let q = req.match_info().query("keyword");
    if q.is_empty() {
        return Ok(HttpResponse::TemporaryRedirect().header("Location", "/").finish())
    }

    let query = q.to_owned();
    let state: &AServerState = req.app_data().expect("appdata");
    let state2 = state.clone();
    let (query, page) = run_timeout(15, move || {
        if !is_alnum(&query) {
            return Ok::<_, failure::Error>((query, None));
        }
        let keyword_query = format!("keywords:\"{}\"", query);
        let results = state2.index.search(&keyword_query, 150, false)?;
        if !results.is_empty() {
            let mut page: Vec<u8> = Vec::with_capacity(50000);
            front_end::render_keyword_page(&mut page, &query, &results, &state2.markup)?;
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
    let err_max = (cache_time*10).max(3600*24*2);
    HttpResponse::Ok()
        .content_type("text/html;charset=UTF-8")
        .header("Cache-Control", format!("public, max-age={}, stale-while-revalidate={}, stale-if-error={}", cache_time, cache_time*3, err_max))
        .if_true(refresh, |h| {h.header("Refresh", "5");})
        .if_some(last_modified, |l, h| {h.header("Last-Modified", l.to_rfc2822());})
        .content_length(page.len() as u64)
        .body(page)
}

fn from_pool<E: std::fmt::Debug + Into<failure::Error>>(err: actix_threadpool::BlockingError<E>) -> failure::Error {
    use actix_threadpool::BlockingError::*;
    match err {
        Canceled => failure::format_err!("cancelled"),
        Error(e) => e.into()
    }
}

fn is_alnum(q: &str) -> bool {
    q.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn is_alnum_dot(q: &str) -> bool {
    let mut chars = q.chars();
    if !chars.next().map_or(false, |first| first.is_ascii_alphanumeric() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}

async fn handle_search(req: HttpRequest) -> Result<HttpResponse, failure::Error> {
    let qs = qstring::QString::from(req.query_string());
    match qs.get("q").unwrap_or("") {
        q if !q.is_empty() => {
            let query = q.to_owned();
            let state: &AServerState = req.app_data().expect("appdata");
            let state = state.clone();

            let results = state
                .index
                .search(&query, 50, true)?;
            let mut page = Vec::with_capacity(50000);
            front_end::render_serp_page(&mut page, &query, &results, &state.markup)?;

            Ok(serve_cached((page, 600, false, None)))
        }
        _ => Ok(HttpResponse::PermanentRedirect().header("Location", "/").finish()),
    }
}

async fn handle_sitemap(req: HttpRequest) -> Result<HttpResponse, failure::Error> {
    let (w, page) = writer::<_, failure::Error>();
    let state: &AServerState = req.app_data().expect("appdata");
    let state = state.clone();

    rayon::spawn(move || {
        let mut w = std::io::BufWriter::with_capacity(16000, w);
        let crates = state.crates.load();
        if let Err(e) = front_end::render_sitemap(&mut w, &crates) {
            if let Ok(mut w) = w.into_inner() {
                w.fail(e.into());
            }
        }
    });

    Ok::<_, failure::Error>(HttpResponse::Ok()
        .content_type("application/xml;charset=UTF-8")
        .header("Cache-Control", "public, max-age=259200, stale-while-revalidate=72000, stale-if-error=72000")
        .streaming(page))
}

async fn handle_feed(req: HttpRequest) -> Result<HttpResponse, failure::Error> {
    let state: &AServerState = req.app_data().expect("appdata");
    let state2 = state.clone();
    let page = run_timeout(60, move || {
        let crates = state2.crates.load();
        crates.prewarm();
        let mut page: Vec<u8> = Vec::with_capacity(50000);
        front_end::render_feed(&mut page, &crates)?;
        Ok::<_, failure::Error>(page)
    }).await?;
    Ok(HttpResponse::Ok()
    .content_type("application/atom+xml;charset=UTF-8")
    .header("Cache-Control", "public, max-age=10800, stale-while-revalidate=259200, stale-if-error=72000")
    .content_length(page.len() as u64)
    .body(page))
}

async fn run_timeout<T: 'static + Send>(secs: u64, cb: impl FnOnce() -> Result<T, failure::Error> + Send + 'static) -> Result<T, failure::Error> {
    tokio::time::timeout(Duration::from_secs(secs), async move { tokio::task::block_in_place(cb) }).await?
}

async fn run_timeout_async<R, T: 'static + Send>(secs: u64, fut: R) -> Result<T, failure::Error> where R: Future<Output=Result<T, failure::Error>> + Send + 'static {
    tokio::time::timeout(Duration::from_secs(secs), fut).await?
}


