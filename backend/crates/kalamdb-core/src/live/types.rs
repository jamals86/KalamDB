use super::initial_data::InitialDataResult;
use datafusion::scalar::ScalarValue;
use kalamdb_commons::models::{LiveQueryId, Row};
use std::collections::BTreeMap;

/// Change notification for live query subscribers
#[derive(Debug, Clone)]
pub struct ChangeNotification {
    pub change_type: ChangeType,
    pub table_id: kalamdb_commons::models::TableId,
    pub row_data: Row,
    pub old_data: Option<Row>,  // For UPDATE notifications
    pub row_id: Option<String>, // For DELETE notifications (hard delete)
}

impl ChangeNotification {
    /// Create an INSERT notification
    pub fn insert(table_id: kalamdb_commons::models::TableId, row_data: Row) -> Self {
        Self {
            change_type: ChangeType::Insert,
            table_id,
            row_data,
            old_data: None,
            row_id: None,
        }
    }

    /// Create an UPDATE notification with old and new values
    pub fn update(
        table_id: kalamdb_commons::models::TableId,
        old_data: Row,
        new_data: Row,
    ) -> Self {
        Self {
            change_type: ChangeType::Update,
            table_id,
            row_data: new_data,
            old_data: Some(old_data),
            row_id: None,
        }
    }

    /// Create a DELETE notification (soft delete with data)
    pub fn delete_soft(table_id: kalamdb_commons::models::TableId, row_data: Row) -> Self {
        Self {
            change_type: ChangeType::Delete,
            table_id,
            row_data,
            old_data: None,
            row_id: None,
        }
    }

    /// Create a DELETE notification (hard delete, row_id only)
    pub fn delete_hard(table_id: kalamdb_commons::models::TableId, row_id: String) -> Self {
        // For hard delete, we create a dummy row with just ID for notification purposes
        // or we might need to change how delete notifications work.
        // For now, let's create a minimal Row with just ID if possible, or empty.
        let mut values = BTreeMap::new();
        values.insert(
            "row_id".to_string(),
            ScalarValue::Utf8(Some(row_id.clone())),
        );

        Self {
            change_type: ChangeType::Delete,
            table_id,
            row_data: Row::new(values),
            old_data: None,
            row_id: Some(row_id),
        }
    }

    /// Create a FLUSH notification (Parquet flush completion)
    pub fn flush(
        table_id: kalamdb_commons::models::TableId,
        row_count: usize,
        parquet_files: Vec<String>,
    ) -> Self {
        let mut values = BTreeMap::new();
        values.insert(
            "row_count".to_string(),
            ScalarValue::Int64(Some(row_count as i64)),
        );
        let files_json = serde_json::to_string(&parquet_files).unwrap_or_default();
        values.insert(
            "parquet_files".to_string(),
            ScalarValue::Utf8(Some(files_json)),
        );
        values.insert(
            "flushed_at".to_string(),
            ScalarValue::Int64(Some(chrono::Utc::now().timestamp_millis())),
        );

        Self {
            change_type: ChangeType::Flush,
            table_id,
            row_data: Row::new(values),
            old_data: None,
            row_id: None,
        }
    }
}

/// Result of registering a live query subscription with initial data
#[derive(Debug, Clone)]
pub struct SubscriptionResult {
    /// The generated LiveId for the subscription
    pub live_id: LiveQueryId,

    /// Initial data returned with the subscription (if requested)
    pub initial_data: Option<InitialDataResult>,
}

/// Type of change that occurred
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeType {
    Insert,
    Update,
    Delete,
    Flush, // Parquet flush completion
}

/// Registry statistics
#[derive(Debug, Clone)]
pub struct RegistryStats {
    pub total_connections: usize,
    pub total_subscriptions: usize,
    pub node_id: String,
}
