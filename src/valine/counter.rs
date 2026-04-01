use std::collections::HashMap;

use bytes::Bytes;
use http::{Request, Response};
use http_body_util::Full;
use hyper::body::Incoming;
use serde::{Deserialize, Serialize};
use url::{Url, form_urlencoded};

use crate::{common, storage, valine};

#[derive(Deserialize, Serialize)]
struct Counter {
    pub time: Option<u64>,
    pub url: Option<String>,
    pub xid: Option<String>,
    pub title: Option<String>,

    #[serde(rename = "objectId")]
    pub id: Option<String>,

    #[serde(rename = "createdAt")]
    pub created_at: Option<String>,

    #[serde(rename = "updatedAt")]
    pub updated_at: Option<String>,
}

fn to_counter(i: &storage::items::Item) -> Counter {
    let path = Url::parse(&i.url).map_or(i.url.to_owned(), |u| u.path().to_owned());
    Counter {
        id: Some(i.id.to_string()),
        time: Some(i.counter),
        url: Some(path.to_owned()),
        xid: Some(path.to_owned()),
        title: Some(i.title.to_owned()),
        created_at: Some(valine::DEFAULT_DATETIME.to_owned()),
        updated_at: Some(valine::DEFAULT_DATETIME.to_owned()),
    }
}

impl valine::Valine {
    pub async fn new_counter(&self, feed_id: u64, body: &str) -> Result<Response<Full<Bytes>>, common::PipeError> {
        if body.is_empty() {
            common::not_found()
        } else {
            let counter_data: Counter = serde_json::from_str(body)?;
            let path = counter_data.url.unwrap_or_default();
            if path.is_empty() {
                common::internal_server_error()
            } else {
                let id = storage::transaction(&self.db, |tx| {
                    storage::valine::get_prefix(tx, feed_id).map_or(0, |prefix| {
                        let url = format!("{}{}", prefix, path);
                        let (new_item_id, _) = storage::items::create_item(tx, feed_id, &url, "", "", &url, "", 0);
                        storage::valine::increment_item_counter(tx, new_item_id);
                        new_item_id
                    })
                });
                common::json_response(&format!(
                    "{{\"objectId\": \"{}\", \"createdAt\": \"{}\"}}",
                    id,
                    valine::DEFAULT_DATETIME
                ))
            }
        }
    }
    pub fn get_counter(&self, feed_id: u64, query: &str) -> Result<Response<Full<Bytes>>, common::PipeError> {
        let query_parsed: HashMap<String, String> = form_urlencoded::parse(query.as_bytes()).into_owned().collect();
        let url = valine::get_where_url(&query_parsed);
        if url.is_empty() {
            common::not_found()
        } else {
            let counters: Vec<Counter> = storage::transaction(&self.db, |tx| {
                storage::valine::find_item_id_by_url(tx, feed_id, &url).map_or(vec![], |c| {
                    storage::items::get_items(tx, "with_ids", &c.to_string()).unwrap_or_default()
                })
            })
            .iter()
            .map(to_counter)
            .collect();
            common::json_response(&format!(
                "{{\"results\": {}}}",
                serde_json::to_string(&counters).unwrap_or("[]".to_owned())
            ))
        }
    }

    pub fn increment_counter(&self, id: &str) -> Result<Response<Full<Bytes>>, common::PipeError> {
        match id.parse() {
            Ok(v) => storage::transaction(&self.db, |tx| {
                storage::valine::increment_item_counter(tx, v);
                common::json_response(&format!(
                    "{{\"objectId\": \"{}\", \"updatedAt\": \"{}\"}}",
                    id,
                    valine::DEFAULT_DATETIME,
                ))
            }),
            Err(_) => common::not_found(),
        }
    }

    pub async fn handle_counter(&self, req: Request<Incoming>) -> Result<Response<Full<Bytes>>, common::PipeError> {
        let method = req.method().to_owned();
        let feed_id = valine::get_feed_id(&self.auth, req.headers());
        if method == http::Method::OPTIONS {
            common::json_response("{}")
        } else if let Some(id) = feed_id {
            let path = req.uri().path().to_owned();
            let query = req.uri().query().unwrap_or("").to_owned();
            let body = common::parse_request_body(req).await;
            if method == http::Method::POST {
                self.new_counter(id, &body).await
            } else if method == http::Method::PUT {
                self.increment_counter(path.split("/").last().unwrap_or("0"))
            } else {
                self.get_counter(id, &query)
            }
        } else {
            common::not_found()
        }
    }
}
