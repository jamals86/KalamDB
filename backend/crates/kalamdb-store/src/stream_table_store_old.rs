//! Stream table storage operations.
//!
//! Provides **in-memory** storage for ephemeral stream tables with TTL-based eviction.
//! Key format: `{timestamp_ms}:{row_id}`.
//!
//! **IMPORTANT**: Stream tables are MEMORY-ONLY and do NOT persist to RocksDB.
//! All data is lost on server restart (expected behavior for ephemeral data).

use anyhow::{Context, Result};
use dashmap::DashMap;
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::key_encoding::{parse_stream_key, stream_key};

/// Stream table storage for ephemeral events (IN-MEMORY ONLY).
///
/// # Key Format
///
/// Keys use the format: `{timestamp_ms}:{row_id}`
///
/// # No System Columns
///
/// Stream tables do NOT have `_updated` or `_deleted` columns.
/// Data is ephemeral and TTL-based.
///
/// # Storage
///
/// All data is stored in memory using DashMap<table_key, BTreeMap<event_key, data>>.
/// Data is LOST on server restart (ephemeral by design).
pub struct StreamTableStore {
    // In-memory storage: table_key -> (event_key -> event_data)
    // table_key format: "{namespace}:{table_name}"
    // event_key format: "{timestamp_ms}:{row_id}"
    tables: Arc<DashMap<String, BTreeMap<String, JsonValue>>>,
}

impl StreamTableStore {
    /// Create a new in-memory stream table store.
    pub fn new(_db: Arc<rocksdb::DB>) -> Result<Self> {
        // NOTE: We accept db parameter for API compatibility but don't use it.
        // Stream tables are memory-only and never write to RocksDB.
        Ok(Self {
            tables: Arc::new(DashMap::new()),
        })
    }

    /// Create a column family for a stream table.
    ///
    /// **NO-OP**: Stream tables are memory-only and don't use RocksDB column families.
    /// This method exists for API compatibility only.
    ///
    /// # Arguments
    ///
    /// * `namespace_id` - Namespace identifier
    /// * `table_name` - Table name
    pub fn create_column_family(&self, namespace_id: &str, table_name: &str) -> Result<()> {
        let table_key = format!("{}:{}", namespace_id, table_name);
        
        // Initialize empty BTreeMap for this table (in-memory only)
        self.tables.entry(table_key).or_insert_with(BTreeMap::new);
        
        log::debug!("Created in-memory stream table: {}.{}", namespace_id, table_name);
        Ok(())
    }

    /// Insert an event with automatic timestamp (IN-MEMORY ONLY).
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
        let table_key = format!("{}:{}", namespace_id, table_name);
        let event_key = stream_key(timestamp_ms, row_id);

        // Get or create the table's event map
        let mut table_ref = self.tables.entry(table_key.clone()).or_insert_with(BTreeMap::new);
        
        // Insert event into in-memory storage
        table_ref.insert(event_key, row_data);
        
        log::trace!("Inserted event into in-memory stream table: {} (key: {})", table_key, stream_key(timestamp_ms, row_id));
        Ok(())
    }

    /// Get an event by timestamp and row ID (from in-memory storage).
    pub fn get(
        &self,
        namespace_id: &str,
        table_name: &str,
        timestamp_ms: i64,
        row_id: &str,
    ) -> Result<Option<JsonValue>> {
        let table_key = format!("{}:{}", namespace_id, table_name);
        let event_key = stream_key(timestamp_ms, row_id);

        if let Some(table_ref) = self.tables.get(&table_key) {
            Ok(table_ref.get(&event_key).cloned())
        } else {
            Ok(None)
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
        let table_key = format!("{}:{}", namespace_id, table_name);
        let event_key = stream_key(timestamp_ms, row_id);

        if let Some(mut table_ref) = self.tables.get_mut(&table_key) {
            table_ref.remove(&event_key);
        }
        Ok(())
    }
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
        let cf_name = format!(
            "{}{}:{}",
            ColumnFamilyNames::STREAM_TABLE_PREFIX,
            namespace_id,
            table_name
        );
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
        let cf_name = format!(
            "{}{}:{}",
            ColumnFamilyNames::STREAM_TABLE_PREFIX,
            namespace_id,
            table_name
        );
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

    /// Drop entire table by deleting its column family
    ///
    /// # Arguments
    ///
    /// * `namespace_id` - The namespace identifier
    /// * `table_name` - The table name
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the column family was successfully dropped,
    /// or an error if the operation fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use kalamdb_store::StreamTableStore;
    /// # use std::sync::Arc;
    /// # use rocksdb::DB;
    /// # let db = Arc::new(DB::open_default("test").unwrap());
    /// let store = StreamTableStore::new(db).unwrap();
    /// store.drop_table("app", "events").unwrap();
    /// ```
    pub fn drop_table(&self, namespace_id: &str, table_name: &str) -> Result<()> {
        let cf_name = format!(
            "{}{}:{}",
            ColumnFamilyNames::STREAM_TABLE_PREFIX,
            namespace_id,
            table_name
        );

        let cf = self
            .db
            .cf_handle(&cf_name)
            .with_context(|| format!("Column family not found: {}", cf_name))?;

        // Delete all keys by iterating and batching deletes
        let mut batch = rocksdb::WriteBatch::default();
        let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);

        for item in iter {
            let (key, _) = item?;
            batch.delete_cf(cf, key);
        }

        self.db.write(batch)?;
        Ok(())
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

        // Use correct column family format: stream_{namespace}:{table}
        let cf_names = vec!["stream_app:events"];
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

    #[test]
    fn test_drop_table_deletes_all_stream_data() {
        let (db, _temp_dir) = create_test_db();
        let store = StreamTableStore::new(db).unwrap();

        let ts1 = 1697299200000; // Timestamp 1
        let ts2 = 1697299210000; // Timestamp 2
        let ts3 = 1697299220000; // Timestamp 3

        // Insert multiple events
        store
            .put("app", "events", ts1, "evt001", json!({"type": "click"}))
            .unwrap();
        store
            .put("app", "events", ts2, "evt002", json!({"type": "view"}))
            .unwrap();
        store
            .put("app", "events", ts3, "evt003", json!({"type": "submit"}))
            .unwrap();

        // Verify data exists
        assert!(store.get("app", "events", ts1, "evt001").unwrap().is_some());
        assert!(store.get("app", "events", ts2, "evt002").unwrap().is_some());
        assert!(store.get("app", "events", ts3, "evt003").unwrap().is_some());

        // Drop the entire table
        store.drop_table("app", "events").unwrap();

        // Verify all data is deleted
        assert!(store.get("app", "events", ts1, "evt001").unwrap().is_none());
        assert!(store.get("app", "events", ts2, "evt002").unwrap().is_none());
        assert!(store.get("app", "events", ts3, "evt003").unwrap().is_none());
    }
}
