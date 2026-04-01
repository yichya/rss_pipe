use rusqlite::{Connection, Transaction, fallible_iterator::FallibleIterator, types::Value};

pub mod blob;
pub mod feeds;
pub mod items;
pub mod valine;

#[derive(Debug)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

pub fn execute_query(db: &str, sql: &str) -> Result<QueryResult, Box<dyn std::error::Error>> {
    let conn = Connection::open(db)?;
    let mut stmt = conn.prepare(sql)?;
    let columns: Vec<String> = stmt.column_names().into_iter().map(|s| s.to_string()).collect();
    let rows: Vec<Vec<String>> = stmt
        .query([])?
        .map(|row| {
            (0..columns.len())
                .map(|i| {
                    let value: Value = row.get(i)?;
                    Ok(match value {
                        Value::Integer(i) => i.to_string(),
                        Value::Real(f) => f.to_string(),
                        Value::Text(s) => s,
                        Value::Blob(_) => "[BLOB]".to_string(),
                        Value::Null => "NULL".to_string(),
                    })
                })
                .collect()
        })
        .collect()?;
    Ok(QueryResult { columns, rows })
}

pub fn transaction<T>(db: &str, callback: impl Fn(&Transaction) -> T) -> T {
    let mut conn = Connection::open(db).unwrap();
    let tx = conn.transaction().unwrap();
    let result = callback(&tx);
    if let Err(e) = tx.commit() {
        println!("!! error committing transaction: {e}");
    }
    result
}

pub fn migrations(_: &str) {
    // todo: handle database migrations later
}
