//! Stream table storage operations.
//!
//! Provides storage for ephemeral stream tables with TTL-based eviction.
//! Key format: `{timestamp_ms}:{row_id}`.

use anyhow::{Context, Result};
use rocksdb::{IteratorMode, DB};
use serde_json::Value as JsonValue;
use std::sync::Arc;

use crate::key_encoding::{parse_stream_key, stream_key};

/// Stream table storage for ephemeral events.
///
/// # Key Format
///
/// Keys use the format: `{timestamp_ms}:{row_id}`
///
/// # No System Columns
///
/// Stream tables do NOT have `_updated` or `_deleted` columns.
/// Data is ephemeral and TTL-based.
pub struct StreamTableStore {
    db: Arc<DB>,
}

impl StreamTableStore {
    /// Create a new stream table store.
    pub fn new(db: Arc<DB>) -> Result<Self> {
        Ok(Self { db })
    }

    /// Insert an event with automatic timestamp.
    ///
    /// # Arguments
    ///
    /// * `namespace_id` - Namespace identifier
    /// * `table_name` - Table name
    /// * `timestamp_ms` - Event timestamp in milliseconds
    /// * `row_id` - Row identifier
    /// * `row_data` - Event data (NO system columns added)
    pub fn put(
        &self,
        namespace_id: &str,
        table_name: &str,
        timestamp_ms: i64,
        row_id: &str,
        row_data: JsonValue,
    ) -> Result<()> {
        let cf_name = format!("stream_table:{}:{}", namespace_id, table_name);
        let cf = self
            .db
            .cf_handle(&cf_name)
            .with_context(|| format!("Column family not found: {}", cf_name))?;

        let key = stream_key(timestamp_ms, row_id);
        let value = serde_json::to_vec(&row_data)?;

        self.db.put_cf(cf, key.as_bytes(), &value)?;
        Ok(())
    }

    /// Get an event by timestamp and row ID.
    pub fn get(
        &self,
        namespace_id: &str,
        table_name: &str,
        timestamp_ms: i64,
        row_id: &str,
    ) -> Result<Option<JsonValue>> {
        let cf_name = format!("stream_table:{}:{}", namespace_id, table_name);
        let cf = self
            .db
            .cf_handle(&cf_name)
            .with_context(|| format!("Column family not found: {}", cf_name))?;

        let key = stream_key(timestamp_ms, row_id);
        let value = self.db.get_cf(cf, key.as_bytes())?;

        match value {
            Some(bytes) => {
                let row_data: JsonValue = serde_json::from_slice(&bytes)?;
                Ok(Some(row_data))
            }
            None => Ok(None),
        }
    }

    /// Delete an event (always hard delete, no soft delete for streams).
    pub fn delete(
        &self,
        namespace_id: &str,
        table_name: &str,
        timestamp_ms: i64,
        row_id: &str,
    ) -> Result<()> {
        let cf_name = format!("stream_table:{}:{}", namespace_id, table_name);
        let cf = self
            .db
            .cf_handle(&cf_name)
            .with_context(|| format!("Column family not found: {}", cf_name))?;

        let key = stream_key(timestamp_ms, row_id);
        self.db.delete_cf(cf, key.as_bytes())?;
        Ok(())
    }

    /// Scan all events in the stream table.
    ///
    /// Returns a vector of `(timestamp_ms, row_id, row_data)` tuples.
    pub fn scan(
        &self,
        namespace_id: &str,
        table_name: &str,
    ) -> Result<Vec<(i64, String, JsonValue)>> {
        let cf_name = format!("stream_table:{}:{}", namespace_id, table_name);
        let cf = self
            .db
            .cf_handle(&cf_name)
            .with_context(|| format!("Column family not found: {}", cf_name))?;

        let mut results = Vec::new();
        let iter = self.db.iterator_cf(cf, IteratorMode::Start);

        for item in iter {
            let (key_bytes, value_bytes) = item?;
            let key = String::from_utf8(key_bytes.to_vec())?;
            let (timestamp_ms, row_id) = parse_stream_key(&key)?;
            let row_data: JsonValue = serde_json::from_slice(&value_bytes)?;

            results.push((timestamp_ms, row_id, row_data));
        }

        Ok(results)
    }

    /// Evict events older than a given timestamp (TTL cleanup).
    ///
    /// # Arguments
    ///
    /// * `cutoff_timestamp_ms` - Delete all events before this timestamp
    ///
    /// # Returns
    ///
    /// Number of events deleted
    pub fn evict_older_than(
        &self,
        namespace_id: &str,
        table_name: &str,
        cutoff_timestamp_ms: i64,
    ) -> Result<usize> {
        let cf_name = format!("stream_table:{}:{}", namespace_id, table_name);
        let cf = self
            .db
            .cf_handle(&cf_name)
            .with_context(|| format!("Column family not found: {}", cf_name))?;

        let mut deleted_count = 0;
        let iter = self.db.iterator_cf(cf, IteratorMode::Start);

        for item in iter {
            let (key_bytes, _) = item?;
            let key = String::from_utf8(key_bytes.to_vec())?;
            let (timestamp_ms, _) = parse_stream_key(&key)?;

            if timestamp_ms < cutoff_timestamp_ms {
                self.db.delete_cf(cf, key.as_bytes())?;
                deleted_count += 1;
            }
        }

        Ok(deleted_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocksdb::Options;
    use serde_json::json;
    use tempfile::TempDir;

    fn create_test_db() -> (Arc<DB>, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        let cf_names = vec!["stream_table:app:events"];
        let db = DB::open_cf(&opts, temp_dir.path(), &cf_names).unwrap();

        (Arc::new(db), temp_dir)
    }

    #[test]
    fn test_put_and_get() {
        let (db, _temp_dir) = create_test_db();
        let store = StreamTableStore::new(db).unwrap();

        let row_data = json!({
            "event_type": "click",
            "user_id": "user123"
        });

        let timestamp_ms = 1697299200000;
        store
            .put("app", "events", timestamp_ms, "evt001", row_data.clone())
            .unwrap();

        let retrieved = store.get("app", "events", timestamp_ms, "evt001").unwrap();
        assert!(retrieved.is_some());

        let retrieved_data = retrieved.unwrap();
        assert_eq!(retrieved_data["event_type"], "click");
        assert_eq!(retrieved_data["user_id"], "user123");

        // Stream tables should NOT have system columns
        assert!(retrieved_data.get("_updated").is_none());
        assert!(retrieved_data.get("_deleted").is_none());
    }

    #[test]
    fn test_delete() {
        let (db, _temp_dir) = create_test_db();
        let store = StreamTableStore::new(db).unwrap();

        let timestamp_ms = 1697299200000;
        store
            .put(
                "app",
                "events",
                timestamp_ms,
                "evt001",
                json!({"event_type": "click"}),
            )
            .unwrap();

        // Delete
        store
            .delete("app", "events", timestamp_ms, "evt001")
            .unwrap();

        // Should return None because event is deleted
        let retrieved = store.get("app", "events", timestamp_ms, "evt001").unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_scan() {
        let (db, _temp_dir) = create_test_db();
        let store = StreamTableStore::new(db).unwrap();

        store
            .put(
                "app",
                "events",
                1697299200000,
                "evt001",
                json!({"event_type": "click"}),
            )
            .unwrap();
        store
            .put(
                "app",
                "events",
                1697299201000,
                "evt002",
                json!({"event_type": "scroll"}),
            )
            .unwrap();

        let results = store.scan("app", "events").unwrap();
        assert_eq!(results.len(), 2);

        assert_eq!(results[0].0, 1697299200000);
        assert_eq!(results[0].1, "evt001");
        assert_eq!(results[1].0, 1697299201000);
        assert_eq!(results[1].1, "evt002");
    }

    #[test]
    fn test_evict_older_than() {
        let (db, _temp_dir) = create_test_db();
        let store = StreamTableStore::new(db).unwrap();

        // Insert events at different timestamps
        store
            .put(
                "app",
                "events",
                1697299200000,
                "evt001",
                json!({"event_type": "old"}),
            )
            .unwrap();
        store
            .put(
                "app",
                "events",
                1697299201000,
                "evt002",
                json!({"event_type": "old"}),
            )
            .unwrap();
        store
            .put(
                "app",
                "events",
                1697299300000,
                "evt003",
                json!({"event_type": "new"}),
            )
            .unwrap();

        // Evict events older than 1697299250000
        let deleted_count = store
            .evict_older_than("app", "events", 1697299250000)
            .unwrap();
        assert_eq!(deleted_count, 2);

        // Only new event should remain
        let results = store.scan("app", "events").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, "evt003");
    }
}
