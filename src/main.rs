#![deny(unused_extern_crates)]

use std::{collections::HashMap, env, net::SocketAddr, sync::OnceLock};

use bytes::Bytes;
use http::StatusCode;
use http_body_util::Full;
use hyper::{body::Incoming, server::conn::http1, service::service_fn, Request, Response};
use tokio::net::TcpListener;

mod fever;
mod pipe;
mod storage;

static ARGS: OnceLock<HashMap<String, String>> = OnceLock::new();
static PARSER: OnceLock<pipe::FeedConsumer> = OnceLock::new();

async fn handle(
    db: &str,
    fever_path: &str,
    fever_auth: &str,
    parser: &pipe::FeedConsumer,
    req: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    if req.uri().path().starts_with(fever_path) {
        let mut response = fever::entrance(db, fever_auth, req).await?;
        response
            .headers_mut()
            .insert("Content-Type", "application/json".parse().unwrap());
        Ok(response)
    } else if req.uri().path().starts_with("/metrics") {
        pipe::metrics().await
    } else if let Some(path) = req.uri().path().strip_prefix("/http/") {
        parser.enqueue_http(format!("http://{}", path), req).await
    } else if let Some(path) = req.uri().path().strip_prefix("/https/") {
        parser.enqueue_https(format!("https://{}", path), req).await
    } else {
        let mut not_found = Response::new(Full::new(Bytes::from("not found")));
        *not_found.status_mut() = StatusCode::NOT_FOUND;
        Ok(not_found)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let m = ARGS.get_or_init(|| {
        env::args()
            .map(|x| {
                x.split_once("=")
                    .map(|(x, y)| (x.to_owned(), y.to_owned()))
                    .unwrap_or(("".into(), "".into()))
            })
            .filter(|(x, y)| x != "" && y != "")
            .collect()
    });

    let db_file = match m.get("--db") {
        Some(v) => v,
        None => "db.sqlite3",
    };
    let bark_addr = match m.get("--bark") {
        Some(v) => v,
        None => "",
    };
    let bind_addr = match m.get("--bind") {
        Some(v) => v,
        None => "172.17.0.1:5080",
    };
    let fever_auth = match m.get("--auth") {
        Some(v) => v,
        None => "not set (please set --auth in order to use fever api)",
    };
    let fever_path = match m.get("--fever") {
        Some(v) => v,
        None => "/fever",
    };
    let proxy_addr = match m.get("--proxy") {
        Some(v) => v,
        None => "",
    };

    let addr: SocketAddr = bind_addr.parse()?;

    storage::migrations();

    let parser = PARSER.get_or_init(|| {
        pipe::FeedConsumer::new(
            db_file.to_owned(),
            bark_addr.to_owned(),
            proxy_addr.to_owned(),
        )
    });

    println!(
        "Running with args (set with --key=value):\n --db: {}\n --auth: {}\n --bark: {}\n --bind: {}\n --proxy: {}\n --fever: {}",
        db_file, fever_auth, bark_addr, bind_addr, proxy_addr, fever_path
    );

    let listener = TcpListener::bind(addr).await?;
    loop {
        let (stream, remote_addr) = listener.accept().await?;
        tokio::task::spawn(async move {
            if let Err(err) = http1::Builder::new()
                .serve_connection(
                    hyper_util::rt::tokio::TokioIo::new(stream),
                    service_fn(move |req| {
                        println!("accepted {} {} {}", remote_addr, req.method(), req.uri());
                        handle(db_file, fever_path, fever_auth, parser, req)
                    }),
                )
                .await
            {
                println!("Error serving connection: {:?}", err);
            }
        });
    }
}
