use actix_web::http::*;
use actix_web::*;
use arc_swap::ArcSwap;
use categories::Category;
use categories::CATEGORIES;
use env_logger;
use failure::ResultExt;
use front_end;
use futures::future::FutureResult;
use futures::future::{self, Future};
use futures_cpupool::CpuPool;
use kitchen_sink;
use kitchen_sink::KitchenSink;
use kitchen_sink::Origin;
use render_readme::{Highlighter, ImageOptimAPIFilter, Renderer, Markup};
use search_index::CrateSearchIndex;
use std::env;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::prelude::FutureExt;
use repo_url::SimpleRepo;
use urlencoding::decode;
use urlencoding::encode;

mod writer;
use crate::writer::*;

use std::alloc::System;
#[global_allocator]
static A: System = System;

static HUP_SIGNAL: AtomicU32 = AtomicU32::new(0);

struct ServerState {
    render_pool: CpuPool,
    search_pool: CpuPool,
    markup: Renderer,
    index: CrateSearchIndex,
    crates: ArcSwap<KitchenSink>,
    page_cache_dir: PathBuf,
    data_dir: PathBuf,
}

type AServerState = Arc<ServerState>;

fn main() {
    if let Err(e) = run_server() {
        for c in e.iter_chain() {
            eprintln!("Error: {}", c);
        }
        std::process::exit(1);
    }
}

fn run_server() -> Result<(), failure::Error> {
    unsafe {
        signal_hook::register(signal_hook::SIGHUP, || HUP_SIGNAL.store(1, Ordering::SeqCst))
    }.expect("signal handler");
    unsafe {
        signal_hook::register(signal_hook::SIGUSR1, || HUP_SIGNAL.store(1, Ordering::SeqCst))
    }.expect("signal handler");

    env_logger::init();
    kitchen_sink::dont_hijack_ctrlc();
    let sys = actix::System::new("crates-server");

    let public_document_root: PathBuf = env::var_os("DOCUMENT_ROOT").map(From::from).unwrap_or_else(|| "../style/public".into());
    let page_cache_dir: PathBuf = "/var/tmp/crates-server".into();
    let data_dir: PathBuf = env::var_os("CRATE_DATA_DIR").map(From::from).unwrap_or_else(|| "../data".into());
    let github_token = env::var("GITHUB_TOKEN").context("GITHUB_TOKEN missing")?;

    let _ = std::fs::create_dir_all(&page_cache_dir);
    assert!(page_cache_dir.exists(), "{} does not exist", page_cache_dir.display());
    assert!(public_document_root.exists(), "DOCUMENT_ROOT {} does not exist", public_document_root.display());
    assert!(data_dir.exists(), "CRATE_DATA_DIR {} does not exist", data_dir.display());

    let crates = KitchenSink::new(&data_dir, &github_token, 20.)?;
    let image_filter = Arc::new(ImageOptimAPIFilter::new("czjpqfbdkz", crates.main_cache_dir().join("images.db"))?);
    let markup = Renderer::new_filter(Some(Highlighter::new()), image_filter);

    let index = CrateSearchIndex::new(&data_dir)?;

    let state = Arc::new(ServerState {
        render_pool: CpuPool::new(8), // may be network-blocked, so not num CPUs
        search_pool: CpuPool::new_num_cpus(),
        markup,
        index,
        crates: ArcSwap::from_pointee(crates),
        page_cache_dir,
        data_dir: data_dir.clone(),
    });

    // refresher thread
    std::thread::spawn({
        let state = state.clone();
        move || {
            state.crates.load().prewarm();
            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
                if 1 == HUP_SIGNAL.swap(0, Ordering::SeqCst) {
                    println!("HUP!");
                    match KitchenSink::new(&data_dir, &github_token, 20.) {
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
        }
    });

    server::new(move || {
        App::with_state(state.clone())
            .middleware(StandardHeaders)
            .middleware(middleware::Logger::default())
            .resource("/", |r| r.method(Method::GET).f(handle_home))
            .resource("/search", |r| r.method(Method::GET).f(handle_search))
            .resource("/index", |r| r.method(Method::GET).f(handle_search)) // old crates.rs/index url
            .resource("/keywords/{keyword}", |r| r.method(Method::GET).f(handle_keyword))
            .resource("/crates/{crate}", |r| r.method(Method::GET).f(handle_crate))
            .resource("/gh/{owner}/{repo}/{crate}", |r| r.method(Method::GET).f(handle_gh_crate))
            .resource("/atom.xml", |r| r.method(Method::GET).f(handle_feed))
            .resource("/sitemap.xml", |r| r.method(Method::GET).f(handle_sitemap))
            .handler("/", fs::StaticFiles::new(&public_document_root).expect("public directory")
                .default_handler(default_handler))
    })
    .bind("127.0.0.1:32531")
    .expect("Can not bind to 127.0.0.1:32531")
    .shutdown_timeout(1)
    .start();

    println!("Started HTTP server {} on http://127.0.0.1:32531", env!("CARGO_PKG_VERSION"));
    let _ = sys.run();
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

fn handle_static_page(state: &ServerState, path: &str) -> Result<Option<HttpResponse>> {
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

    let md = std::fs::read_to_string(md_path)?
        .replace("$CRATE_NUM", &format!("{}", state.crates.load().all_crates_io_crates().len()));
    let mut page = Vec::with_capacity(md.len()*2);
    front_end::render_static_page(&mut page, path_capitalized, &Markup::Markdown(md), &state.markup)?;
    Ok(Some(HttpResponse::Ok()
        .content_type("text/html;charset=UTF-8")
        .header("Cache-Control", "public, max-age=7200, stale-while-revalidate=604800, stale-if-error=86400")
        .content_length(page.len() as u64)
        .body(page)))
}

fn default_handler(req: &HttpRequest<AServerState>) -> FutureResponse<HttpResponse> {
    let state = req.state();
    let path = req.uri().path();
    assert!(path.starts_with('/'));
    if path.ends_with('/') {
        return Box::new(future::ok(HttpResponse::PermanentRedirect().header("Location", path.trim_end_matches('/')).body("")));
    }

    if let Some(cat) = find_category(path.split('/').skip(1)) {
        return handle_category(req, cat);
    }

    match handle_static_page(state, path) {
        Ok(None) => {},
        Ok(Some(page)) => return Box::new(future::ok(page)),
        Err(err) => return Box::new(future::err(err)),
    }

    let crates = state.crates.load();
    let name = path.trim_matches('/');
    if let Ok(k) = crates.rich_crate(&Origin::from_crates_io_name(name)) {
        return Box::new(future::ok(HttpResponse::PermanentRedirect().header("Location", format!("/crates/{}", encode(k.name()))).body("")));
    }
    let inverted_hyphens: String = name.chars().map(|c| if c == '-' {'_'} else if c == '_' {'-'} else {c.to_ascii_lowercase()}).collect();
    if let Ok(k) = crates.rich_crate(&Origin::from_crates_io_name(&inverted_hyphens)) {
        return Box::new(future::ok(HttpResponse::TemporaryRedirect().header("Location", format!("/crates/{}", encode(k.name()))).body("")));
    }
    if crates.is_it_a_keyword(&inverted_hyphens) {
        return Box::new(future::ok(HttpResponse::TemporaryRedirect().header("Location", format!("/keywords/{}", encode(&inverted_hyphens))).body("")));
    }

    Box::new(future::result(render_404_page(state, path)))
}

fn render_404_page(state: &AServerState, path: &str) -> Result<HttpResponse> {
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

fn handle_category(req: &HttpRequest<AServerState>, cat: &'static Category) -> FutureResponse<HttpResponse> {
    let state = Arc::clone(req.state());
    let crates = state.crates.load();
    crates.prewarm();
    let cache_file = state.page_cache_dir.join(format!("_{}.html", cat.slug));
    with_file_cache(cache_file, 7200, move || {
        Arc::clone(&state).render_pool.spawn_fn(move || {
            let mut page: Vec<u8> = Vec::with_capacity(150000);
            front_end::render_category(&mut page, cat, &crates, &state.markup).expect("render");
            Ok(page)
        })
    })
    .from_err()
    .and_then(serve_cached)
    .responder()
}

fn handle_home(req: &HttpRequest<AServerState>) -> FutureResponse<HttpResponse> {
    let query = req.query_string().trim_start_matches('?');
    if !query.is_empty() && query.find('=').is_none() {
        return future::ok(HttpResponse::TemporaryRedirect().header("Location", format!("/search?q={}", query)).finish()).responder();
    }

    let state = Arc::clone(req.state());
    let cache_file = state.page_cache_dir.join("_.html");
    with_file_cache(cache_file, 3600, move || {
        state.render_pool.spawn_fn({
            let state = state.clone();
            move || {
                let crates = state.crates.load();
                crates.prewarm();
                let mut page: Vec<u8> = Vec::with_capacity(50000);
                front_end::render_homepage(&mut page, &crates)?;
                Ok(page)
            }
        })
        .timeout(Duration::from_secs(300))
        .map_err(map_err)
    })
    .from_err()
    .and_then(serve_cached)
    .responder()
}

fn handle_gh_crate(req: &HttpRequest<AServerState>) -> FutureResponse<HttpResponse> {
    let inf = req.match_info();
    let state = Arc::clone(req.state());
    let owner: String = inf.query("owner").expect("arg1");
    let repo: String = inf.query("repo").expect("arg2");
    let crate_name: String = inf.query("crate").expect("arg3");
    println!("GH crate {}/{}/{}", owner, repo, crate_name);
    if !is_alnum(&owner) || !is_alnum_dot(&repo) || !is_alnum(&crate_name) {
        return Box::new(future::result(render_404_page(&state, &crate_name)));
    }

    let cache_file = state.page_cache_dir.join(format!("gh,{},{},{}.html", owner, repo, crate_name));
    let origin = Origin::from_github(SimpleRepo::new(owner.as_str(), repo.as_str()), crate_name);
    if !state.crates.load().crate_exists(&origin) {
        return Box::new(future::ok(HttpResponse::TemporaryRedirect().header("Location", format!("https://github.com/{}/{}", owner, repo)).finish()));
    }

    with_file_cache(cache_file, 9000, move || {
        render_crate_page(&state, origin)
            .timeout(Duration::from_secs(60))
            .map_err(map_err)})
    .from_err()
    .and_then(serve_cached)
    .responder()
}

fn handle_crate(req: &HttpRequest<AServerState>) -> FutureResponse<HttpResponse> {
    let crate_name: String = req.match_info().query("crate").expect("arg");
    println!("crate page for {:?}", crate_name);
    let state = Arc::clone(req.state());
    let crates = state.crates.load();
    let origin = Origin::from_crates_io_name(&crate_name);
    if !is_alnum(&crate_name) || !crates.crate_exists(&origin) {
        return Box::new(future::result(render_404_page(&state, &crate_name)));
    }
    let cache_file = state.page_cache_dir.join(format!("{}.html", crate_name));
    with_file_cache(cache_file, 1800, move || {
        render_crate_page(&state, origin)
            .timeout(Duration::from_secs(30))
            .map_err(map_err)})
    .from_err()
    .and_then(serve_cached)
    .responder()
}

/// takes path to storage, freshness in seconds, and a function to call on cache miss
/// returns (page, fresh in seconds)
fn with_file_cache<F>(cache_file: PathBuf, cache_time: u32, generate: impl FnOnce() -> F) -> impl Future<Item=(Vec<u8>, u32, bool), Error=failure::Error>
    where F: Future<Item=Vec<u8>, Error=failure::Error> + 'static {
    if let Ok(modified) = std::fs::metadata(&cache_file).and_then(|m| m.modified()) {
        let now = SystemTime::now();
        // rebuild in debug always
        let is_fresh = !cfg!(debug_assertions) && modified > (now - Duration::from_secs((cache_time/20+5).into()));
        let is_acceptable = modified > (now - Duration::from_secs((3600*24*7 + cache_time*5).into()));

        let age_secs = now.duration_since(modified).ok().map(|age| age.as_secs() as u32).unwrap_or(0);

        if let Ok(page_cached) = std::fs::read(&cache_file) {
            let cache_time_remaining = cache_time.saturating_sub(age_secs);

            println!("Using cached page {} {}s fresh={:?} acc={:?}", cache_file.display(), cache_time_remaining, is_fresh, is_acceptable);

            if !is_fresh {
                actix::spawn(generate()
                    .map(move |page| {
                        if let Err(e) = std::fs::write(&cache_file, &page) {
                            eprintln!("warning: Failed writing to {}: {}", cache_file.display(), e);
                        }
                    })
                    .map_err(move |e| {eprintln!("Cache pre-warm: {}", e);}))
            }
            return Either::A(future::ok(
                (page_cached, if !is_fresh {cache_time_remaining/4} else {cache_time_remaining}.max(2), !is_acceptable)
            ));
        }

        println!("Cache miss {} {}", cache_file.display(), age_secs);
    } else {
        println!("Cache miss {} no file", cache_file.display());
    }

    Either::B(generate().map(move |page| {
        if let Err(e) = std::fs::write(&cache_file, &page) {
            eprintln!("warning: Failed writing to {}: {}", cache_file.display(), e);
        }
        (page, cache_time, false)
    }))
}

fn render_crate_page(state: &AServerState, origin: Origin) -> impl Future<Item = Vec<u8>, Error = failure::Error> {
    let state2 = Arc::clone(state);
    state.render_pool.spawn_fn(move || {
        let crates = state2.crates.load();
        crates.prewarm();
        let all = crates.rich_crate(&origin)?;
        let ver = crates.rich_crate_version(&origin)?;
        let mut page: Vec<u8> = Vec::with_capacity(50000);
        front_end::render_crate_page(&mut page, &all, &ver, &crates, &state2.markup)?;
        Ok(page)
    })
}

fn handle_keyword(req: &HttpRequest<AServerState>) -> FutureResponse<HttpResponse> {
    let kw: Result<String, _> = req.match_info().query("keyword");
    match kw {
        Ok(ref q) if !q.is_empty() => {
            let query = q.to_owned();
            let state = req.state();
            let state2 = Arc::clone(state);
            state
                .search_pool
                .spawn_fn(move || {
                    if !is_alnum(&query) {
                        return Ok((query, None));
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
                })
                .timeout(Duration::from_secs(4))
                .map_err(map_err)
                .from_err()
                .and_then(|(query, page)| {
                    future::ok(if let Some(page) = page {
                        HttpResponse::Ok()
                            .content_type("text/html;charset=UTF-8")
                            .header("Cache-Control", "public, max-age=172800, stale-while-revalidate=604800, stale-if-error=86400")
                            .content_length(page.len() as u64)
                            .body(page)
                    } else {
                        HttpResponse::TemporaryRedirect().header("Location", format!("/search?q={}", urlencoding::encode(&query))).finish()
                    })
                    .responder()
                })
                .responder()
        },
        _ => future::ok(HttpResponse::PermanentRedirect().header("Location", "/").finish()).responder(),
    }
}

fn serve_cached<T>((page, cache_time, refresh): (Vec<u8>, u32, bool)) -> FutureResult<HttpResponse, T> {
    future::ok(HttpResponse::Ok()
        .content_type("text/html;charset=UTF-8")
        .header("Cache-Control", format!("public, max-age={}, stale-while-revalidate={}, stale-if-error={}", cache_time, cache_time*4, cache_time*10))
        .if_true(refresh, |h| {h.header("Refresh", "4");})
        .content_length(page.len() as u64)
        .body(page))
}

fn map_err(err: tokio_timer::timeout::Error<failure::Error>) -> failure::Error {
    match err.into_inner() {
        Some(e) => e,
        None => {
            eprintln!("Page render timed out");
            failure::err_msg("timed out")
        },
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

fn handle_search(req: &HttpRequest<AServerState>) -> Result<HttpResponse> {
    match req.query().get("q") {
        Some(q) if !q.is_empty() => {
            let query = q.to_owned();
            let state = Arc::clone(req.state());

            let (mut w, page) = writer();
            rayon::spawn(move || {
                let res = state
                    .index
                    .search(&query, 50, true)
                    .map_err(From::from)
                    .and_then(|results| front_end::render_serp_page(&mut w, &query, &results, &state.markup));
                if let Err(e) = res {
                    w.fail(e.into());
                }
            });

            Ok(HttpResponse::Ok()
                .content_type("text/html;charset=UTF-8")
                .header("Cache-Control", "public, max-age=600, stale-while-revalidate=259200, stale-if-error=72000")
                .body(Body::Streaming(Box::new(page))))
        }
        _ => Ok(HttpResponse::PermanentRedirect().header("Location", "/").finish()),
    }
}

fn handle_sitemap(req: &HttpRequest<AServerState>) -> Result<HttpResponse> {
    let (w, page) = writer();
    let state = Arc::clone(req.state());

    rayon::spawn(move || {
        let mut w = std::io::BufWriter::with_capacity(16000, w);
        let crates = state.crates.load();
        if let Err(e) = front_end::render_sitemap(&mut w, &crates) {
            if let Ok(mut w) = w.into_inner() {
                w.fail(e.into());
            }
        }
    });

    Ok(HttpResponse::Ok()
        .content_type("application/xml;charset=UTF-8")
        .header("Cache-Control", "public, max-age=259200, stale-while-revalidate=72000, stale-if-error=72000")
        .body(Body::Streaming(Box::new(page))))
}

fn handle_feed(req: &HttpRequest<AServerState>) -> FutureResponse<HttpResponse> {
    let state = req.state();
    let state2 = Arc::clone(state);
    state
        .render_pool
        .spawn_fn(move || {
            let crates = state2.crates.load();
            crates.prewarm();
            let mut page: Vec<u8> = Vec::with_capacity(50000);
            front_end::render_feed(&mut page, &crates)?;
            Ok(page)
        })
        .timeout(Duration::from_secs(60))
        .map_err(map_err)
        .from_err()
        .and_then(|page| {
            future::ok(
                HttpResponse::Ok()
                    .content_type("application/atom+xml;charset=UTF-8")
                    .header("Cache-Control", "public, max-age=10800, stale-while-revalidate=259200, stale-if-error=72000")
                    .content_length(page.len() as u64)
                    .body(page),
            )
        })
        .responder()
}

use actix_web::middleware::{Middleware, Response};
use header::HeaderValue;
struct StandardHeaders;

impl<S> Middleware<S> for StandardHeaders {
    fn response(&self, _req: &HttpRequest<S>, mut resp: HttpResponse) -> Result<Response> {
        resp.headers_mut().insert("Server", HeaderValue::from_static(concat!("actix-web/0.7 crates.rs/", env!("CARGO_PKG_VERSION"))));
        Ok(Response::Done(resp))
    }
}
