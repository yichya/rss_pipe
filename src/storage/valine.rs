use rusqlite::Transaction;
use url::Url;

use crate::storage::blob::BlobStorage;

pub fn get_prefix(tx: &Transaction, id: u64) -> Option<String> {
    let domain: String = tx
        .query_row("select url from feed_url where feed_id = ?1", [id], |row| row.get(0))
        .ok()?;
    let parsed = Url::parse(&domain).ok()?;
    let scheme = parsed.scheme();
    let host = parsed.host_str()?;
    Some(format!("{}://{}", scheme, host))
}

pub fn find_item_id_by_url(tx: &Transaction, feed_id: u64, url: &str) -> Option<u64> {
    let prefix = get_prefix(tx, feed_id)?;
    tx.query_row("select id from item where url = ?1 limit 1", [prefix + url], |row| {
        row.get(0)
    })
    .ok()
}

pub fn refresh_existing_item(
    tx: &Transaction,
    feed_id: u64,
    guid: &str,
    title: &str,
    html: &str,
    url: &str,
    author: &str,
    created_at_str: &str,
) -> (u64, bool) {
    let existing_id = tx.query_row(
        "update item set counter = 0, update_time = current_timestamp, \
        guid = ?1, title = ?2, author = ?3, content = ?4, create_time = datetime(?5, 'unixepoch') \
        where feed_id = ?6 and url = ?7 and title = '' and author = '' returning id",
        rusqlite::params![guid, title, author, html, created_at_str, feed_id, url],
        |row| row.get(0),
    );
    match existing_id {
        Ok(id) => (id, true),
        Err(_) => (0, false),
    }
}

pub fn increment_item_counter(tx: &Transaction, id: u64) -> Option<usize> {
    tx.execute(
        "update item set counter = counter + 1, update_time = current_timestamp where id = ?",
        [id],
    )
    .ok()
}

pub fn get_comment_count(tx: &Transaction, item_id: u64) -> u64 {
    tx.query_row(
        "select count(*) from blob_storage where item_id = ?1 and reply_id is null",
        [item_id],
        |row| row.get(0),
    )
    .unwrap_or(0)
}

pub fn get_comment_by_item_id(tx: &Transaction, item_id: u64, limit: u64, skip: u64) -> Option<Vec<BlobStorage>> {
    let result: Result<Vec<BlobStorage>, _> = tx
        .prepare(
            "select id, item_id, reply_id, metadata, create_time, \
            iif(json_extract(metadata, '$.url') like '/%', cast(data as varchar), '') from blob_storage \
            where item_id = ?1 and reply_id is null order by id desc limit ?2 offset ?3",
        )
        .ok()?
        .query_map([item_id, limit, skip], |row| {
            Ok(BlobStorage {
                id: row.get(0)?,
                item_id: row.get(1)?,
                reply_id: row.get(2)?,
                metadata: row.get(3)?,
                create_time: row.get(4)?,
                data: row.get(5)?,
            })
        })
        .ok()?
        .collect();
    result.ok()
}

pub fn get_comment_by_reply_id(tx: &Transaction, feed_id: u64, reply_ids: &[String]) -> Option<Vec<BlobStorage>> {
    let reply_id_param = reply_ids.join("','");
    let result: Result<Vec<BlobStorage>, _> = tx
        .prepare(&format!(
            "select id, item_id, reply_id, metadata, create_time, \
            iif(json_extract(metadata, '$.url') like '/%', cast(data as varchar), '') from blob_storage \
            where reply_id in ('{}') and item_id in (select item_id from item where feed_id = ?1) order by id desc",
            reply_id_param
        ))
        .ok()?
        .query_map([feed_id], |row| {
            Ok(BlobStorage {
                id: row.get(0)?,
                item_id: row.get(1)?,
                reply_id: row.get(2)?,
                metadata: row.get(3)?,
                create_time: row.get(4)?,
                data: row.get(5)?,
            })
        })
        .ok()?
        .collect();
    result.ok()
}

pub fn save_comment(
    tx: &Transaction,
    item_id: u64,
    reply_id: Option<&str>,
    data: &str,
    metadata: &str,
) -> Result<i64, rusqlite::Error> {
    tx.query_row(
        "INSERT INTO blob_storage (item_id, reply_id, data, metadata) VALUES (?, ?, ?, ?) returning id",
        rusqlite::params![item_id, reply_id, data, metadata],
        |row| row.get(0),
    )
}
