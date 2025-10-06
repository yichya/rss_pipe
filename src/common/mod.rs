use std::collections::HashMap;
use std::string::FromUtf8Error;

use bytes::Bytes;
use http::header::InvalidHeaderValue;
use http::uri::InvalidUri;
use http::{Response, StatusCode};
use http_body_util::Full;
use hyper_socks2::TlsError;

pub mod extract_content;
pub mod script;

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
}

impl From<hyper::Error> for PipeError {
    fn from(err: hyper::Error) -> PipeError {
        PipeError::HyperError(err)
    }
}

impl From<hyper_util::client::legacy::Error> for PipeError {
    fn from(err: hyper_util::client::legacy::Error) -> PipeError {
        PipeError::HyperLegacyError(err)
    }
}

impl From<TlsError> for PipeError {
    fn from(err: TlsError) -> PipeError {
        PipeError::SocksError(err)
    }
}

impl From<InvalidUri> for PipeError {
    fn from(err: InvalidUri) -> PipeError {
        PipeError::InvalidUri(err)
    }
}

impl From<http::Error> for PipeError {
    fn from(err: http::Error) -> PipeError {
        PipeError::HttpError(err)
    }
}

impl From<InvalidHeaderValue> for PipeError {
    fn from(_err: InvalidHeaderValue) -> PipeError {
        PipeError::InvalidHeaderValueError
    }
}

impl From<FromUtf8Error> for PipeError {
    fn from(err: FromUtf8Error) -> PipeError {
        PipeError::FromUtf8Error(err)
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

pub fn json_response(v: Bytes) -> Result<Response<Full<Bytes>>, PipeError> {
    Response::builder()
        .status(StatusCode::OK)
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(Full::new(v))
        .map_err(|e| e.into())
}

pub fn not_found() -> Result<Response<Full<Bytes>>, PipeError> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Full::new(Bytes::from("not found")))
        .map_err(|e| e.into())
}
