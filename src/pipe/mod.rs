use std::sync::atomic::{AtomicU64, Ordering};

use bytes::{Buf, Bytes};
use http_body_util::{BodyExt, Full};
use hyper::{body::Incoming, Request, Response, StatusCode};
use tokio::sync::mpsc::{channel, Sender};

use crate::storage::{feeds, items, transaction};

mod bark;
mod proxy;

struct ParseRequest {
    url: String,
    body: Bytes,
    status_code: StatusCode,
}

#[derive(Clone)]
pub struct FeedConsumer {
    db: String,
    bark: String,
    proxy: String,
    sender: Sender<ParseRequest>,
}

static GLOBAL_HTTP_200: AtomicU64 = AtomicU64::new(0);
static GLOBAL_HTTP_304: AtomicU64 = AtomicU64::new(0);
static GLOBAL_HTTP_502: AtomicU64 = AtomicU64::new(0);
static GLOBAL_HTTP_503: AtomicU64 = AtomicU64::new(0);

pub async fn metrics() -> Result<Response<Full<Bytes>>, hyper::Error> {
    Ok(Response::new(Full::new(Bytes::from(format!(
        "\
        rss_pipe_status_code_count{{status_code=\"200\"}} {}\n\
        rss_pipe_status_code_count{{status_code=\"304\"}} {}\n\
        rss_pipe_status_code_count{{status_code=\"502\"}} {}\n\
        rss_pipe_status_code_count{{status_code=\"503\"}} {}\n",
        GLOBAL_HTTP_200.load(Ordering::Relaxed),
        GLOBAL_HTTP_304.load(Ordering::Relaxed),
        GLOBAL_HTTP_502.load(Ordering::Relaxed),
        GLOBAL_HTTP_503.load(Ordering::Relaxed)
    )))))
}

fn handle_error(uri: &str, message: String) -> String {
    GLOBAL_HTTP_502.fetch_add(1, Ordering::Relaxed);
    println!("returned 502 handling feed {}: {}", uri, message);
    message
}

impl FeedConsumer {
    pub fn new(db: String, bark: String, proxy: String) -> Self {
        let (sender, mut receiver) = channel(1024);
        let parser = Self {
            db,
            bark,
            proxy,
            sender,
        };

        let consumer = parser.clone();
        tokio::spawn(async move {
            loop {
                if let Some(p) = &receiver.recv().await {
                    match feed_rs::parser::parse(p.body.clone().reader()) {
                        Ok(feed) => consumer.handle_feed(p.url.as_str(), feed).await,
                        Err(v) => consumer.handle_feed_error(p, v).await,
                    }
                }
            }
        });
        parser
    }

    async fn handle_feed(&self, url: &str, feed: feed_rs::model::Feed) {
        let feed_title = match feed.title {
            Some(title) => title.content.clone(),
            None => "".into(),
        };
        let bark_requests = transaction(self.db.as_str(), |tx| {
            let mut bark_requests: Vec<(&str, &str, &str, &str)> = Vec::new();
            let (feed_id, url_id, feed_created) = feeds::upsert_feed(tx, url, feed_title.as_str());
            if feed_created {
                bark_requests.push(("New Feed Subscription", "", feed_title.as_str(), ""));
                println!(
                    "creating new feed {} [{}] {} [{}]",
                    feed_title, feed_id, url, url_id
                );
            }
            for item in feed.entries.iter().rev() {
                let item_title = match &item.title {
                    Some(title) => title.content.as_str(),
                    None => "",
                };
                let content = match &item.content {
                    Some(content) => match &content.body {
                        Some(body) => body.as_str(),
                        None => "",
                    },
                    None => match &item.summary {
                        Some(summary) => summary.content.as_str(),
                        None => "",
                    },
                };
                let link = match item.links.first() {
                    Some(link) => link.href.as_str(),
                    None => "",
                };
                let (item_id, item_created) = items::create_item(
                    tx,
                    feed_id,
                    item.id.as_str(),
                    item_title,
                    content,
                    link,
                    match item.authors.first() {
                        Some(person) => match &person.email {
                            Some(email) => email.as_str(),
                            None => person.name.as_str(),
                        },
                        None => "",
                    },
                    match item.published {
                        Some(published) => published.timestamp() as u64,
                        None => match item.updated {
                            Some(updated) => updated.timestamp() as u64,
                            None => 0,
                        },
                    },
                );
                if item_created {
                    println!("creating new item {} [{}]", item.id, item_id);
                    if !feed_created {
                        bark_requests.push((feed_title.as_str(), item_title, content, link));
                    }
                }
            }
            bark_requests
        });
        for request in bark_requests {
            bark::send_notification(
                request.0,
                request.1,
                request.2,
                request.3,
                self.bark.as_str(),
            )
            .await;
        }
    }

    async fn handle_feed_error(&self, p: &ParseRequest, v: feed_rs::parser::ParseFeedError) {
        if p.status_code == StatusCode::NOT_MODIFIED {
            let url = p.url.as_str();
            if let None = transaction(self.db.as_str(), |tx| feeds::get_feed_id_by_url(tx, url)) {
                println!("received status code 304 without existing feed, fetching again without cache: {}", url);
                if let Ok(response) = proxy::http_https_get(url, self.proxy.as_str()).await {
                    if let Err(e) = self.enqueue_response_body(url.to_owned(), response).await {
                        println!("received error {} enqueuing response body", e);
                    }
                }
            }
        } else {
            println!(
                "received status code {} handling feed {}: {}",
                p.status_code, p.url, v
            )
        }
    }

    async fn enqueue_response_body(
        &self,
        url: String,
        response_in: Response<Incoming>,
    ) -> Result<Response<Full<Bytes>>, hyper::Error> {
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
            println!("{}", e);
        };
        Ok(Response::from_parts(parts, Full::new(content)))
    }

    pub async fn enqueue_http(
        &self,
        uri: String,
        req: Request<Incoming>,
    ) -> Result<Response<Full<Bytes>>, hyper::Error> {
        match proxy::http_call(uri.as_str(), req).await {
            Ok(response) => self.enqueue_response_body(uri, response).await,
            Err(error) => proxy::handle_error(handle_error(uri.as_str(), format!("{:?}", error))),
        }
    }

    pub async fn enqueue_https(
        &self,
        uri: String,
        req: Request<Incoming>,
    ) -> Result<Response<Full<Bytes>>, hyper::Error> {
        match proxy::https_call(uri.as_str(), req, self.proxy.as_str()).await {
            Ok(response) => self.enqueue_response_body(uri, response).await,
            Err(error) => proxy::handle_error(handle_error(uri.as_str(), format!("{:?}", error))),
        }
    }
}
