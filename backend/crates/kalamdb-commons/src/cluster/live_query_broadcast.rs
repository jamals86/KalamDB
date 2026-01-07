use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::{TableId, UserId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveQueryBroadcast {
    pub cluster_id: String,
    pub user_id: UserId,
    pub table_id: TableId,
    pub change_type: String,
    pub row_data: Value,
    pub old_data: Option<Value>,
    pub row_id: Option<String>,
}
