//! Version resolution function for MVCC queries
//!
//! This module implements resolve_latest_version(), which applies MAX(_seq) grouping
//! per primary key and filters out deleted versions.

use crate::error::KalamDbError;
use datafusion::arrow::array::RecordBatch;
use std::collections::HashMap;

/// Resolve latest version from a RecordBatch containing multiple versions
///
/// **MVCC Architecture**: This function is used by query planning to deduplicate versions.
/// Given a RecordBatch with multiple versions of the same rows, it:
/// 1. Groups rows by primary key (extracted from fields column)
/// 2. Selects MAX(_seq) version for each PK
/// 3. Filters out rows where _deleted = true
///
/// # Arguments
/// * `batch` - RecordBatch with columns: _seq, _deleted, fields (JSON), and optionally user_id
/// * `pk_column` - Name of the primary key column within the fields JSON
///
/// # Returns
/// * `Ok(RecordBatch)` - Deduplicated batch with only latest non-deleted versions
/// * `Err(KalamDbError)` - If resolution fails
///
/// **Note**: This is a placeholder implementation. Full implementation requires:
/// - JSON path extraction from fields column
/// - Arrow compute functions for grouping and filtering
/// - Proper type handling for different PK types
pub fn resolve_latest_version(
    batch: RecordBatch,
    pk_column: &str,
) -> Result<RecordBatch, KalamDbError> {
    // TODO: Implement actual version resolution logic
    // For now, return the batch as-is (placeholder)

    // Proper implementation steps:
    // 1. Extract _seq column as i64 array
    // 2. Extract _deleted column as bool array
    // 3. Extract fields column as JSON/String array
    // 4. Parse PK values from fields JSON using pk_column name
    // 5. Group by PK, find MAX(_seq) indices
    // 6. Filter by _deleted = false
    // 7. Build new RecordBatch with filtered rows

    let _ = pk_column; // Silence unused warning for now

    Ok(batch)
}

/// Resolve latest version from in-memory rows (for hot storage)
///
/// This is a simpler version used for resolving versions from RocksDB scans
/// before converting to Arrow format.
///
/// # Arguments
/// * `rows` - Vec of (pk_value, seq_id, deleted, fields_json) tuples
///
/// # Returns
/// * HashMap<pk_value, (seq_id, fields_json)> with only latest non-deleted versions
#[cfg_attr(not(test), allow(dead_code))]
pub fn resolve_latest_version_from_rows(
    rows: Vec<(String, i64, bool, serde_json::Value)>,
) -> HashMap<String, (i64, serde_json::Value)> {
    let mut latest: HashMap<String, (i64, bool, serde_json::Value)> = HashMap::new();

    // Group by PK and find MAX(seq)
    for (pk, seq, deleted, fields) in rows {
        latest
            .entry(pk)
            .and_modify(|existing| {
                if seq > existing.0 {
                    *existing = (seq, deleted, fields.clone());
                }
            })
            .or_insert((seq, deleted, fields));
    }

    // Filter out deleted rows and return (seq, fields) only
    latest
        .into_iter()
        .filter_map(|(pk, (seq, deleted, fields))| {
            if !deleted {
                Some((pk, (seq, fields)))
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_latest_version_from_rows() {
        let rows = vec![
            (
                "pk1".to_string(),
                100,
                false,
                serde_json::json!({"id": 1, "name": "Alice"}),
            ),
            (
                "pk1".to_string(),
                200,
                false,
                serde_json::json!({"id": 1, "name": "Alice Updated"}),
            ),
            (
                "pk2".to_string(),
                150,
                false,
                serde_json::json!({"id": 2, "name": "Bob"}),
            ),
            (
                "pk2".to_string(),
                250,
                true,
                serde_json::json!({"id": 2, "name": "Bob Deleted"}),
            ),
        ];

        let resolved = resolve_latest_version_from_rows(rows);

        // pk1 should have seq=200 (latest)
        assert_eq!(resolved.len(), 1); // Only pk1 (pk2 deleted)
        assert!(resolved.contains_key("pk1"));
        assert_eq!(resolved["pk1"].0, 200);

        let fields = &resolved["pk1"].1;
        assert_eq!(fields["name"], "Alice Updated");
    }

    #[test]
    fn test_resolve_latest_version_filters_deleted() {
        let rows = vec![
            ("pk1".to_string(), 100, false, serde_json::json!({"id": 1})),
            ("pk1".to_string(), 200, true, serde_json::json!({"id": 1})), // Deleted
        ];

        let resolved = resolve_latest_version_from_rows(rows);

        // pk1 deleted at seq=200, should not appear
        assert_eq!(resolved.len(), 0);
    }

    #[test]
    fn test_resolve_latest_version_multiple_keys() {
        let rows = vec![
            ("pk1".to_string(), 100, false, serde_json::json!({"id": 1})),
            ("pk2".to_string(), 150, false, serde_json::json!({"id": 2})),
            ("pk3".to_string(), 120, false, serde_json::json!({"id": 3})),
            (
                "pk1".to_string(),
                300,
                false,
                serde_json::json!({"id": 1, "updated": true}),
            ),
        ];

        let resolved = resolve_latest_version_from_rows(rows);

        assert_eq!(resolved.len(), 3);
        assert_eq!(resolved["pk1"].0, 300); // pk1 updated to seq=300
        assert_eq!(resolved["pk2"].0, 150);
        assert_eq!(resolved["pk3"].0, 120);
    }
}
