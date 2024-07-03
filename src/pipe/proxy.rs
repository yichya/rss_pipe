use std::str::FromStr;

use bytes::Bytes;
use http::{
    header::InvalidHeaderValue,
    uri::{InvalidUri, Scheme, Uri},
    StatusCode,
};
use http_body_util::{Empty, Full};
use hyper::{body::Incoming, Request, Response};
use hyper_socks2::{SocksConnector, TlsError};
use hyper_tls::HttpsConnector;
use hyper_util::client::legacy::{connect::HttpConnector, Client};

enum FetchRequest {
    GetUri(Uri),
    RequestIncoming(Request<Incoming>),
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum ProxyError {
    InvalidHeaderValueError,
    UnsupportedSchemeError,
    HyperError(hyper::Error),
    HyperLegacyError(hyper_util::client::legacy::Error),
    InvalidUri(InvalidUri),
    HttpError(http::Error),
    SocksError(TlsError),
}

impl From<hyper::Error> for ProxyError {
    fn from(err: hyper::Error) -> ProxyError {
        ProxyError::HyperError(err)
    }
}

impl From<hyper_util::client::legacy::Error> for ProxyError {
    fn from(err: hyper_util::client::legacy::Error) -> ProxyError {
        ProxyError::HyperLegacyError(err)
    }
}

impl From<TlsError> for ProxyError {
    fn from(err: TlsError) -> ProxyError {
        ProxyError::SocksError(err)
    }
}

impl From<InvalidUri> for ProxyError {
    fn from(err: InvalidUri) -> ProxyError {
        ProxyError::InvalidUri(err)
    }
}

impl From<http::Error> for ProxyError {
    fn from(err: http::Error) -> ProxyError {
        ProxyError::HttpError(err)
    }
}

impl From<InvalidHeaderValue> for ProxyError {
    fn from(_err: InvalidHeaderValue) -> ProxyError {
        ProxyError::InvalidHeaderValueError
    }
}

fn forward_uri<B>(forward_url: &str, req: &Request<B>) -> Result<Uri, InvalidUri> {
    let forward_uri = match req.uri().query() {
        Some(query) => format!("{}?{}", forward_url, query),
        None => forward_url.into(),
    };

    Uri::from_str(forward_uri.as_str())
}

fn create_proxied_request<B>(
    forward_url: &str,
    mut request: Request<B>,
) -> Result<Request<B>, ProxyError> {
    // todo: support gzip decompression later
    match request.headers_mut().entry("Accept-Encoding").into() {
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
        match request.headers_mut().entry("Host").into() {
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
) -> Result<Response<Incoming>, ProxyError> {
    match response {
        Ok(v) => Ok(v),
        Err(e) => Err(ProxyError::HyperLegacyError(e)),
    }
}

async fn https_fetch(request: FetchRequest, proxy: &str) -> Result<Response<Incoming>, ProxyError> {
    let builder = Client::builder(hyper_util::rt::TokioExecutor::new());
    let response = if proxy == "" {
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

pub async fn http_https_get(uri: &str, proxy: &str) -> Result<Response<Incoming>, ProxyError> {
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
    Err(ProxyError::UnsupportedSchemeError)
}

pub async fn https_call(
    forward_uri: &str,
    request: Request<Incoming>,
    proxy: &str,
) -> Result<Response<Incoming>, ProxyError> {
    let proxied_request = create_proxied_request(&forward_uri, request)?;
    https_fetch(FetchRequest::RequestIncoming(proxied_request), proxy).await
}

pub async fn http_call(
    forward_uri: &str,
    request: Request<Incoming>,
) -> Result<Response<Incoming>, ProxyError> {
    let proxied_request = create_proxied_request(&forward_uri, request)?;
    let response = Client::builder(hyper_util::rt::TokioExecutor::new())
        .build(HttpConnector::new())
        .request(proxied_request)
        .await;
    handle_response(response).await
}

pub fn handle_error(error: String) -> Result<Response<Full<Bytes>>, hyper::Error> {
    Ok(Response::builder()
        .status(StatusCode::BAD_GATEWAY)
        .body(Full::new(Bytes::from(error)))
        .unwrap())
}
