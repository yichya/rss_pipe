use std::collections::HashMap;

use bytes::Bytes;
use http::{Request, Response};
use http_body_util::Full;
use hyper::body::Incoming;
use serde::{Deserialize, Serialize};
use url::form_urlencoded;

use crate::{common, push, storage, valine};

#[derive(Deserialize, Serialize)]
pub struct Date {
    iso: String,

    #[serde(rename = "__type")]
    underline_type: String,
}

impl Date {
    pub fn from_datetime(v: &str) -> Self {
        Self {
            iso: format!(
                "{}T{}.000Z",
                v.split_whitespace().next().unwrap_or(""),
                v.split_whitespace().nth(1).unwrap_or("")
            ),
            underline_type: "Date".to_owned(),
        }
    }
}

#[derive(Deserialize, Serialize)]
pub struct Metadata {
    ip: Option<String>,
    ua: Option<String>,
    pid: Option<String>,
    url: Option<String>,
    link: Option<String>,
    mail: Option<String>,
    nick: Option<String>,
    content_type: Option<String>,

    #[serde(rename = "QQAvatar")]
    avatar: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub struct Comment {
    #[serde(rename = "objectId")]
    id: Option<String>,
    #[serde(rename = "rid")]
    reply_id: Option<String>,
    #[serde(rename = "comment")]
    data: Option<String>,

    #[serde(rename = "insertedAt")]
    inserted_at: Option<Date>,
    #[serde(rename = "createdAt")]
    created_at: Option<String>,
    #[serde(rename = "updatedAt")]
    updated_at: Option<String>,

    #[serde(flatten)]
    metadata: Metadata,
}

fn get_rids_from_cql_template(v: &str) -> Vec<String> {
    let start = v.find("in (").map(|i| i + 4).unwrap_or(0);
    let end = v[start..].find(")").map(|i| i + start).unwrap_or(v.len());
    v[start..end]
        .split(',')
        .filter_map(|s| {
            let trimmed = s.trim();
            if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() > 1 {
                Some(trimmed[1..trimmed.len() - 1].to_string())
            } else {
                None
            }
        })
        .filter(|s| s.chars().all(|c| c.is_ascii_hexdigit()))
        .collect()
}

fn to_comment(blob: &storage::blob::BlobStorage) -> Comment {
    let metadata = serde_json::from_str(&blob.metadata).unwrap_or(Metadata {
        ip: None,
        ua: None,
        pid: None,
        url: None,
        link: None,
        mail: None,
        avatar: None,
        content_type: None,
        nick: Some("Anonymous".to_owned()),
    });

    Comment {
        metadata,
        created_at: None,
        updated_at: None,
        id: Some(blob.id.to_string()),
        data: Some(blob.data.to_owned()),
        reply_id: blob.reply_id.map(|i| i.to_string()),
        inserted_at: Some(Date::from_datetime(&blob.create_time)),
    }
}

impl valine::Valine {
    pub async fn new_comment(&self, feed_id: u64, body: &str) -> Result<Response<Full<Bytes>>, common::PipeError> {
        if body.is_empty() {
            common::not_found()
        } else {
            let c: Comment = serde_json::from_str(body)?;
            let data = c.data.as_deref().unwrap_or("");
            let url = c.metadata.url.as_deref().unwrap_or("");
            let v = storage::transaction(&self.db, |tx| {
                storage::valine::find_item_id_by_url(tx, feed_id, url).and_then(|item_id| {
                    storage::valine::save_comment(
                        tx,
                        item_id,
                        c.reply_id.as_deref(),
                        data,
                        &serde_json::to_string(&c.metadata).unwrap_or("{}".to_owned()),
                    )
                    .ok()
                })
            })
            .map(async |object_id| {
                if url != self.path {
                    push::bark::send_notification(
                        &format!("New comment {}", url),
                        "",
                        data,
                        "rss_pipe_valine",
                        None, // todo: link
                        None,
                        &self.bark,
                    )
                    .await;
                }
                common::json_response(&format!(
                    "{{\"objectId\": \"{}\", \"createdAt\": \"{}\"}}",
                    object_id,
                    valine::DEFAULT_DATETIME
                ))
            });
            match v {
                Some(v) => v.await,
                None => common::internal_server_error(),
            }
        }
    }

    pub fn get_comment_count(&self, feed_id: u64, url: &str) -> Result<Response<Full<Bytes>>, common::PipeError> {
        if url.is_empty() {
            common::not_found()
        } else {
            let count = storage::transaction(&self.db, |tx| {
                storage::valine::find_item_id_by_url(tx, feed_id, url).map_or(0, |id| {
                    storage::valine::get_comment_count(tx, id) // make cargo fmt happy
                })
            });
            common::json_response(&format!("{{\"results\": [], \"count\": {}}}", count))
        }
    }

    pub fn get_comment_paged(
        &self,
        feed_id: u64,
        url: &str,
        limit: u64,
        skip: u64,
    ) -> Result<Response<Full<Bytes>>, common::PipeError> {
        if url.is_empty() {
            common::not_found()
        } else if limit == 0 {
            common::json_response("{\"results\": [], \"count\": 0}")
        } else {
            let comments: Vec<Comment> = storage::transaction(&self.db, |tx| {
                storage::valine::find_item_id_by_url(tx, feed_id, url).map_or(vec![], |id| {
                    storage::valine::get_comment_by_item_id(tx, id, limit, skip).unwrap_or_default()
                })
            })
            .iter()
            .map(to_comment)
            .collect();
            common::json_response(&format!(
                "{{\"results\": {}, \"count\": 0}}",
                serde_json::to_string(&comments).unwrap_or("[]".to_owned())
            ))
        }
    }

    pub fn get_all_replies(&self, feed_id: u64, rids: &[String]) -> Result<Response<Full<Bytes>>, common::PipeError> {
        if rids.is_empty() {
            common::json_response("{\"results\": [], \"className\": \"Comment\"}")
        } else {
            let comments: Vec<Comment> = storage::transaction(&self.db, |tx| {
                storage::valine::get_comment_by_reply_id(tx, feed_id, rids).unwrap_or_default()
            })
            .iter()
            .map(to_comment)
            .collect();
            common::json_response(&format!(
                "{{\"results\": {}, \"className\": \"Comment\"}}",
                serde_json::to_string(&comments).unwrap_or("[]".to_owned())
            ))
        }
    }

    pub async fn handle_comment(&self, req: Request<Incoming>) -> Result<Response<Full<Bytes>>, common::PipeError> {
        let method = req.method().to_owned();
        let feed_id = valine::get_feed_id(&self.auth, req.headers());
        if method == http::Method::OPTIONS {
            common::json_response("{}")
        } else if let Some(id) = feed_id {
            let query = req.uri().query().unwrap_or("").to_owned();
            let query_parsed: HashMap<String, String> = form_urlencoded::parse(query.as_bytes()).into_owned().collect();
            let count = query_parsed.get("count").map_or("", |v| v.as_str());
            let body = common::parse_request_body(req).await;
            let url = valine::get_where_url(&query_parsed);

            if method == http::Method::POST {
                self.new_comment(id, &body).await
            } else if count == "1" {
                self.get_comment_count(id, &url)
            } else {
                let skip: u64 = query_parsed.get("skip").and_then(|v| v.parse().ok()).unwrap_or(0);
                let limit: u64 = query_parsed.get("limit").and_then(|v| v.parse().ok()).unwrap_or(0);
                self.get_comment_paged(id, &url, limit, skip)
            }
        } else {
            common::not_found()
        }
    }

    pub async fn handle_cloud_query(&self, req: Request<Incoming>) -> Result<Response<Full<Bytes>>, common::PipeError> {
        let method = req.method().to_owned();
        let feed_id = valine::get_feed_id(&self.auth, req.headers());
        if method == http::Method::OPTIONS {
            common::json_response("{}")
        } else if let Some(id) = feed_id {
            let query = req.uri().query().unwrap_or("").to_owned();
            let query_parsed: HashMap<String, String> = form_urlencoded::parse(query.as_bytes()).into_owned().collect();
            let cql = query_parsed.get("cql").map(|v| v.as_str()).unwrap_or("");
            let rids = get_rids_from_cql_template(cql);
            self.get_all_replies(id, &rids)
        } else {
            common::not_found()
        }
    }
}
