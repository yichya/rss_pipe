use bytes::Bytes;
use http_body_util::Full;
use hyper::{Method, Request};
use hyper_util::client::legacy::{Client, connect::HttpConnector};
use serde::Serialize;

use crate::common::extract_content;

#[derive(Debug, Serialize)]
struct BarkRequest {
    body: String,
    title: String,
    group: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image: Option<String>,
}

impl BarkRequest {
    pub async fn send_notification(&self, destination: &str) {
        let builder = Client::builder(hyper_util::rt::TokioExecutor::new());
        let client = builder.build(HttpConnector::new());
        let body = serde_json::to_string(self).unwrap_or("null".to_owned());
        println!("building bark push request: {body}");
        let req = Request::builder()
            .uri(destination)
            .method(Method::POST)
            .header("Content-Type", "application/json")
            .body(Full::new(Bytes::from(body)));
        match req {
            Ok(v) => match client.request(v).await {
                Ok(s) => {
                    println!("complete bark push {}", s.status());
                }
                Err(e) => {
                    println!("!! error received from bark push: {e}");
                }
            },
            Err(_) => {
                println!(
                    "======== Bark Preview ========\n{}\n{}\n==============================",
                    self.title,
                    self.body.trim_end_matches("\n")
                )
            }
        }
    }
}

pub async fn send_notification(
    feed_title: &str,
    item_title: &str,
    content: &str,
    group: &str,
    link: Option<String>,
    image: Option<String>,
    destination: &str,
) {
    let extracted_content = extract_content::extract_content(item_title, content, 250);
    BarkRequest {
        title: feed_title.into(),
        group: if group.is_empty() {
            "rss_pipe_rust".to_owned()
        } else {
            group.to_owned()
        },
        body: extracted_content,
        url: link,
        image,
    }
    .send_notification(destination)
    .await;
}
