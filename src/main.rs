use std::borrow::Cow;
use actix_web::{http::*, *};
use env_logger;
use futures::future::{self, Future};
use futures_cpupool::CpuPool;
use log::*;
use std::sync::Arc;
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

    let index = CrateSearchIndex::new("../data").unwrap();

    let state = Arc::new(ServerState {
        pool: CpuPool::new_num_cpus(),
        markup,
        index,
    });

    server::new(move || {
        App::with_state(state.clone())
            .middleware(middleware::Logger::default())
            .resource("/search", |r| r.method(Method::GET).f(handle_search))
            .handler("/", fs::StaticFiles::new("../style/public").unwrap())
            .default_resource(|r| r.f(handle_404))
    })
    .bind("127.0.0.1:8080")
    .expect("Can not bind to 127.0.0.1:8080")
    .shutdown_timeout(0) // <- Set shutdown timeout to 0 seconds (default 60s)
    .start();

    info!("Starting http server");
    let _ = sys.run();
}

fn handle_404(_req: &HttpRequest<AServerState>) -> Result<HttpResponse> {
    Ok(HttpResponse::NotFound().content_type("text/plain;charset=UTF-8").body("404\n"))
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
