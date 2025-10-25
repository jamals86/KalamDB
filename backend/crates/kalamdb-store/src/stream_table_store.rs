//! Stream table storage operations.
//!
//! Provides **in-memory** storage for ephemeral stream tables with TTL-based eviction.
//! Key format: `{timestamp_ms}:{row_id}`.
//!
//! **IMPORTANT**: Stream tables are MEMORY-ONLY and do NOT persist to RocksDB.
//! All data is lost on server restart (expected behavior for ephemeral data).

use anyhow::Result;
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
        self.tables.entry(table_key.clone()).or_insert_with(BTreeMap::new);
        
        log::info!("✨ Created in-memory stream table: {}.{} (ephemeral, data lost on restart)", namespace_id, table_name);
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

    /// Scan all events in the stream table (from in-memory storage).
    ///
    /// Returns a vector of `(timestamp_ms, row_id, row_data)` tuples.
    pub fn scan(
        &self,
        namespace_id: &str,
        table_name: &str,
    ) -> Result<Vec<(i64, String, JsonValue)>> {
        let table_key = format!("{}:{}", namespace_id, table_name);

        if let Some(table_ref) = self.tables.get(&table_key) {
            let mut results = Vec::new();
            
            for (event_key, row_data) in table_ref.iter() {
                let (timestamp_ms, row_id) = parse_stream_key(event_key)?;
                results.push((timestamp_ms, row_id, row_data.clone()));
            }
            
            Ok(results)
        } else {
            Ok(Vec::new())
        }
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
        let table_key = format!("{}:{}", namespace_id, table_name);

        if let Some(mut table_ref) = self.tables.get_mut(&table_key) {
            let mut deleted_count = 0;
            
            // Collect keys to delete (can't modify while iterating)
            let keys_to_delete: Vec<String> = table_ref
                .iter()
                .filter_map(|(key, _)| {
                    if let Ok((timestamp_ms, _)) = parse_stream_key(key) {
                        if timestamp_ms < cutoff_timestamp_ms {
                            return Some(key.clone());
                        }
                    }
                    None
                })
                .collect();
            
            // Delete the keys
            for key in keys_to_delete {
                table_ref.remove(&key);
                deleted_count += 1;
            }
            
            Ok(deleted_count)
        } else {
            Ok(0)
        }
    }

    /// Drop entire table by removing it from in-memory storage.
    ///
    /// # Arguments
    ///
    /// * `namespace_id` - The namespace identifier
    /// * `table_name` - The table name
    pub fn drop_table(&self, namespace_id: &str, table_name: &str) -> Result<()> {
        let table_key = format!("{}:{}", namespace_id, table_name);
        self.tables.remove(&table_key);
        log::info!("🗑️  Dropped in-memory stream table: {}.{}", namespace_id, table_name);
        Ok(())
    }

    /// Count total events in a stream table.
    pub fn count(&self, namespace_id: &str, table_name: &str) -> Result<usize> {
        let table_key = format!("{}:{}", namespace_id, table_name);
        
        if let Some(table_ref) = self.tables.get(&table_key) {
            Ok(table_ref.len())
        } else {
            Ok(0)
        }
    }

    /// Get oldest event timestamp in table (for TTL eviction).
    pub fn get_oldest_timestamp(&self, namespace_id: &str, table_name: &str) -> Result<Option<i64>> {
        let table_key = format!("{}:{}", namespace_id, table_name);
        
        if let Some(table_ref) = self.tables.get(&table_key) {
            // BTreeMap is sorted by key, so first() gives us the oldest timestamp
            if let Some((first_key, _)) = table_ref.iter().next() {
                let (timestamp_ms, _) = parse_stream_key(first_key)?;
                return Ok(Some(timestamp_ms));
            }
        }
        
        Ok(None)
    }

    /// Get newest event timestamp in table.
    pub fn get_newest_timestamp(&self, namespace_id: &str, table_name: &str) -> Result<Option<i64>> {
        let table_key = format!("{}:{}", namespace_id, table_name);
        
        if let Some(table_ref) = self.tables.get(&table_key) {
            // BTreeMap is sorted by key, so last() gives us the newest timestamp
            if let Some((last_key, _)) = table_ref.iter().next_back() {
                let (timestamp_ms, _) = parse_stream_key(last_key)?;
                return Ok(Some(timestamp_ms));
            }
        }
        
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    fn create_test_store() -> StreamTableStore {
        let dir = tempdir().unwrap();
        let db = Arc::new(rocksdb::DB::open_default(dir.path()).unwrap());
        StreamTableStore::new(db).unwrap()
    }

    #[test]
    fn test_create_and_put() {
        let store = create_test_store();
        store.create_column_family("test", "events").unwrap();

        let data = json!({"event": "click", "user": "alice"});
        store.put("test", "events", 1000, "evt1", data.clone()).unwrap();

        let result = store.get("test", "events", 1000, "evt1").unwrap();
        assert_eq!(result, Some(data));
    }

    #[test]
    fn test_scan() {
        let store = create_test_store();
        store.create_column_family("test", "events").unwrap();

        store.put("test", "events", 1000, "evt1", json!({"a": 1})).unwrap();
        store.put("test", "events", 2000, "evt2", json!({"a": 2})).unwrap();
        store.put("test", "events", 3000, "evt3", json!({"a": 3})).unwrap();

        let results = store.scan("test", "events").unwrap();
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_evict_older_than() {
        let store = create_test_store();
        store.create_column_family("test", "events").unwrap();

        store.put("test", "events", 1000, "evt1", json!({"a": 1})).unwrap();
        store.put("test", "events", 2000, "evt2", json!({"a": 2})).unwrap();
        store.put("test", "events", 3000, "evt3", json!({"a": 3})).unwrap();

        let deleted = store.evict_older_than("test", "events", 2500).unwrap();
        assert_eq!(deleted, 2); // evt1 and evt2

        let remaining = store.scan("test", "events").unwrap();
        assert_eq!(remaining.len(), 1);
    }

    #[test]
    fn test_drop_table() {
        let store = create_test_store();
        store.create_column_family("test", "events").unwrap();

        store.put("test", "events", 1000, "evt1", json!({"a": 1})).unwrap();
        assert_eq!(store.count("test", "events").unwrap(), 1);

        store.drop_table("test", "events").unwrap();
        assert_eq!(store.count("test", "events").unwrap(), 0);
    }
}
