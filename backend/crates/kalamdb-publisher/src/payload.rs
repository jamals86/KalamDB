//! Payload extraction for topic messages.
//!
//! Converts Row data to serialized byte payloads based on the route's PayloadMode.

use kalamdb_commons::{
    conversions::arrow_json_conversion::row_to_json_map,
    errors::{CommonError, Result},
    models::{rows::Row, PayloadMode, TableId},
};
use kalamdb_system::providers::topics::TopicRoute;

/// Extract payload bytes from a Row based on the route's PayloadMode.
///
/// For Full and Diff modes, injects `_table` metadata so consumers can identify
/// the source table of each message.
pub(crate) fn extract_payload(
    route: &TopicRoute,
    row: &Row,
    table_id: &TableId,
) -> Result<Vec<u8>> {
    match route.payload_mode {
        PayloadMode::Key => extract_key_columns(row),
        PayloadMode::Full | PayloadMode::Diff => extract_full_row_with_metadata(row, table_id),
    }
}

/// Extract a message key from a Row (for keyed/partitioned topics).
pub(crate) fn extract_key(row: &Row) -> Result<Option<String>> {
    // TODO: Add partition_key_expr support
    if row.values.is_empty() {
        return Ok(None);
    }

    let json_map = row_to_json_map(row)
        .map_err(|e| CommonError::Internal(format!("Failed to convert row to JSON: {}", e)))?;

    let json_str = serde_json::to_string(&json_map)
        .map_err(|e| CommonError::Internal(format!("Failed to serialize key: {}", e)))?;

    Ok(Some(json_str))
}

/// Hash a row for consistent partition selection.
pub(crate) fn hash_row(row: &Row) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();

    // Hash the JSON representation for consistency
    if let Ok(json_map) = row_to_json_map(row) {
        if let Ok(json_str) = serde_json::to_string(&json_map) {
            json_str.hash(&mut hasher);
            return hasher.finish();
        }
    }

    // Fallback: hash column names only
    for key in row.values.keys() {
        key.hash(&mut hasher);
    }

    hasher.finish()
}

// ---- Internal helpers ----

fn extract_key_columns(row: &Row) -> Result<Vec<u8>> {
    if row.values.is_empty() {
        return Err(CommonError::InvalidInput("Cannot extract key from empty row".to_string()));
    }

    let json_map = row_to_json_map(row)
        .map_err(|e| CommonError::Internal(format!("Failed to convert row to JSON: {}", e)))?;

    serde_json::to_vec(&json_map)
        .map_err(|e| CommonError::Internal(format!("Failed to serialize keys: {}", e)))
}

/// Extract full row payload with `_table` metadata injected.
///
/// This allows consumers to identify which source table produced each message
/// without needing to track route configurations.
fn extract_full_row_with_metadata(row: &Row, table_id: &TableId) -> Result<Vec<u8>> {
    let mut json_map = row_to_json_map(row)
        .map_err(|e| CommonError::Internal(format!("Failed to convert row to JSON: {}", e)))?;

    // Inject source table metadata (uses system column convention with underscore prefix)
    json_map.insert("_table".to_string(), serde_json::Value::String(table_id.to_string()));

    serde_json::to_vec(&json_map)
        .map_err(|e| CommonError::Internal(format!("Failed to serialize row: {}", e)))
}
