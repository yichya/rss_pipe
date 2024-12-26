use bytes::Bytes;
use http_body_util::Full;
use hyper::{Method, Request};
use hyper_util::client::legacy::{connect::HttpConnector, Client};
use serde::Serialize;

mod extract_content;

#[derive(Debug, Serialize)]
struct BarkRequest {
    title: String,
    group: String,
    body: String,
    url: Option<String>,
}

impl BarkRequest {
    pub async fn send_notification(&self, destination: &str) {
        let builder = Client::builder(hyper_util::rt::TokioExecutor::new());
        let client = builder.build(HttpConnector::new());
        let body = serde_json::to_string(self).unwrap_or("null".to_owned());
        println!("building bark push request: {}", body);
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
                    println!("!! error received from bark push: {}", e);
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
    link: &str,
    destination: &str,
) {
    let extracted_content = extract_content::extract_content(item_title, content, 250);
    BarkRequest {
        title: feed_title.into(),
        group: "rss_pipe_rust".into(),
        body: extracted_content,
        url: match link {
            "" => None,
            _ => Some(link.into()),
        },
    }
    .send_notification(destination)
    .await;
}
