use rusqlite::Transaction;
use serde::Serialize;

#[derive(Serialize, Debug)]
pub struct Item {
    pub id: u64,
    pub feed_id: u64,
    pub title: String,
    pub author: String,
    pub html: String,
    pub url: String,
    pub is_saved: u8,
    pub is_read: u8,
    pub counter: u64,
    pub created_on_time: u64,
}

#[allow(clippy::too_many_arguments)]
pub fn create_item(
    tx: &Transaction,
    feed_id: u64,
    guid: &str,
    title: &str,
    html: &str,
    url: &str,
    author: &str,
    created_at: u64,
) -> (u64, bool) {
    if let Ok(existing_id) = tx.query_row(
        "select id from item where feed_id = ?1 and guid = ?2",
        rusqlite::params![feed_id, guid],
        |row| row.get(0),
    ) {
        return (existing_id, false);
    }
    if let Ok(new_id) = tx.query_row(
        "insert into item (feed_id, guid, title, author, content, url, create_time) \
        values (?1, ?2, ?3, ?4, ?5, ?6, datetime(?7, 'unixepoch')) returning id",
        rusqlite::params![feed_id, guid, title, author, html, url, created_at],
        |row| row.get(0),
    ) {
        return (new_id, true);
    }
    (0, true)
}

pub fn set_item_read_status(tx: &Transaction, id: &str, status: &str) {
    if let Err(e) = tx.execute("update item set is_read = ?1 where id = ?2", [status, id]) {
        println!("!! error setting item read status: {e}")
    }
}

pub fn set_item_saved_status(tx: &Transaction, id: &str, status: &str) {
    if let Err(e) = tx.execute("update item set is_saved = ?1 where id = ?2", [status, id]) {
        println!("!! error setting item saved status: {e}")
    }
}

pub fn get_items(tx: &Transaction, filter_op: &str, filter_arg: &str, feed_ids: Option<&str>) -> Option<Vec<Item>> {
    let mut conditions = Vec::new();
    let mut order_desc = false;

    match filter_op {
        "max_id" => {
            order_desc = true;
            if let Ok(id) = filter_arg.parse::<u64>() {
                if id > 0 {
                    conditions.push(format!("id < {id}"));
                }
            }
        }
        "with_ids" => {
            if !filter_arg.is_empty() {
                conditions.push(format!("id in ({})", validate_comma_ids("with_ids", filter_arg)?));
            }
        }
        _ => {
            if let Ok(id) = filter_arg.parse::<u64>() {
                if id > 0 {
                    conditions.push(format!("id > {id}"));
                }
            }
        }
    }

    if let Some(fids) = feed_ids {
        if !fids.is_empty() {
            conditions.push(format!("feed_id in ({})", validate_comma_ids("feed_ids", fids)?));
        }
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("where {}", conditions.join(" and "))
    };

    let order = if order_desc { "desc" } else { "asc" };
    let statement = format!(
        "select {} from item {} order by id {} limit 50",
        "id, feed_id, title, author, url, content, is_saved, is_read, counter, unixepoch(create_time)",
        where_clause,
        order,
    );

    let result: Result<Vec<Item>, _> = tx
        .prepare(&statement)
        .ok()?
        .query_map([], |row| {
            Ok(Item {
                id: row.get(0)?,
                feed_id: row.get(1)?,
                title: row.get(2)?,
                author: row.get(3)?,
                url: row.get(4)?,
                html: row.get(5)?,
                is_saved: row.get(6)?,
                is_read: row.get(7)?,
                counter: row.get(8)?,
                created_on_time: row.get(9)?,
            })
        })
        .ok()?
        .collect();
    result.ok()
}

fn validate_comma_ids<'a>(label: &str, raw: &'a str) -> Option<&'a str> {
    for x in raw.split(",") {
        if x.is_empty() {
            continue;
        }
        if x.parse::<u64>().is_err() {
            println!("!! parse argument failed for {label}: {raw}");
            return None;
        }
    }
    Some(raw)
}

pub fn get_total_items(tx: &Transaction, extra_filter: &str) -> u64 {
    tx.query_row(&format!("select count(*) from item {extra_filter}"), [], |row| {
        row.get(0)
    })
    .unwrap_or(0)
}

pub fn get_unread_item_ids(tx: &Transaction) -> Option<Vec<u64>> {
    let unread = tx.prepare("select id from item where is_read = 0");
    let ids: Result<Vec<u64>, _> = unread.ok()?.query_map([], |row| row.get(0)).ok()?.collect();
    ids.ok()
}

pub fn get_saved_item_ids(tx: &Transaction) -> Option<Vec<u64>> {
    let saved = tx.prepare("select id from item where is_saved = 1");
    let ids: Result<Vec<u64>, _> = saved.ok()?.query_map([], |row| row.get(0)).ok()?.collect();
    ids.ok()
}
