use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bytes::{Buf, Bytes};
use http::{Method, header};
use http_body_util::{BodyExt, Full};
use hyper::{Request, Response, StatusCode, body::Incoming};
use tokio::sync::mpsc::{Sender, channel};

use crate::{common, metrics, push, storage};

mod proxy;

struct ParseRequest {
    url: String,
    body: Bytes,
    query: Option<String>,
    status_code: StatusCode,
}

pub struct Pipe {
    db: String,
    bark: String,
    proxy: String,
    sender: Sender<ParseRequest>,
    methods: common::script::Script,
}

fn handle_error(uri: &str, message: String) -> String {
    metrics::status_code_502();
    println!("returned 502 handling feed {uri}: {message}");
    message
}

impl Pipe {
    pub fn new(db: &str, bark: &str, proxy: &str, methods: common::script::Script) -> Self {
        let (sender, mut receiver) = channel(1024);

        let consumer = Self {
            bark: bark.to_owned(),
            db: db.to_owned(),
            methods: common::script::Script::empty(),
            proxy: proxy.to_owned(),
            sender: sender.clone(),
        };

        tokio::spawn(async move {
            loop {
                if let Some(p) = &receiver.recv().await {
                    match feed_rs::parser::parse(p.body.clone().reader()) {
                        Ok(feed) => consumer.handle_feed(&p.url, &p.query, feed).await,
                        Err(v) => consumer.handle_feed_error(p, v).await,
                    }
                }
            }
        });
        Self {
            bark: bark.to_owned(),
            db: db.to_owned(),
            methods,
            proxy: proxy.to_owned(),
            sender,
        }
    }

    async fn handle_feed(&self, url: &str, query: &Option<String>, feed: feed_rs::model::Feed) {
        let full_url = match query {
            Some(v) => format!("{}?{}", url, v),
            None => url.to_owned(),
        };
        let feed_title = feed.title.map_or_else(String::new, |title| title.content.to_owned());
        let bark_requests = storage::transaction(&self.db, |tx| {
            let mut bark_requests: Vec<(&str, &str, &str, &str)> = Vec::new();
            let (feed_id, url_id, feed_created) = storage::feeds::upsert_feed(tx, &full_url, Some(&feed_title));
            if feed_created {
                bark_requests.push(("New Feed Subscription", "", &feed_title, ""));
                println!("creating new feed {feed_title} [{feed_id}] {full_url} [{url_id}]");
            }
            if feed_id > 0 && url_id > 0 {
                for item in feed.entries.iter().rev() {
                    let item_title = item.title.as_ref().map_or("", |title| &title.content);
                    let content = match &item.content {
                        Some(content) => content.body.as_deref().unwrap_or(""),
                        None => item.summary.as_ref().map_or("", |summary| &summary.content),
                    };
                    let link = item.links.first().map_or("", |a| &a.href);
                    let author = item.authors.first().map_or("", |a| a.email.as_ref().unwrap_or(&a.name));
                    let created_at = match item.published {
                        Some(published) => published.timestamp(),
                        None => item.updated.map_or(0, |updated| updated.timestamp()),
                    };
                    let created_at_valid = match created_at {
                        0 => SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or(Duration::from_secs(0))
                            .as_secs(),
                        _ => created_at as u64,
                    };
                    let (item_id_update, item_updated) = storage::valine::refresh_existing_item(
                        tx,
                        feed_id,
                        &item.id,
                        item_title,
                        content,
                        link,
                        author,
                        created_at_valid,
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
                            created_at_valid,
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
        let full_url = match &p.query {
            Some(v) => format!("{}?{}", p.url, v),
            None => p.url.to_owned(),
        };
        if p.status_code == StatusCode::NOT_MODIFIED {
            if storage::transaction(&self.db, |tx| {
                let feed_id = storage::feeds::get_feed_id_by_url(tx, &full_url);
                if feed_id.is_some() {
                    storage::feeds::upsert_feed(tx, &full_url, None);
                    false
                } else {
                    true
                }
            }) {
                println!("received status code 304 without existing feed, fetching again without cache: {full_url}");
                if let Ok(response) = proxy::http_https_get(&full_url, &self.proxy).await {
                    if let Err(e) = self.enqueue_response_body(&p.url, &p.query, response).await {
                        metrics::pipe_error();
                        println!("!! error enqueuing response body: {e:?}");
                    }
                }
            }
        } else {
            metrics::pipe_error();
            println!(
                "received status code {} handling feed {}: {}",
                p.status_code, full_url, v
            )
        }
    }

    async fn enqueue_response_body(
        &self,
        url: &str,
        query: &Option<String>,
        response_in: Response<Incoming>,
    ) -> Result<Response<Full<Bytes>>, common::PipeError> {
        let status_code = response_in.status();
        let return_empty_not_modified = match status_code {
            StatusCode::OK => {
                metrics::status_code_200();
                false
            }
            StatusCode::NOT_MODIFIED => {
                metrics::status_code_304();
                false
            }
            StatusCode::BAD_GATEWAY => {
                metrics::status_code_502();
                true
            }
            StatusCode::SERVICE_UNAVAILABLE => {
                metrics::status_code_503();
                true
            }
            _ => {
                println!("received status code {status_code}");
                false
            }
        };
        if return_empty_not_modified {
            let mut empty_not_modified = Response::builder().status(StatusCode::NOT_MODIFIED);
            if let Some(cache_control) = response_in.headers().get(header::CACHE_CONTROL) {
                empty_not_modified = empty_not_modified.header(header::CACHE_CONTROL, cache_control);
            }
            if let Some(last_modified) = response_in.headers().get(header::LAST_MODIFIED) {
                empty_not_modified = empty_not_modified.header(header::LAST_MODIFIED, last_modified);
            }
            if let Some(etag) = response_in.headers().get(header::ETAG) {
                empty_not_modified = empty_not_modified.header(header::ETAG, etag);
            }
            Ok(empty_not_modified.body(Full::from(""))?)
        } else {
            let (parts, incoming) = response_in.into_parts();
            let content = incoming.collect().await?.to_bytes();
            let parse_request = ParseRequest {
                status_code,
                url: url.to_owned(),
                query: query.to_owned(),
                body: content.to_owned(),
            };
            if let Err(e) = self.sender.send(parse_request).await {
                metrics::pipe_error();
                println!("!! error sending data to pipe: {e}");
            };
            Ok(Response::from_parts(parts, Full::new(content)))
        }
    }

    pub async fn enqueue_http(
        &self,
        uri: &str,
        req: Request<Incoming>,
    ) -> Result<Response<Full<Bytes>>, common::PipeError> {
        let query: Option<String> = req.uri().query().map(|x| x.to_owned());
        match proxy::http_call(uri, req).await {
            Ok(response) => self.enqueue_response_body(uri, &query, response).await,
            Err(error) => proxy::handle_error(handle_error(uri, format!("{error:?}"))),
        }
    }

    pub async fn enqueue_https(
        &self,
        uri: &str,
        req: Request<Incoming>,
    ) -> Result<Response<Full<Bytes>>, common::PipeError> {
        let query: Option<String> = req.uri().query().map(|x| x.to_owned());
        match proxy::https_call(uri, req, &self.proxy).await {
            Ok(response) => self.enqueue_response_body(uri, &query, response).await,
            Err(error) => proxy::handle_error(handle_error(uri, format!("{error:?}"))),
        }
    }

    pub async fn enqueue_invoke(
        &self,
        path: &str,
        req: Request<Incoming>,
    ) -> Result<Response<Full<Bytes>>, common::PipeError> {
        let body = match req.method() {
            &Method::POST => String::from_utf8(req.into_body().collect().await?.to_bytes().to_vec())?,
            _ => String::new(),
        };
        let (method, params) = path.split_once('/').unwrap_or((path, &body));
        let content = self.methods.evaluate("invoke", method, params, false).unwrap_or(body);
        let parse_request = ParseRequest {
            query: None,
            status_code: StatusCode::OK,
            body: Bytes::from(content.to_owned()),
            url: format!("rss-pipe://{}/{}", self.methods.get_name(), path),
        };
        if let Err(e) = self.sender.send(parse_request).await {
            metrics::pipe_error();
            println!("!! error sending data to pipe: {e}");
        };
        common::json_response(&content)
    }
}
