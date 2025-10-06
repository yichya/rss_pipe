use std::str::FromStr;

use bytes::Bytes;
use http::{
    StatusCode,
    uri::{InvalidUri, Scheme, Uri},
};
use http_body_util::{Empty, Full};
use hyper::{Request, Response, body::Incoming};
use hyper_socks2::SocksConnector;
use hyper_tls::HttpsConnector;
use hyper_util::client::legacy::{Client, connect::HttpConnector};

use crate::common::PipeError;

enum FetchRequest {
    GetUri(Uri),
    RequestIncoming(Request<Incoming>),
}

fn forward_uri<B>(forward_url: &str, req: &Request<B>) -> Result<Uri, InvalidUri> {
    let forward_uri = match req.uri().query() {
        Some(query) => format!("{forward_url}?{query}"),
        None => forward_url.into(),
    };

    Uri::from_str(forward_uri.as_str())
}

fn create_proxied_request<B>(forward_url: &str, mut request: Request<B>) -> Result<Request<B>, PipeError> {
    // todo: support gzip decompression later
    match request.headers_mut().entry("Accept-Encoding") {
        hyper::header::Entry::Vacant(entry) => {
            entry.insert("identity".parse()?);
        }
        hyper::header::Entry::Occupied(mut entry) => {
            entry.insert("identity".parse()?);
        }
    }
    // replace host
    let destination = forward_uri(forward_url, &request)?;
    if let Some(host) = destination.host() {
        match request.headers_mut().entry("Host") {
            hyper::header::Entry::Vacant(entry) => {
                entry.insert(host.parse()?);
            }
            hyper::header::Entry::Occupied(mut entry) => {
                entry.insert(host.parse()?);
            }
        }
    }
    *request.uri_mut() = destination;
    Ok(request)
}

async fn handle_response(
    response: Result<Response<Incoming>, hyper_util::client::legacy::Error>,
) -> Result<Response<Incoming>, PipeError> {
    match response {
        Ok(v) => Ok(v),
        Err(e) => Err(PipeError::HyperLegacyError(e)),
    }
}

async fn https_fetch(request: FetchRequest, proxy: &str) -> Result<Response<Incoming>, PipeError> {
    let builder = Client::builder(hyper_util::rt::TokioExecutor::new());
    let response = if proxy.is_empty() {
        match request {
            FetchRequest::RequestIncoming(request) => {
                let client = builder.build(HttpsConnector::new());
                client.request(request).await
            }
            FetchRequest::GetUri(uri) => {
                let client = builder.build(HttpsConnector::new());
                let request = Request::builder().uri(uri).body(Empty::<Bytes>::new())?;
                client.request(request).await
            }
        }
    } else {
        let mut connector = HttpConnector::new();
        connector.enforce_http(false);
        let proxy = SocksConnector {
            proxy_addr: Uri::from_str(proxy)?, // scheme is required by HttpConnector
            auth: None,
            connector,
        }
        .with_tls()?;
        match request {
            FetchRequest::RequestIncoming(request) => {
                let client = builder.build(proxy);
                client.request(request).await
            }
            FetchRequest::GetUri(uri) => {
                let client = builder.build(proxy);
                let request = Request::builder().uri(uri).body(Empty::<Bytes>::new())?;
                client.request(request).await
            }
        }
    };
    handle_response(response).await
}

pub async fn http_https_get(uri: &str, proxy: &str) -> Result<Response<Incoming>, PipeError> {
    let uri_parsed = Uri::from_str(uri)?;
    if let Some(a) = uri_parsed.scheme() {
        if a == &Scheme::HTTPS {
            return https_fetch(FetchRequest::GetUri(uri_parsed), proxy).await;
        } else if a == &Scheme::HTTP {
            let builder = Client::builder(hyper_util::rt::TokioExecutor::new());
            let request = Request::builder().uri(uri).body(Empty::<Bytes>::new())?;
            let response = builder.build(HttpConnector::new()).request(request).await;
            return handle_response(response).await;
        };
    };
    Err(PipeError::UnsupportedSchemeError)
}

pub async fn https_call(
    forward_uri: &str,
    request: Request<Incoming>,
    proxy: &str,
) -> Result<Response<Incoming>, PipeError> {
    let proxied_request = create_proxied_request(forward_uri, request)?;
    https_fetch(FetchRequest::RequestIncoming(proxied_request), proxy).await
}

pub async fn http_call(forward_uri: &str, request: Request<Incoming>) -> Result<Response<Incoming>, PipeError> {
    let proxied_request = create_proxied_request(forward_uri, request)?;
    let response = Client::builder(hyper_util::rt::TokioExecutor::new())
        .build(HttpConnector::new())
        .request(proxied_request)
        .await;
    handle_response(response).await
}

pub fn handle_error(error: String) -> Result<Response<Full<Bytes>>, PipeError> {
    match Response::builder()
        .status(StatusCode::BAD_GATEWAY)
        .body(Full::new(Bytes::from(error)))
    {
        Ok(v) => Ok(v),
        Err(e) => Err(e.into()),
    }
}
