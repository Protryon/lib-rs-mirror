use std::path::PathBuf;
use actix_web::*;
use actix_web::http::*;
use env_logger;
use futures::future::{self, Future};
use futures_cpupool::CpuPool;
use std::sync::Arc;
use std::env;
use kitchen_sink;
use kitchen_sink::KitchenSink;
use kitchen_sink::Origin;
use kitchen_sink::CrateData;
use front_end;
use search_index::CrateSearchIndex;
use render_readme::{Highlighter, Renderer, ImageOptimAPIFilter};

use std::alloc::System;
#[global_allocator]
static A: System = System;

struct ServerState {
    pool: CpuPool,
    markup: Renderer,
    index: CrateSearchIndex,
    crates: KitchenSink,
}

type AServerState = Arc<ServerState>;

fn main() {
    env_logger::init();
    kitchen_sink::dont_hijack_ctrlc();
    let sys = actix::System::new("crates-server");


    let public_dir: PathBuf = env::var_os("DOCUMENT_ROOT").map(From::from).unwrap_or_else(|| "../style/public".into());
    let data_dir: PathBuf = env::var_os("CRATE_DATA_DIR").map(From::from).unwrap_or_else(|| "../data".into());
    let github_token = env::var("GITHUB_TOKEN").expect("GITHUB_TOKEN missing");

    assert!(public_dir.exists(), "DOCUMENT_ROOT {} does not exist", public_dir.display());
    assert!(data_dir.exists(), "CRATE_DATA_DIR {} does not exist", data_dir.display());

    let crates = KitchenSink::new(&data_dir, &github_token).unwrap();
    let image_filter = Arc::new(ImageOptimAPIFilter::new("czjpqfbdkz", crates.main_cache_dir().join("images.db")).unwrap());
    let markup = Renderer::new_filter(Highlighter::new(), image_filter);

    let index = CrateSearchIndex::new(data_dir).expect("data directory");

    let state = Arc::new(ServerState {
        pool: CpuPool::new_num_cpus(),
        markup,
        index,
        crates,
    });

    let state2 = Arc::clone(&state);
    let _ = std::thread::spawn(move || {
        state2.crates.prewarm();
    });

    server::new(move || {
        App::with_state(state.clone())
            .middleware(middleware::Logger::default())
            .resource("/", |r| r.method(Method::GET).f(handle_home))
            .resource("/search", |r| r.method(Method::GET).f(handle_search))
            .resource("/keywords/{keyword}", |r| r.method(Method::GET).f(handle_keyword))
            .resource("/crates/{crate}", |r| r.method(Method::GET).f(handle_crate))
            .handler("/", fs::StaticFiles::new(&public_dir).expect("public directory"))
            .default_resource(|r| r.f(handle_404))
    })
    .bind("127.0.0.1:32531")
    .expect("Can not bind to 127.0.0.1:32531")
    .shutdown_timeout(0) // <- Set shutdown timeout to 0 seconds (default 60s)
    .start();

    println!("Starting HTTP server on http://127.0.0.1:32531");
    let _ = sys.run();
}

fn handle_404(_req: &HttpRequest<AServerState>) -> Result<HttpResponse> {
    Ok(HttpResponse::NotFound().content_type("text/plain;charset=UTF-8").body("404\n"))
}

fn handle_home(req: &HttpRequest<AServerState>) -> FutureResponse<HttpResponse> {
    let state = req.state();
    let state2 = Arc::clone(state);
    state.pool.spawn_fn(move || {
        let mut page: Vec<u8> = Vec::with_capacity(50000);
        front_end::render_homepage(&mut page, &state2.crates).unwrap();
        Ok::<_,()>(page)
    })
    .map_err(|_| unreachable!())
    .and_then(|page| {
        future::ok(HttpResponse::Ok()
            .content_type("text/html;charset=UTF-8")
            .content_length(page.len() as u64)
            .body(page))
    })
    .responder()
}

fn handle_crate(req: &HttpRequest<AServerState>) -> FutureResponse<HttpResponse> {
    let kw: String = req.match_info().query("crate").unwrap();
    println!("rendering {:?}", kw);
    let origin = Origin::from_crates_io_name(&kw);
    let state = req.state();
    let state2 = Arc::clone(state);
    state.pool.spawn_fn(move || {
        let all = state2.crates.rich_crate(&origin).unwrap();
        let ver = state2.crates.rich_crate_version(&origin, CrateData::Full).unwrap();
        let mut page: Vec<u8> = Vec::with_capacity(50000);
        front_end::render_crate_page(&mut page, &all, &ver, &state2.crates, &state2.markup).unwrap();
        Ok::<_,()>(page)
    })
    .map_err(|_| unreachable!())
    .and_then(|page| {
        future::ok(HttpResponse::Ok()
            .content_type("text/html;charset=UTF-8")
            .header("Cache-Control", "max-age=172800, stale-while-revalidate=604800")
            .content_length(page.len() as u64)
            .body(page))
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
            state.pool.spawn_fn(move || {
                if !is_alnum(&query) {
                    return Ok((query, None));
                }
                let mut page: Vec<u8> = Vec::with_capacity(50000);
                let keyword_query = format!("keywords:\"{}\"", query);
                let results = state2.index.search(&keyword_query, 100).unwrap();
                if !results.is_empty() {
                    front_end::render_keyword_page(&mut page, &query, &results, &state2.markup).unwrap();
                    Ok::<_,()>((query, Some(page)))
                } else {
                    Ok((query, None))
                }
            })
            .map_err(|_| unreachable!())
            .and_then(|(query, page)| {
                future::ok(if let Some(page) = page {
                    HttpResponse::Ok()
                        .content_type("text/html;charset=UTF-8")
                        .header("Cache-Control", "max-age=604800, stale-while-revalidate=604800, stale-if-error=86400")
                        .content_length(page.len() as u64)
                        .body(page)
                } else {
                    HttpResponse::TemporaryRedirect()
                        .header("Location", format!("/search?q={}", urlencoding::encode(&query)))
                        .finish()

                }).responder()
            })
            .responder()
        },
        _ => {
            future::ok(HttpResponse::TemporaryRedirect()
                .header("Location", "/")
                .header("Cache-Control", "max-age=172800, stale-while-revalidate=604800, stale-if-error=86400")
                .finish())
                .responder()
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
            state.pool.spawn_fn(move || {
                let mut page: Vec<u8> = Vec::with_capacity(50000);
                let results = state2.index.search(&query, 50).unwrap();
                front_end::render_serp_page(&mut page, &query, &results, &state2.markup).unwrap();
                Ok::<_,()>(page)
            })
            .map_err(|_| unreachable!())
            .and_then(|page| {
                future::ok(HttpResponse::Ok()
                    .content_type("text/html;charset=UTF-8")
                    .content_length(page.len() as u64)
                    .body(page))
            })
            .responder()
        },
        _ => {
            future::ok(HttpResponse::TemporaryRedirect()
                .header("Location", "/")
                .finish())
                .responder()
        },
    }
}
