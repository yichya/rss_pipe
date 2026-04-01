use std::collections::HashMap;

use base64::{Engine, prelude::BASE64_STANDARD};
use http::{HeaderMap, HeaderValue};
use openssl::hash::{MessageDigest, hash};
use serde::{Deserialize, Serialize};

mod comment;
mod counter;

const DEFAULT_DATETIME: &str = "2000-01-01T00:00:00.000Z";

pub struct Valine {
    bark: String,
    path: String,
    pub db: String,
    pub auth: String,
}

#[derive(Deserialize, Serialize)]
struct WherePath {
    url: Option<String>,
}

pub fn get_where_url(query_parsed: &HashMap<String, String>) -> String {
    let condition = query_parsed.get("where").map_or("", |v| v.as_str());
    let condition_parsed: Option<WherePath> = serde_json::from_str(condition).ok();
    condition_parsed.and_then(|v| v.url).unwrap_or_default()
}

pub fn md5_base64(input: &str) -> Option<String> {
    let digest = hash(MessageDigest::md5(), input.as_bytes()).ok()?;
    Some(BASE64_STANDARD.encode(digest))
}

pub fn get_feed_id(auth: &str, headers: &HeaderMap<HeaderValue>) -> Option<u64> {
    let header_split: Vec<&str> = headers.get("x-lc-id").map(|c| c.to_str().ok())??.split("-").collect();
    let header_first = header_split.first()?;
    let header_last = header_split.last()?;
    let authentication = md5_base64(&format!("{}:{}", auth, header_last))?;
    if header_first == &authentication {
        u64::from_str_radix(header_last, 16).ok()
    } else {
        println!("!! error: invalid auth {}, expected {}", header_first, authentication);
        None
    }
}

impl Valine {
    pub fn new(db: &str, auth: &str, bark: &str, path: &str) -> Self {
        Self {
            db: db.to_owned(),
            auth: auth.to_owned(),
            bark: bark.to_owned(),
            path: format!("/{}/", path),
        }
    }
}
