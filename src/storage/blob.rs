use serde::Serialize;

#[derive(Serialize, Debug)]
pub struct BlobStorage {
    pub id: i64,
    pub item_id: i64,
    pub reply_id: Option<i64>,
    pub data: String,
    pub metadata: String,
    pub create_time: String,
}
