use std::collections::HashMap;
use std::pin::Pin;
use std::string::FromUtf8Error;
use std::task::{Context, Poll};

use bytes::Bytes;
use http::header::InvalidHeaderValue;
use http::uri::InvalidUri;
use http::{Request, Response, StatusCode};
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper_socks2::TlsError;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;

pub mod extract_content;
pub mod script;

/// 包装 TcpStream，在读取时先返回一个前缀字节
pub struct PrefixedStream {
    prefix: Option<u8>,
    inner: TcpStream,
}

impl PrefixedStream {
    pub fn new(prefix: u8, inner: TcpStream) -> Self {
        Self {
            prefix: Some(prefix),
            inner,
        }
    }
}

impl AsyncRead for PrefixedStream {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
        if let Some(byte) = self.prefix.take() {
            if buf.remaining() > 0 {
                buf.put_slice(&[byte]);
                return Poll::Ready(Ok(()));
            }
        }
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for PrefixedStream {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum PipeError {
    InvalidHeaderValueError,
    UnsupportedSchemeError,
    HyperError(hyper::Error),
    HyperLegacyError(hyper_util::client::legacy::Error),
    InvalidUri(InvalidUri),
    HttpError(http::Error),
    SocksError(TlsError),
    FromUtf8Error(FromUtf8Error),
    SerdeError(serde_json::error::Error),
}

impl From<hyper::Error> for PipeError {
    fn from(err: hyper::Error) -> Self {
        Self::HyperError(err)
    }
}

impl From<hyper_util::client::legacy::Error> for PipeError {
    fn from(err: hyper_util::client::legacy::Error) -> Self {
        Self::HyperLegacyError(err)
    }
}

impl From<TlsError> for PipeError {
    fn from(err: TlsError) -> Self {
        Self::SocksError(err)
    }
}

impl From<InvalidUri> for PipeError {
    fn from(err: InvalidUri) -> Self {
        Self::InvalidUri(err)
    }
}

impl From<http::Error> for PipeError {
    fn from(err: http::Error) -> Self {
        Self::HttpError(err)
    }
}

impl From<InvalidHeaderValue> for PipeError {
    fn from(_err: InvalidHeaderValue) -> Self {
        Self::InvalidHeaderValueError
    }
}

impl From<FromUtf8Error> for PipeError {
    fn from(err: FromUtf8Error) -> Self {
        Self::FromUtf8Error(err)
    }
}

impl From<serde_json::error::Error> for PipeError {
    fn from(value: serde_json::error::Error) -> Self {
        Self::SerdeError(value)
    }
}

pub async fn parse_request_body(req: Request<Incoming>) -> String {
    match req.into_body().collect().await {
        Ok(v) => String::from_utf8(v.to_bytes().to_vec()).unwrap_or_else(|e| {
            println!("!! error converting body to string: {e}");
            String::new()
        }),
        Err(e) => {
            println!("!! error reading request body: {e}");
            String::new()
        }
    }
}

pub fn get_request_params(query: &str, body: &str) -> HashMap<String, String> {
    [query, body]
        .iter()
        .flat_map(|i| i.split("&"))
        .filter_map(|j| {
            let p: Vec<&str> = j.split("=").collect();
            match (p.first(), p.last()) {
                (Some(k), Some(v)) if !k.is_empty() && !v.is_empty() => Some((k.to_string(), v.to_string())),
                _ => None,
            }
        })
        .collect()
}

pub fn json_response(v: &str) -> Result<Response<Full<Bytes>>, PipeError> {
    Response::builder()
        .status(StatusCode::OK)
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(http::header::ACCESS_CONTROL_ALLOW_HEADERS, "*")
        .header(http::header::ACCESS_CONTROL_ALLOW_METHODS, "*")
        .body(Full::from(v.to_owned()))
        .map_err(|e| e.into())
}

pub fn not_found() -> Result<Response<Full<Bytes>>, PipeError> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Full::from("not found"))
        .map_err(|e| e.into())
}

pub fn internal_server_error() -> Result<Response<Full<Bytes>>, PipeError> {
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .body(Full::from("internal server error"))
        .map_err(|e| e.into())
}
