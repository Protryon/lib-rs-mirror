use std::path::PathBuf;
use std::borrow::Cow;
use actix_web::{http::*, *};
use env_logger;
use futures::future::{self, Future};
use futures_cpupool::CpuPool;
use std::sync::Arc;
use std::env;
use front_end;
use search_index::CrateSearchIndex;
use render_readme::{Highlighter, ImageFilter, Renderer};

struct NopFilter {}
impl ImageFilter for NopFilter {
    fn filter_url<'a>(&self, _: &'a str) -> Cow<'a, str> {
        "#nope".into()
    }

    fn image_size(&self, _: &str) -> Option<(u32, u32)> {
        None
    }
}

struct ServerState {
    pool: CpuPool,
    markup: Renderer,
    index: CrateSearchIndex,
}

type AServerState = Arc<ServerState>;

fn main() {
    env_logger::init();
    let sys = actix::System::new("crates-server");

    let markup = Renderer::new_filter(Highlighter::new(), Arc::new(NopFilter {}));

    let public_dir: PathBuf = env::var_os("DOCUMENT_ROOT").map(From::from).unwrap_or_else(|| "../style/public".into());
    let data_dir: PathBuf = env::var_os("CRATE_DATA_DIR").map(From::from).unwrap_or_else(|| "../data".into());

    assert!(public_dir.exists(), "DOCUMENT_ROOT {} does not exist", public_dir.display());
    assert!(data_dir.exists(), "CRATE_DATA_DIR {} does not exist", data_dir.display());

    let index = CrateSearchIndex::new(data_dir).expect("data directory");

    let state = Arc::new(ServerState {
        pool: CpuPool::new_num_cpus(),
        markup,
        index,
    });

    server::new(move || {
        App::with_state(state.clone())
            .middleware(middleware::Logger::default())
            .resource("/search", |r| r.method(Method::GET).f(handle_search))
            .resource("/keywords/{keyword}", |r| r.method(Method::GET).f(handle_keyword))
            .handler("/", fs::StaticFiles::new(&public_dir).expect("public directory"))
            .default_resource(|r| r.f(handle_404))
    })
    .bind("127.0.0.1:32531")
    .expect("Can not bind to 127.0.0.1:32531")
    .shutdown_timeout(0) // <- Set shutdown timeout to 0 seconds (default 60s)
    .start();

    println!("Starting HTTP server on 127.0.0.1:32531");
    let _ = sys.run();
}

fn handle_404(_req: &HttpRequest<AServerState>) -> Result<HttpResponse> {
    Ok(HttpResponse::NotFound().content_type("text/plain;charset=UTF-8").body("404\n"))
}

fn handle_keyword(req: &HttpRequest<AServerState>) -> FutureResponse<HttpResponse> {
    let kw: Result<String, _> = req.match_info().query("keyword");
    match kw {
        Ok(ref q) if !q.is_empty() && is_alnum(&q) => {
            let query = q.to_owned();
            let state = req.state();
            let state2 = Arc::clone(state);
            state.pool.spawn_fn(move || {
                let mut page: Vec<u8> = Vec::with_capacity(50000);
                let keyword_query = format!("keywords:{}", query);
                let results = state2.index.search(&keyword_query, 100).unwrap();
                front_end::render_keyword_page(&mut page, &query, &results, &state2.markup).unwrap();
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
