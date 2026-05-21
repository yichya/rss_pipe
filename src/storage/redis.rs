use rusqlite::{Connection, OptionalExtension};

pub struct RedisStorage {
    db_path: String,
}

impl RedisStorage {
    pub fn new(db_path: &str) -> Self {
        Self {
            db_path: db_path.to_string(),
        }
    }

    /// SET 命令实现
    pub fn set(&self, db: i64, key: &str, value: &str, expire_seconds: Option<i64>) -> Result<(), String> {
        let conn = Connection::open(&self.db_path).map_err(|e| format!("Database error: {e}"))?;

        conn.execute(
            "INSERT OR REPLACE INTO redis_storage (db, key, value, expire_at) VALUES (?1, ?2, ?3, CASE WHEN ?4 IS NOT NULL THEN unixepoch() + ?4 ELSE NULL END)",
            (db, key, value, expire_seconds),
        )
        .map_err(|e| format!("Database error: {e}"))?;

        Ok(())
    }

    /// GET 命令实现
    pub fn get(&self, db: i64, key: &str) -> Result<Option<String>, String> {
        let conn = Connection::open(&self.db_path).map_err(|e| format!("Database error: {e}"))?;

        let result: Option<String> = conn
            .query_row(
                "SELECT value FROM redis_storage WHERE db = ?1 AND key = ?2 AND (expire_at IS NULL OR expire_at > unixepoch())",
                (db, key),
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| format!("Database error: {e}"))?;

        Ok(result)
    }

    /// EXISTS 命令实现
    pub fn exists(&self, db: i64, key: &str) -> Result<bool, String> {
        let conn = Connection::open(&self.db_path).map_err(|e| format!("Database error: {e}"))?;

        let count = conn
            .query_row(
                "SELECT COUNT(*) FROM redis_storage WHERE db = ?1 AND key = ?2 AND (expire_at IS NULL OR expire_at > unixepoch())",
                (db, key),
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| format!("Database error: {e}"))?
            .unwrap_or(0);

        Ok(count > 0)
    }

    /// EXPIRE 命令实现
    pub fn expire(&self, db: i64, key: &str, seconds: i64) -> Result<bool, String> {
        let conn = Connection::open(&self.db_path).map_err(|e| format!("Database error: {e}"))?;

        let affected = conn
            .execute(
                "UPDATE redis_storage SET expire_at = unixepoch() + ?1 WHERE db = ?2 AND key = ?3 AND (expire_at IS NULL OR expire_at > unixepoch())",
                (seconds, db, key),
            )
            .map_err(|e| format!("Database error: {e}"))?;

        Ok(affected > 0)
    }

    /// 清理过期键
    pub fn cleanup_expired(&self) -> Result<u64, String> {
        let conn = Connection::open(&self.db_path).map_err(|e| format!("Database error: {e}"))?;

        let deleted = conn
            .execute(
                "DELETE FROM redis_storage WHERE expire_at IS NOT NULL AND expire_at <= unixepoch()",
                [],
            )
            .map_err(|e| format!("Database error: {e}"))?;

        Ok(deleted as u64)
    }
}
