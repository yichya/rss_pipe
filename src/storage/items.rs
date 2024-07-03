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
    pub created_on_time: u64,
}

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
    if let Ok(existing_id) = tx
        .prepare("select id from item where feed_id = ?1 and guid = ?2")
        .unwrap()
        .query_row([&feed_id.to_string(), guid], |row| row.get(0))
    {
        return (existing_id, false);
    }
    if let Ok(new_id) = tx
        .prepare("insert into item (feed_id, guid, title, author, content, url, create_time) values (?1, ?2, ?3, ?4, ?5, ?6, datetime(?7, 'unixepoch')) returning id")
        .unwrap()
        .query_row([feed_id.to_string(), guid.to_string(), title.to_string(), author.to_string(), html.to_string(), url.to_string(), created_at.to_string()], |row| row.get(0))
    {
        return (new_id, true)
    }
    (0, true)
}

pub fn set_item_read_status(tx: &Transaction, id: &str, status: &str) {
    if let Err(e) = tx
        .prepare("update item set is_read = ?1 where id = ?2")
        .unwrap()
        .execute([status, id])
    {
        println!("set item read status error: {}", e)
    }
}

pub fn set_item_saved_status(tx: &Transaction, id: &str, status: &str) {
    if let Err(e) = tx
        .prepare("update item set is_saved = ?1 where id = ?2")
        .unwrap()
        .execute([status, id])
    {
        println!("set item saved status error: {}", e)
    }
}

pub fn get_items(tx: &Transaction, filter_op: &str, filter_arg: &str) -> Vec<Item> {
    // validation for filter_arg
    for x in filter_arg.split(",") {
        if let Err(e) = x.parse::<u64>() {
            println!("invalid argument for get_items: {} ({})", filter_arg, e);
            return vec![];
        }
    }

    let result: Result<Vec<Item>, _> = tx
        .prepare(format!("select id, feed_id, title, author, url, content, is_saved, is_read, unixepoch(create_time) from item {} limit 50", if filter_op == "with_ids" {
            format!("where id in ({})", filter_arg)
        } else if filter_op == "since_id" {
            format!("where id > {}", filter_arg)
        } else {
            "".into() // todo: check if everything should be pulled here
        }).as_str())
        .unwrap()
        .query_map([], |row| Ok(Item{
            id: row.get(0)?,
            feed_id: row.get(1)?,
            title: row.get(2)?,
            author: row.get(3)?,
            url: row.get(4)?,
            html: row.get(5)?,
            is_saved: row.get(6)?,
            is_read: row.get(7)?,
            created_on_time: row.get(8)?,
        })).unwrap().collect();
    result.unwrap()
}

pub fn get_total_items(tx: &Transaction) -> u64 {
    tx.prepare("select count(*) from item")
        .unwrap()
        .query_row([], |row| row.get(0))
        .unwrap_or(0)
}

pub fn get_unread_item_ids(tx: &Transaction) -> Vec<u64> {
    let mut unread = tx
        .prepare("select id from item where is_read = 0") // make cargo fmt happy
        .unwrap();
    let ids: Result<Vec<u64>, _> = unread.query_map([], |row| row.get(0)).unwrap().collect();
    ids.unwrap_or_default()
}

pub fn get_saved_item_ids(tx: &Transaction) -> Vec<u64> {
    let mut saved = tx
        .prepare("select id from item where is_saved = 1") // make cargo fmt happy
        .unwrap();
    let ids: Result<Vec<u64>, _> = saved.query_map([], |row| row.get(0)).unwrap().collect();
    ids.unwrap_or_default()
}
