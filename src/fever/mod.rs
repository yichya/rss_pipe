use std::collections::HashMap;

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{Request, Response, body::Incoming};
use rusqlite::Transaction;
use serde::Serialize;

use crate::{common, storage};

mod feeds;
mod items;

async fn parse_request_actions(req: Request<Incoming>) -> HashMap<String, String> {
    let query_string = req.uri().query().unwrap_or("").to_owned();
    let body = match req.into_body().collect().await {
        Ok(v) => String::from_utf8(v.to_bytes().to_vec()).unwrap_or_else(|e| {
            println!("!! error converting body to string: {e}");
            "".into()
        }),
        Err(e) => {
            println!("!! error reading request body: {e}");
            "".into()
        }
    };
    common::get_request_params(&query_string, &body)
}

fn unauthorized() -> Result<Response<Full<Bytes>>, common::PipeError> {
    common::json_response(Bytes::from("{\"api_version\": 3, \"auth\": 0}"))
}

fn return_with_base_response<T: Serialize>(
    tx: &Transaction,
    k: &str,
    v: &T,
    a: &str,
) -> Result<Response<Full<Bytes>>, common::PipeError> {
    let kv_part = if k.is_empty() {
        "".into()
    } else {
        format!(", \"{}\": {}", k, serde_json::to_string(v).unwrap_or("null".into()))
    };
    let result = format!(
        "{{\"api_version\": 3, \"auth\": 1, \"last_refreshed_on_time\": {}{}{}}}",
        feeds::get_last_refreshed_time(tx),
        kv_part,
        a
    );
    common::json_response(Bytes::from(result))
}

pub async fn fever(db: &str, auth: &str, req: Request<Incoming>) -> Result<Response<Full<Bytes>>, common::PipeError> {
    let empty = Vec::<u8>::new();
    let actions = parse_request_actions(req).await;
    if let Some(api_key) = actions.get("api_key") {
        if api_key.to_lowercase() == auth {
            return storage::transaction(db, |tx| {
                if actions.contains_key("feeds") {
                    return return_with_base_response(tx, "feeds", &feeds::get_all_feeds(tx), "");
                }
                if actions.contains_key("items") {
                    return return_with_base_response(
                        tx,
                        "items",
                        &items::get_items(tx, &actions),
                        items::get_total_items(tx).as_str(),
                    );
                }
                if actions.contains_key("unread_item_ids") {
                    return return_with_base_response(tx, "unread_item_ids", &items::get_unread_item_ids(tx), "");
                }
                if actions.contains_key("saved_item_ids") {
                    return return_with_base_response(tx, "saved_item_ids", &items::get_saved_item_ids(tx), "");
                }
                // unimplemented read operations
                if actions.contains_key("links") {
                    return return_with_base_response(tx, "links", &empty, "");
                }
                if actions.contains_key("groups") {
                    return return_with_base_response(tx, "groups", &empty, "");
                }
                if actions.contains_key("favicons") {
                    return return_with_base_response(tx, "favicons", &empty, "");
                }
                // write operations
                if let Some(mark) = actions.get("mark") {
                    if let Some(kind) = actions.get("as") {
                        if let Some(id) = actions.get("id") {
                            if mark.as_str() == "item" {
                                items::mark(tx, id, kind)
                            }
                        }
                    }
                }
                // default handler
                return_with_base_response(tx, "", &Vec::<u8>::new(), "")
            });
        } else {
            println!("!! token not valid, provided {api_key}, expect {auth}")
        }
    }
    unauthorized()
}
