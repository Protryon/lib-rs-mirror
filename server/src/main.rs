use actix_web::http::*;
use actix_web::*;
use categories::Category;
use categories::CATEGORIES;
use env_logger;
use front_end;
use futures::future::{self, Future};
use futures_cpupool::CpuPool;
use kitchen_sink;
use kitchen_sink::CrateData;
use kitchen_sink::KitchenSink;
use kitchen_sink::Origin;
use render_readme::{Highlighter, ImageOptimAPIFilter, Renderer};
use search_index::CrateSearchIndex;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::prelude::FutureExt;

use std::alloc::System;
#[global_allocator]
static A: System = System;

struct ServerState {
    render_pool: CpuPool,
    search_pool: CpuPool,
    markup: Renderer,
    index: CrateSearchIndex,
    crates: KitchenSink,
    public_crates_dir: PathBuf,
}

type AServerState = Arc<ServerState>;

fn main() {
    env_logger::init();
    kitchen_sink::dont_hijack_ctrlc();
    let sys = actix::System::new("crates-server");

    let public_styles_dir: PathBuf = env::var_os("DOCUMENT_ROOT").map(From::from).unwrap_or_else(|| "../style/public".into());
    let public_crates_dir: PathBuf = env::var_os("CRATE_HTML_ROOT").map(From::from).unwrap_or_else(|| "/www/crates.rs/public/crates".into());
    let data_dir: PathBuf = env::var_os("CRATE_DATA_DIR").map(From::from).unwrap_or_else(|| "../data".into());
    let github_token = env::var("GITHUB_TOKEN").expect("GITHUB_TOKEN missing");

    assert!(public_crates_dir.exists(), "CRATE_HTML_ROOT {} does not exist", public_crates_dir.display());
    assert!(public_styles_dir.exists(), "DOCUMENT_ROOT {} does not exist", public_styles_dir.display());
    assert!(data_dir.exists(), "CRATE_DATA_DIR {} does not exist", data_dir.display());

    let crates = KitchenSink::new(&data_dir, &github_token).expect("init");
    let image_filter = Arc::new(ImageOptimAPIFilter::new("czjpqfbdkz", crates.main_cache_dir().join("images.db")).expect("images.db"));
    let markup = Renderer::new_filter(Some(Highlighter::new()), image_filter);

    let index = CrateSearchIndex::new(data_dir).expect("data directory");

    let state = Arc::new(ServerState {
        render_pool: CpuPool::new_num_cpus(),
        search_pool: CpuPool::new_num_cpus(),
        markup,
        index,
        crates,
        public_crates_dir,
    });

    std::thread::spawn({
        let state = state.clone();
        move || {
            state.crates.prewarm();
        }
    });

    server::new(move || {
        App::with_state(state.clone())
            .middleware(middleware::Logger::default())
            .resource("/", |r| r.method(Method::GET).f(handle_home))
            .resource("/search", |r| r.method(Method::GET).f(handle_search))
            .resource("/keywords/{keyword}", |r| r.method(Method::GET).f(handle_keyword))
            .resource("/crates/{crate}", |r| r.method(Method::GET).f(handle_crate))
            .handler("/", fs::StaticFiles::new(&public_styles_dir).expect("public directory")
                .default_handler(default_handler))
    })
    .bind("127.0.0.1:32531")
    .expect("Can not bind to 127.0.0.1:32531")
    .shutdown_timeout(1)
    .start();

    println!("Started HTTP server on http://127.0.0.1:32531");
    let _ = sys.run();
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

fn default_handler(req: &HttpRequest<AServerState>) -> Result<HttpResponse> {
    let path = req.uri().path();
    assert!(path.starts_with('/'));
    if path.ends_with('/') {
        return Ok(HttpResponse::PermanentRedirect().header("Location", path.trim_end_matches('/')).body(""));
    }
    if let Some(cat) = find_category(path.split('/').skip(1)) {
        return handle_category(req, cat);
    }
    if let Some(name) = path.split('/').skip(1).next() {
        if let Ok(_) = req.state().crates.rich_crate(&Origin::from_crates_io_name(name)) {
            return Ok(HttpResponse::PermanentRedirect().header("Location", format!("/crates/{}", name)).body(""));
        }
    }

    Ok(HttpResponse::NotFound().content_type("text/plain;charset=UTF-8").body("404\n"))
}

fn handle_category(req: &HttpRequest<AServerState>, cat: &Category) -> Result<HttpResponse> {
    let mut page: Vec<u8> = Vec::with_capacity(150000);
    let state = req.state();
    state.crates.prewarm();
    front_end::render_category(&mut page, cat, &state.crates, &state.markup).expect("render");
    Ok(HttpResponse::Ok()
        .content_type("text/html;charset=UTF-8")
        .header("Cache-Control", "public, s-maxage=600, max-age=43200, stale-while-revalidate=259200, stale-if-error=72000")
        .content_length(page.len() as u64)
        .body(page))
}

fn handle_home(req: &HttpRequest<AServerState>) -> FutureResponse<HttpResponse> {
    let state = req.state();
    let state2 = Arc::clone(state);
    state
        .render_pool
        .spawn_fn(move || {
            state2.crates.prewarm();
            let mut page: Vec<u8> = Vec::with_capacity(50000);
            front_end::render_homepage(&mut page, &state2.crates)?;
            Ok(page)
        })
        .timeout(Duration::from_secs(300))
        .map_err(map_err)
        .from_err()
        .and_then(|page| {
            future::ok(
                HttpResponse::Ok()
                    .content_type("text/html;charset=UTF-8")
                    .header("Cache-Control", "public, s-maxage=600, max-age=43200, stale-while-revalidate=259200, stale-if-error=72000")
                    .content_length(page.len() as u64)
                    .body(page),
            )
        })
        .responder()
}

fn handle_crate(req: &HttpRequest<AServerState>) -> FutureResponse<HttpResponse> {
    let kw: String = req.match_info().query("crate").expect("arg");
    println!("rendering {:?}", kw);
    let state = req.state();
    let state2 = Arc::clone(state);
    state
        .render_pool
        .spawn_fn(move || {
            assert!(is_alnum(&kw));
            state2.crates.prewarm();
            let origin = Origin::from_crates_io_name(&kw);
            let all = state2.crates.rich_crate(&origin)?;
            let ver = state2.crates.rich_crate_version(&origin, CrateData::Full)?;
            let mut page: Vec<u8> = Vec::with_capacity(50000);
            front_end::render_crate_page(&mut page, &all, &ver, &state2.crates, &state2.markup)?;
            std::fs::write(state2.public_crates_dir.join(format!("{}.html", kw)), &page)?;
            Ok(page)
        })
        .timeout(Duration::from_secs(12))
        .map_err(map_err)
        .from_err()
        .and_then(|page| {
            future::ok(
                HttpResponse::Ok()
                    .content_type("text/html;charset=UTF-8")
                    .header("Cache-Control", "public, s-maxage=3600, max-age=172800, stale-while-revalidate=604800, stale-if-error=72000")
                    .content_length(page.len() as u64)
                    .body(page),
            )
        })
        .responder()
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
                    let mut page: Vec<u8> = Vec::with_capacity(50000);
                    let keyword_query = format!("keywords:\"{}\"", query);
                    let results = state2.index.search(&keyword_query, 100)?;
                    if !results.is_empty() {
                        front_end::render_keyword_page(&mut page, &query, &results, &state2.markup)?;
                        Ok((query, Some(page)))
                    } else {
                        Ok((query, None))
                    }
                })
                .timeout(Duration::from_secs(2))
                .map_err(map_err)
                .from_err()
                .and_then(|(query, page)| {
                    future::ok(if let Some(page) = page {
                        HttpResponse::Ok()
                            .content_type("text/html;charset=UTF-8")
                            .header("Cache-Control", "public, s-maxage=3600, max-age=604800, stale-while-revalidate=604800, stale-if-error=86400")
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

fn handle_search(req: &HttpRequest<AServerState>) -> FutureResponse<HttpResponse> {
    match req.query().get("q") {
        Some(q) if !q.is_empty() => {
            let query = q.to_owned();
            let state = req.state();
            let state2 = Arc::clone(state);
            state
                .search_pool
                .spawn_fn(move || {
                    let mut page: Vec<u8> = Vec::with_capacity(50000);
                    let results = state2.index.search(&query, 50)?;
                    front_end::render_serp_page(&mut page, &query, &results, &state2.markup)?;
                    Ok(page)
                })
                .timeout(Duration::from_secs(2))
                .map_err(map_err)
                .from_err()
                .and_then(|page| {
                    future::ok(
                        HttpResponse::Ok()
                            .content_type("text/html;charset=UTF-8")
                            .header("Cache-Control", "public, s-maxage=60, max-age=43200, stale-while-revalidate=259200, stale-if-error=72000")
                            .content_length(page.len() as u64)
                            .body(page),
                    )
                })
                .responder()
        },
        _ => future::ok(HttpResponse::TemporaryRedirect().header("Location", "/").finish()).responder(),
    }
}
