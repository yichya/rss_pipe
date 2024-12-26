use rusqlite::{Connection, Transaction};

pub mod feeds;
pub mod items;

pub fn transaction<T>(db: &str, callback: impl Fn(&Transaction) -> T) -> T {
    let mut conn = Connection::open(db).unwrap();
    let tx = conn.transaction().unwrap();
    let result = callback(&tx);
    if let Err(e) = tx.commit() {
        println!("!! error committing transaction: {}", e);
    }
    result
}

pub fn migrations() {
    // todo: handle database migrations later
}
