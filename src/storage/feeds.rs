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
    tx.query_row("SELECT feed_id from feed_url where url = ?1 ", [&url], |row| row.get(0))
        .ok()
}

pub fn get_all_feeds(tx: &Transaction) -> Option<Vec<(Feed, FeedUrl)>> {
    let get_all_feeds_statement = tx.prepare(
        "with f as ( \
            select feed.id, feed.title, feed.last_updated, max(feed_url.id) as feed_url_id \
            from feed join feed_url on feed.id = feed_url.feed_id group by feed.id \
        ) select f.id, f.title, unixepoch(f.last_updated), u.id, u.url from f join feed_url u on f.feed_url_id = u.id",
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

pub fn upsert_feed(tx: &Transaction, url: &str, title: Option<&str>) -> (u64, u64, bool) {
    if let Some(feed_id) = get_feed_id_by_url(tx, url) {
        let last_updated = tx
            .query_row(
                "update feed set title = iif(?1 is null, title, ?2), last_updated = datetime() where id = ?3 returning unixepoch(last_updated)",
                rusqlite::params![title, title, feed_id],
                |row| row.get(0),
            )
            .unwrap_or(0);
        (feed_id, last_updated, false)
    } else if let Ok(feed_id) = tx.query_row("insert into feed (title) values (?1) returning id", [&title], |row| {
        row.get::<usize, u64>(0)
    }) {
        let feed_url_id = tx
            .query_row(
                "insert into feed_url (feed_id, url) values (?1, ?2) returning id",
                rusqlite::params![feed_id, url],
                |row| row.get(0),
            )
            .unwrap_or(0);
        (feed_id, feed_url_id, true)
    } else {
        (0, 0, false)
    }
}
