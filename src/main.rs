#![deny(unused_extern_crates)]
use std::{collections::HashMap, error::Error, net::SocketAddr, sync::OnceLock, time::Instant};

use bytes::Bytes;
use http_body_util::Full;
use hyper::{Request, Response, body::Incoming, server::conn::http1::Builder, service::service_fn};
use tokio::net::TcpListener;

mod common;
mod fever;
mod pipe;
mod push;
mod storage;

static ARGS: OnceLock<HashMap<String, String>> = OnceLock::new();
static PIPE: OnceLock<pipe::Pipe> = OnceLock::new();

async fn handle(
    db: &str,
    prefix: &str,
    fever_auth: &str,
    pipe: &pipe::Pipe,
    req: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, common::PipeError> {
    let req_path = req.uri().path();
    if req_path == "/metrics" {
        pipe::metrics(db).await
    } else if req_path.starts_with(format!("/{prefix}/fever").as_str()) {
        fever::fever(db, fever_auth, req).await
    } else if let Some(path) = req_path.strip_prefix("/http/") {
        pipe.enqueue_http(format!("http://{path}"), req).await
    } else if let Some(path) = req_path.strip_prefix("/https/") {
        pipe.enqueue_https(format!("https://{path}"), req).await
    } else if let Some(path) = req_path.strip_prefix("/invoke/") {
        pipe.enqueue_invoke(path.to_owned(), req).await
    } else {
        common::not_found()
    }
}

async fn handle_wrapper(
    db: &str,
    prefix: &str,
    fever_auth: &str,
    pipe: &pipe::Pipe,
    req: Request<Incoming>,
    remote_addr: SocketAddr,
) -> Result<Response<Full<Bytes>>, String> {
    let start_time = Instant::now();
    let req_info = format!("accepted {} {} {}", remote_addr, req.method(), req.uri());
    let response = handle(db, prefix, fever_auth, pipe, req).await;
    match response {
        Ok(r) => {
            println!(
                "{} {} {}ms",
                req_info,
                r.status().as_u16(),
                start_time.elapsed().as_millis()
            );
            Ok(r)
        }
        Err(e) => Err(format!("{e:?}")),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let m = ARGS.get_or_init(|| {
        std::env::args()
            .map(|x| {
                x.split_once("=")
                    .map(|(x, y)| (x.to_owned(), y.to_owned()))
                    .unwrap_or(("".into(), "".into()))
            })
            .filter(|(x, y)| !x.is_empty() && !y.is_empty())
            .collect()
    });

    let args_db = match m.get("--db") {
        Some(v) => v,
        None => "db.sqlite3",
    };
    let args_auth = match m.get("--auth") {
        Some(v) => v,
        None => "not set (please set --auth in order to use fever api)",
    };
    let args_bark = match m.get("--bark") {
        Some(v) => v,
        None => "",
    };
    let args_bind = match m.get("--bind") {
        Some(v) => v,
        None => "172.17.0.1:5080",
    };
    let args_path = match m.get("--path") {
        Some(v) => v,
        None => "rss_pipe",
    };
    let args_proxy = match m.get("--proxy") {
        Some(v) => v,
        None => "",
    };
    let args_prefix = match m.get("--prefix") {
        Some(v) => v,
        None => "https://example.com/",
    };
    let args_pipe = match m.get("--pipe") {
        Some(v) => v,
        None => "rss_pipe.py",
    };

    let addr: SocketAddr = args_bind.parse()?;

    storage::migrations(args_db);

    common::script::Script::initialize();

    let pipe_instance = PIPE.get_or_init(|| {
        pipe::Pipe::new(
            args_db.to_owned(),
            args_bark.to_owned(),
            args_proxy.to_owned(),
            common::script::Script::new(args_pipe),
        )
    });

    println!(
        "Running with args (set with --key=value):\n \
        --db: {args_db}\n \
        --auth: {args_auth}\n \
        --bark: {args_bark}\n \
        --bind: {args_bind}\n \
        --path: {args_path}\n \
        --pipe: {args_pipe}\n \
        --proxy: {args_proxy}\n \
        --prefix: {args_prefix}"
    );

    let listener = TcpListener::bind(addr).await?;
    loop {
        let (stream, remote_addr) = listener.accept().await?;
        let service =
            service_fn(move |req| handle_wrapper(args_db, args_path, args_auth, pipe_instance, req, remote_addr));
        let tokio_io = hyper_util::rt::tokio::TokioIo::new(stream);
        tokio::task::spawn(async move {
            if let Err(err) = Builder::new().serve_connection(tokio_io, service).await {
                println!("!! error serving connection: {err:?}");
            }
        });
    }
}
