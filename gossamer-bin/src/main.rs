// gossamer-examples/src/main.rs
use std::{convert::Infallible, net::SocketAddr, sync::Arc};

use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto;
use tokio::net::TcpListener;

use gossamer::{Ctx, Error, RouterBuilder};

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let listener = TcpListener::bind(addr).await?;
    println!("listening on http://{addr}");

    let app = Arc::new(
        RouterBuilder::new(())
            .get("/ping", ping::<()>)
            .get("/echo/{phrase}", echo::<()>)
            .finish()
            .0,
    );

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
