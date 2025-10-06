use rusqlite::Transaction;
use serde::Serialize;

use crate::storage::feeds;

#[derive(Serialize, Debug)]
pub struct FeedFever {
    id: u64,
    favicon_id: u64,
    title: String,
    url: String,
    site_url: String,
    is_spark: u8,
    last_updated_on_time: u64,
}

pub fn get_all_feeds(tx: &Transaction) -> Vec<FeedFever> {
    feeds::get_all_feeds(tx)
        .unwrap_or_default()
        .iter()
        .map(|(feed, feed_url)| FeedFever {
            id: feed.id,
            favicon_id: feed.id,
            title: feed.title.clone(),
            url: feed_url.url.clone(),
            site_url: "".into(),
            is_spark: 0,
            last_updated_on_time: feed.last_updated,
        })
        .collect()
}

pub fn get_last_refreshed_time(tx: &Transaction) -> u64 {
    feeds::get_last_refreshed_time(tx)
}
