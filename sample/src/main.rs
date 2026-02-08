// gossamer-examples/src/main.rs
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};

use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto;
use tokio::net::TcpListener;

use gossamer::openapi::{OpenApiAcc, serde_json};
use gossamer::{Ctx, Error, GossamerResponse, Router, RouterBuilder};

async fn ping<S>(_ctx: Ctx<S>) -> Result<&'static str, Error> {
    Ok("pong")
}

#[derive(serde::Deserialize)]
struct EchoQuery {
    times: Option<usize>,
}

async fn echo<S>(ctx: Ctx<S>) -> Result<String, Error> {
    let q: EchoQuery = ctx.query()?;
    let phrase = ctx
        .params()
        .get("phrase")
        .map(|phr| phr.repeat(q.times.unwrap_or(1).min(50)))
        .unwrap_or(String::from(""));
    Ok(phrase.to_string())
}

async fn openapi(ctx: gossamer::Ctx<AppState>) -> Result<GossamerResponse, gossamer::Error> {
    let state = ctx.state();
    let bytes = state
        .openapi_json
        .get()
        .ok_or_else(|| gossamer::Error::bad_request("OpenAPI not initialized"))?;

    Ok(gossamer::JsonBytes(bytes.clone()).into())
}

struct AppState {
    openapi_json: OnceLock<Vec<u8>>,
}

fn build_router() -> Router<AppState> {
    let state = AppState {
        openapi_json: OnceLock::new(),
    };

    let (router, openapi) = RouterBuilder::new(state)
        .with_meta_builder(OpenApiAcc::new("Gossamer API", "0.1.0"))
        .get("/ping", ping)
        .get("/echo/{phrase}", echo)
        .get("/openapi.json", openapi)
        .finish();

    let openapi_json = serde_json::to_vec_pretty(&openapi).expect("Failed to serialize OpenAPI doc");

    router
        .state()
        .openapi_json
        .set(openapi_json)
        .expect("Failed to set OpenAPI JSON in state");

    router
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let listener = TcpListener::bind(addr).await?;

    let app = Arc::new(build_router());

    println!("listening on http://{addr}");

    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let app = app.clone();

        tokio::spawn(async move {
            let builder = auto::Builder::new(TokioExecutor::new());
            let svc = service_fn(move |req| {
                let app = app.clone();
                async move { Ok::<_, Infallible>(app.handle(req).await) }
            });

            if let Err(err) = builder.serve_connection(io, svc).await {
                eprintln!("connection error: {err:?}");
            }
        });
    }
}
