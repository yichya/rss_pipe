use std::collections::HashMap;

use rusqlite::Transaction;

use crate::storage::items;

pub fn get_items(tx: &Transaction, actions: &HashMap<String, String>) -> Vec<items::Item> {
    if let Some(with_ids) = actions.get("with_ids") {
        return items::get_items(tx, "with_ids", &with_ids.replace("%2C", ","));
    }
    if let Some(since_id) = actions.get("since_id") {
        return items::get_items(tx, "since_id", since_id);
    }
    vec![]
}

pub fn get_total_items(tx: &Transaction) -> String {
    format!(", \"total_items\": {}", items::get_total_items(tx, ""))
}

pub fn get_unread_item_ids(tx: &Transaction) -> String {
    let ids = items::get_unread_item_ids(tx);
    let ids_str: Vec<String> = ids.iter().map(|x| x.to_string()).collect();
    ids_str.join(",")
}

pub fn get_saved_item_ids(tx: &Transaction) -> String {
    let ids = items::get_saved_item_ids(tx);
    let ids_str: Vec<String> = ids.iter().map(|x| x.to_string()).collect();
    ids_str.join(",")
}

pub fn mark(tx: &Transaction, id: &str, kind: &str) {
    match kind {
        "read" => items::set_item_read_status(tx, id, "1"),
        "saved" => items::set_item_saved_status(tx, id, "1"),
        "unread" => items::set_item_read_status(tx, id, "0"),
        "unsaved" => items::set_item_saved_status(tx, id, "0"),
        _ => {}
    }
}
