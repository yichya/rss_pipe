use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bytes::{Buf, Bytes};
use http::{Method, header};
use http_body_util::{BodyExt, Full};
use hyper::{Request, Response, StatusCode, body::Incoming};
use tokio::sync::mpsc::{Sender, channel};

use crate::{common, push, storage};

mod proxy;

struct ParseRequest {
    url: String,
    body: Bytes,
    status_code: StatusCode,
}

pub struct Pipe {
    db: String,
    bark: String,
    proxy: String,
    sender: Sender<ParseRequest>,
    methods: common::script::Script,
}

static GLOBAL_HTTP_200: AtomicU64 = AtomicU64::new(0);
static GLOBAL_HTTP_304: AtomicU64 = AtomicU64::new(0);
static GLOBAL_HTTP_502: AtomicU64 = AtomicU64::new(0);
static GLOBAL_HTTP_503: AtomicU64 = AtomicU64::new(0);
static GLOBAL_PIPE_ERR: AtomicU64 = AtomicU64::new(0);

pub async fn metrics(db: &str) -> Result<Response<Full<Bytes>>, common::PipeError> {
    Response::builder()
        .header(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8; escaping=values",
        )
        .body(Full::new(Bytes::from(format!(
            "# RSS Pipe Metrics\n\
            rss_pipe_status_code_count{{status_code=\"200\"}} {}\n\
            rss_pipe_status_code_count{{status_code=\"304\"}} {}\n\
            rss_pipe_status_code_count{{status_code=\"502\"}} {}\n\
            rss_pipe_status_code_count{{status_code=\"503\"}} {}\n\
            rss_pipe_error_count{{}} {}\n\
            rss_pipe_unread_count{{}} {}",
            GLOBAL_HTTP_200.load(Ordering::Relaxed),
            GLOBAL_HTTP_304.load(Ordering::Relaxed),
            GLOBAL_HTTP_502.load(Ordering::Relaxed),
            GLOBAL_HTTP_503.load(Ordering::Relaxed),
            GLOBAL_PIPE_ERR.load(Ordering::Relaxed),
            storage::transaction(db, |tx| storage::items::get_total_items(tx, "where is_read = 0")),
        ))))
        .map_err(|e| e.into())
}

fn handle_error(uri: &str, message: String) -> String {
    GLOBAL_HTTP_502.fetch_add(1, Ordering::Relaxed);
    println!("returned 502 handling feed {uri}: {message}");
    message
}

impl Pipe {
    pub fn new(db: String, bark: String, proxy: String, methods: common::script::Script) -> Self {
        let (sender, mut receiver) = channel(1024);

        let consumer = Self {
            db: db.clone(),
            bark: bark.clone(),
            proxy: proxy.clone(),
            sender: sender.clone(),
            methods: common::script::Script::empty(), // not used in consumer
        };

        tokio::spawn(async move {
            loop {
                if let Some(p) = &receiver.recv().await {
                    match feed_rs::parser::parse(p.body.clone().reader()) {
                        Ok(feed) => consumer.handle_feed(&p.url, feed).await,
                        Err(v) => consumer.handle_feed_error(p, v).await,
                    }
                }
            }
        });
        Self {
            db,
            bark,
            proxy,
            sender,
            methods,
        }
    }

    async fn handle_feed(&self, url: &str, feed: feed_rs::model::Feed) {
        let feed_title = match feed.title {
            Some(title) => title.content.clone(),
            None => "".into(),
        };
        let bark_requests = storage::transaction(&self.db, |tx| {
            let mut bark_requests: Vec<(&str, &str, &str, &str)> = Vec::new();
            let (feed_id, url_id, feed_created) = storage::feeds::upsert_feed(tx, url, &feed_title);
            if feed_created {
                bark_requests.push(("New Feed Subscription", "", &feed_title, ""));
                println!("creating new feed {feed_title} [{feed_id}] {url} [{url_id}]");
            }
            if feed_id > 0 && url_id > 0 {
                for item in feed.entries.iter().rev() {
                    let item_title = match &item.title {
                        Some(title) => &title.content,
                        None => "",
                    };
                    let content = match &item.content {
                        Some(content) => match &content.body {
                            Some(body) => body,
                            None => "",
                        },
                        None => match &item.summary {
                            Some(summary) => &summary.content,
                            None => "",
                        },
                    };
                    let link = match item.links.first() {
                        Some(link) => &link.href,
                        None => "",
                    };
                    let author = match item.authors.first() {
                        Some(person) => match &person.email {
                            Some(email) => email,
                            None => &person.name,
                        },
                        None => "",
                    };
                    let created_at = match item.published {
                        Some(published) => published.timestamp() as u64,
                        None => match item.updated {
                            Some(updated) => updated.timestamp() as u64,
                            None => 0,
                        },
                    };
                    let created_at_str = match created_at {
                        0 => SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or(Duration::from_secs(0))
                            .as_secs(),
                        _ => created_at,
                    }
                    .to_string();
                    let (item_id_update, item_updated) = storage::valine::refresh_existing_item(
                        tx,
                        feed_id,
                        &item.id,
                        item_title,
                        content,
                        link,
                        author,
                        &created_at_str,
                    );
                    if item_updated {
                        println!("updating existing item {} [{}]", item.id, item_id_update);
                        bark_requests.push((&feed_title, item_title, content, link));
                    } else {
                        let (item_id, item_created) = storage::items::create_item(
                            tx,
                            feed_id,
                            &item.id,
                            item_title,
                            content,
                            link,
                            author,
                            &created_at_str,
                        );
                        if item_created {
                            println!("creating new item {} [{}]", item.id, item_id);
                            if !feed_created {
                                bark_requests.push((&feed_title, item_title, content, link));
                            }
                        }
                    }
                }
            }
            bark_requests
        });
        for request in bark_requests {
            push::bark::send_notification(
                request.0,
                request.1,
                request.2,
                "rss_pipe_rust",
                Some(request.3.to_owned()),
                None,
                &self.bark,
            )
            .await;
        }
    }

    async fn handle_feed_error(&self, p: &ParseRequest, v: feed_rs::parser::ParseFeedError) {
        if p.status_code == StatusCode::NOT_MODIFIED {
            if storage::transaction(&self.db, |tx| storage::feeds::get_feed_id_by_url(tx, &p.url)).is_none() {
                println!(
                    "received status code 304 without existing feed, fetching again without cache: {}",
                    p.url
                );
                if let Ok(response) = proxy::http_https_get(&p.url, &self.proxy).await {
                    if let Err(e) = self.enqueue_response_body(p.url.to_owned(), response).await {
                        GLOBAL_PIPE_ERR.fetch_add(1, Ordering::Relaxed);
                        println!("!! error enqueuing response body: {e:?}");
                    }
                }
            }
        } else {
            GLOBAL_PIPE_ERR.fetch_add(1, Ordering::Relaxed);
            println!("received status code {} handling feed {}: {}", p.status_code, p.url, v)
        }
    }

    async fn enqueue_response_body(
        &self,
        url: String,
        response_in: Response<Incoming>,
    ) -> Result<Response<Full<Bytes>>, common::PipeError> {
        let status_code = response_in.status();
        match status_code {
            StatusCode::OK => {
                GLOBAL_HTTP_200.fetch_add(1, Ordering::Relaxed);
            }
            StatusCode::NOT_MODIFIED => {
                GLOBAL_HTTP_304.fetch_add(1, Ordering::Relaxed);
            }
            StatusCode::BAD_GATEWAY => {
                GLOBAL_HTTP_502.fetch_add(1, Ordering::Relaxed);
            }
            StatusCode::SERVICE_UNAVAILABLE => {
                GLOBAL_HTTP_503.fetch_add(1, Ordering::Relaxed);
            }
            _ => {
                println!("received status code {}", response_in.status());
            }
        }
        let (parts, incoming) = response_in.into_parts();
        let content = incoming.collect().await?.to_bytes();
        let parse_request = ParseRequest {
            url,
            status_code,
            body: content.clone(),
        };
        if let Err(e) = self.sender.send(parse_request).await {
            GLOBAL_PIPE_ERR.fetch_add(1, Ordering::Relaxed);
            println!("!! error sending data to pipe: {e}");
        };
        Ok(Response::from_parts(parts, Full::new(content)))
    }

    pub async fn enqueue_http(
        &self,
        uri: String,
        req: Request<Incoming>,
    ) -> Result<Response<Full<Bytes>>, common::PipeError> {
        match proxy::http_call(&uri, req).await {
            Ok(response) => self.enqueue_response_body(uri, response).await,
            Err(error) => proxy::handle_error(handle_error(&uri, format!("{error:?}"))),
        }
    }

    pub async fn enqueue_https(
        &self,
        uri: String,
        req: Request<Incoming>,
    ) -> Result<Response<Full<Bytes>>, common::PipeError> {
        match proxy::https_call(&uri, req, &self.proxy).await {
            Ok(response) => self.enqueue_response_body(uri, response).await,
            Err(error) => proxy::handle_error(handle_error(&uri, format!("{error:?}"))),
        }
    }

    pub async fn enqueue_invoke(
        &self,
        path: String,
        req: Request<Incoming>,
    ) -> Result<Response<Full<Bytes>>, common::PipeError> {
        let body = if req.method() == Method::POST {
            String::from_utf8(req.into_body().collect().await?.to_bytes().to_vec())?
        } else {
            "".to_owned()
        };
        let (method, params) = path.split_once('/').unwrap_or((&path, &body));
        let content = self.methods.evaluate(method, params, false).unwrap_or(body);
        let parse_request = ParseRequest {
            url: format!("rss_pipe://{}/{}", self.methods.get_name(), path),
            status_code: StatusCode::OK,
            body: content.clone().into(),
        };
        if let Err(e) = self.sender.send(parse_request).await {
            GLOBAL_PIPE_ERR.fetch_add(1, Ordering::Relaxed);
            println!("!! error sending data to pipe: {e}");
        };
        common::json_response(content.into())
    }
}
