//! Manifest cache models for query optimization (Phase 4 - US6).
//!
//! This module defines ManifestFile and ManifestCacheEntry structures for
//! tracking Parquet batch file metadata and caching manifests in RocksDB.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::models::TableId;
use crate::UserId;

/// Synchronization state of a cached manifest entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncState {
    /// Cache is in sync with storage
    InSync,
    /// Cache may be stale and needs refresh
    Stale,
    /// Error occurred during last sync attempt
    Error,
}

impl Default for SyncState {
    fn default() -> Self {
        Self::InSync
    }
}

impl std::fmt::Display for SyncState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncState::InSync => write!(f, "in_sync"),
            SyncState::Stale => write!(f, "stale"),
            SyncState::Error => write!(f, "error"),
        }
    }
}

/// Manifest cache entry stored in RocksDB (Phase 4 - US6).
///
/// Fields:
/// - `manifest_json`: Serialized ManifestFile content
/// - `etag`: Storage ETag or version identifier for freshness validation
/// - `last_refreshed`: Unix timestamp (seconds) of last successful refresh
/// - `source_path`: Full path to manifest.json in storage backend
/// - `sync_state`: Current synchronization state (InSync | Stale | Error)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestCacheEntry {
    /// Serialized ManifestFile JSON
    pub manifest_json: String, //TODO: Maybe its better to have it as parsed one instead of string

    /// ETag or version identifier from storage backend
    pub etag: Option<String>,

    /// Last refresh timestamp (Unix seconds)
    pub last_refreshed: i64,

    /// Source path in storage (e.g., "s3://bucket/namespace/table/manifest.json")
    pub source_path: String,

    /// Synchronization state
    pub sync_state: SyncState,
}

impl ManifestCacheEntry {
    /// Create a new cache entry
    pub fn new(
        manifest_json: String,
        etag: Option<String>,
        last_refreshed: i64,
        source_path: String,
        sync_state: SyncState,
    ) -> Self {
        Self {
            manifest_json,
            etag,
            last_refreshed,
            source_path,
            sync_state,
        }
    }

    /// Check if entry is stale based on TTL
    pub fn is_stale(&self, ttl_seconds: i64, now_timestamp: i64) -> bool {
        now_timestamp - self.last_refreshed > ttl_seconds
    }

    /// Mark entry as stale
    pub fn mark_stale(&mut self) {
        self.sync_state = SyncState::Stale;
    }

    /// Mark entry as in sync
    pub fn mark_in_sync(&mut self, etag: Option<String>, timestamp: i64) {
        self.sync_state = SyncState::InSync;
        self.etag = etag;
        self.last_refreshed = timestamp;
    }

    /// Mark entry as error
    pub fn mark_error(&mut self) {
        self.sync_state = SyncState::Error;
    }
}

/// Statistics for a single column in a segment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ColumnStats {
    /// Minimum value in the column (serialized as JSON value)
    pub min: Option<serde_json::Value>,

    /// Maximum value in the column (serialized as JSON value)
    pub max: Option<serde_json::Value>,

    /// Number of null values
    pub null_count: Option<i64>,
}

/// Segment metadata tracking a data file (Parquet) or hot storage segment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentMetadata {
    /// Unique segment identifier (UUID)
    pub id: String,

    /// Path to the segment file (relative to table root)
    pub path: String,

    /// Column statistics (min/max/nulls) keyed by column name
    /// Replaces legacy min_key/max_key
    #[serde(default)]
    pub column_stats: HashMap<String, ColumnStats>,

    /// Minimum sequence number in this segment (for MVCC pruning)
    pub min_seq: i64,

    /// Maximum sequence number in this segment (for MVCC pruning)
    pub max_seq: i64,

    /// Number of rows in this segment
    pub row_count: u64,

    /// Size in bytes
    pub size_bytes: u64,

    /// Creation timestamp (Unix seconds)
    pub created_at: i64,

    /// If true, this segment is marked for deletion (compaction/cleanup)
    pub tombstone: bool,
}

impl SegmentMetadata {
    pub fn new(
        id: String,
        path: String,
        column_stats: HashMap<String, ColumnStats>,
        min_seq: i64,
        max_seq: i64,
        row_count: u64,
        size_bytes: u64,
    ) -> Self {
        Self {
            id,
            path,
            column_stats,
            min_seq,
            max_seq,
            row_count,
            size_bytes,
            created_at: chrono::Utc::now().timestamp(),
            tombstone: false,
        }
    }
}

/// Manifest file tracking segments for a table.
///
/// The manifest is the source of truth for data location (Hot vs Cold).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// Table identifier
    pub table_id: TableId,

    /// Optional user_id for User tables
    pub user_id: Option<UserId>,

    /// Manifest version (incremented on update)
    pub version: u64,

    /// Creation timestamp
    pub created_at: i64,

    /// Last update timestamp
    pub updated_at: i64,

    /// List of data segments
    pub segments: Vec<SegmentMetadata>,

    /// Last assigned sequence number (for append-only sequencing)
    pub last_sequence_number: u64,
}

impl Manifest {
    pub fn new(table_id: TableId, user_id: Option<UserId>) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            table_id,
            user_id,
            version: 1,
            created_at: now,
            updated_at: now,
            segments: Vec::new(),
            last_sequence_number: 0,
        }
    }
    // ...existing code...

    pub fn add_segment(&mut self, segment: SegmentMetadata) {
        self.segments.push(segment);
        self.updated_at = chrono::Utc::now().timestamp();
        self.version += 1;
    }

    pub fn update_sequence_number(&mut self, seq: u64) {
        if seq > self.last_sequence_number {
            self.last_sequence_number = seq;
            self.updated_at = chrono::Utc::now().timestamp();
            // Version bump? Maybe not for just seq update if it happens often in memory
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NamespaceId, TableName};

    #[test]
    fn test_manifest_cache_entry_is_stale() {
        let entry = ManifestCacheEntry::new(
            "{}".to_string(),
            Some("etag123".to_string()),
            1000,
            "path/to/manifest.json".to_string(),
            SyncState::InSync,
        );

        // Not stale within TTL
        assert!(!entry.is_stale(3600, 1000 + 1800));

        // Stale after TTL
        assert!(entry.is_stale(3600, 1000 + 3601));
    }

    #[test]
    fn test_manifest_cache_entry_state_transitions() {
        let mut entry = ManifestCacheEntry::new(
            "{}".to_string(),
            None,
            1000,
            "path".to_string(),
            SyncState::InSync,
        );

        entry.mark_stale();
        assert_eq!(entry.sync_state, SyncState::Stale);

        entry.mark_in_sync(Some("new_etag".to_string()), 2000);
        assert_eq!(entry.sync_state, SyncState::InSync);
        assert_eq!(entry.etag, Some("new_etag".to_string()));
        assert_eq!(entry.last_refreshed, 2000);

        entry.mark_error();
        assert_eq!(entry.sync_state, SyncState::Error);
    }

    #[test]
    fn test_manifest_add_segment() {
        let table_id = TableId::new(NamespaceId::new("test"), TableName::new("table"));
        let mut manifest = Manifest::new(table_id, None);

        assert_eq!(manifest.segments.len(), 0);

        let segment = SegmentMetadata::new(
            "uuid-1".to_string(),
            "segment-1.parquet".to_string(),
            HashMap::new(),
            1000,
            2000,
            50,
            512,
        );

        manifest.add_segment(segment);
        assert_eq!(manifest.segments.len(), 1);
        assert_eq!(manifest.version, 2);
    }

    #[test]
    fn test_manifest_update_sequence_number() {
        let table_id = TableId::new(NamespaceId::new("test"), TableName::new("table"));
        let mut manifest = Manifest::new(table_id, None);

        assert_eq!(manifest.last_sequence_number, 0);

        manifest.update_sequence_number(100);
        assert_eq!(manifest.last_sequence_number, 100);

        // Should not decrease
        manifest.update_sequence_number(50);
        assert_eq!(manifest.last_sequence_number, 100);
    }
}
