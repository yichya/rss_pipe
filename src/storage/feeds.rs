use rusqlite::Transaction;

#[derive(Debug)]
pub struct Feed {
    pub id: u64,
    pub title: String,
    pub last_updated: u64,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct FeedUrl {
    pub id: u64,
    pub feed_id: u64,
    pub url: String,
}

pub fn get_last_refreshed_time(tx: &Transaction) -> u64 {
    tx.query_row("select unixepoch(max(last_updated)) from feed", [], |row| row.get(0))
        .unwrap_or(0)
}

pub fn get_feed_id_by_url(tx: &Transaction, url: &str) -> Option<u64> {
    let mut stmt = tx
        .prepare("SELECT id, feed_id, url from feed_url where url = ?1 limit 1")
        .ok()?;
    let feed_urls = stmt
        .query_map([&url], |row| {
            Ok(FeedUrl {
                id: row.get(0)?,
                feed_id: row.get(1)?,
                url: row.get(2)?,
            })
        })
        .ok()?;
    feed_urls.flatten().next().map(|feed_url| feed_url.feed_id)
}

pub fn get_all_feeds(tx: &Transaction) -> Option<Vec<(Feed, FeedUrl)>> {
    let get_all_feeds_statement = tx.prepare(
        "with feeds as ( \
        select feed.id, feed.title, feed.last_updated, max(feed_url.id) as feed_url_id from feed join feed_url on feed.id = feed_url.feed_id group by feed.id \
    ) select feeds.id, feeds.title, unixepoch(feeds.last_updated), feed_url.id, feed_url.url from feeds join feed_url on feeds.feed_url_id = feed_url.id",
    );

    let all_feeds: Result<Vec<(Feed, FeedUrl)>, _> = get_all_feeds_statement
        .ok()?
        .query_map([], |row| {
            Ok((
                Feed {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    last_updated: row.get(2)?,
                },
                FeedUrl {
                    id: row.get(3)?,
                    feed_id: row.get(0)?,
                    url: row.get(4)?,
                },
            ))
        })
        .ok()?
        .collect();
    all_feeds.ok()
}

pub fn upsert_feed(tx: &Transaction, url: &str, title: &str) -> (u64, u64, bool) {
    if let Some(feed_id) = get_feed_id_by_url(tx, url) {
        let last_updated = tx
            .query_row(
                "update feed set title = ?1, last_updated = datetime() where id = ?2 returning unixepoch(last_updated)",
                [title, feed_id.to_string().as_str()],
                |row| row.get(0),
            )
            .unwrap_or(0);
        (feed_id, last_updated, false)
    } else {
        let feed_id = tx
            .query_row("insert into feed (title) values (?1) returning id", [&title], |row| {
                row.get(0)
            })
            .unwrap_or(0);
        let feed_url_id = tx
            .query_row(
                "insert into feed_url (feed_id, url) values (?1, ?2) returning id",
                [&feed_id.to_string(), url],
                |row| row.get(0),
            )
            .unwrap_or(0);
        (feed_id, feed_url_id, true)
    }
}
