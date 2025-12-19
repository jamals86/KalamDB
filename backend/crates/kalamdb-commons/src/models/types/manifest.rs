//! Manifest cache models for query optimization (Phase 4 - US6).
//!
//! This module defines ManifestFile and ManifestCacheEntry structures for
//! tracking Parquet batch file metadata and caching manifests in RocksDB.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::models::TableId;
use crate::UserId;

/// Synchronization state of a cached manifest entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncState {
    /// Cache is in sync with storage (manifest.json on disk matches cache)
    #[default]
    InSync,
    /// Cache has local changes that need to be written to storage (pending flush)
    PendingWrite,
    /// Flush in progress: Parquet being written to temp location (atomic write pattern)
    /// If server crashes in this state, temp files can be cleaned up on restart.
    Syncing,
    /// Cache may be stale and needs refresh from storage
    Stale,
    /// Error occurred during last sync attempt
    Error,
}

impl std::fmt::Display for SyncState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncState::InSync => write!(f, "in_sync"),
            SyncState::PendingWrite => write!(f, "pending_write"),
            SyncState::Syncing => write!(f, "syncing"),
            SyncState::Stale => write!(f, "stale"),
            SyncState::Error => write!(f, "error"),
        }
    }
}

/// Manifest cache entry stored in RocksDB (Phase 4 - US6).
///
/// Fields:
/// - `manifest`: The parsed Manifest object (stored directly, serialized on-the-fly for display)
/// - `etag`: Storage ETag or version identifier for freshness validation
/// - `last_refreshed`: Unix timestamp (seconds) of last successful refresh
/// - `source_path`: Full path to manifest.json in storage backend
/// - `sync_state`: Current synchronization state (InSync | Stale | Error)
///
/// Note: Uses custom serialization because bincode doesn't support serde_json::Value in ColumnStats.
/// The Manifest is serialized as JSON string for storage, but kept as typed object in memory.
#[derive(Debug, Clone)]
pub struct ManifestCacheEntry {
    /// The Manifest object (typed for in-memory access)
    pub manifest: Manifest,

    /// ETag or version identifier from storage backend
    pub etag: Option<String>,

    /// Last refresh timestamp (Unix seconds)
    pub last_refreshed: i64,

    /// Source path in storage (e.g., "s3://bucket/namespace/table/manifest.json")
    pub source_path: String,

    /// Synchronization state
    pub sync_state: SyncState,
}

/// Intermediate struct for bincode-compatible serialization
#[derive(Serialize, Deserialize)]
struct ManifestCacheEntryRaw {
    manifest_json: String,
    etag: Option<String>,
    last_refreshed: i64,
    source_path: String,
    sync_state: SyncState,
}

impl Serialize for ManifestCacheEntry {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let raw = ManifestCacheEntryRaw {
            manifest_json: serde_json::to_string(&self.manifest).unwrap_or_else(|_| "{}".to_string()),
            etag: self.etag.clone(),
            last_refreshed: self.last_refreshed,
            source_path: self.source_path.clone(),
            sync_state: self.sync_state,
        };
        raw.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ManifestCacheEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = ManifestCacheEntryRaw::deserialize(deserializer)?;
        let manifest: Manifest = serde_json::from_str(&raw.manifest_json)
            .map_err(|e| serde::de::Error::custom(format!("Failed to parse manifest JSON: {}", e)))?;
        Ok(ManifestCacheEntry {
            manifest,
            etag: raw.etag,
            last_refreshed: raw.last_refreshed,
            source_path: raw.source_path,
            sync_state: raw.sync_state,
        })
    }
}

impl ManifestCacheEntry {
    /// Create a new cache entry
    pub fn new(
        manifest: Manifest,
        etag: Option<String>,
        last_refreshed: i64,
        source_path: String,
        sync_state: SyncState,
    ) -> Self {
        Self {
            manifest,
            etag,
            last_refreshed,
            source_path,
            sync_state,
        }
    }

    /// Serialize manifest to JSON string (for display in system.manifest table)
    pub fn manifest_json(&self) -> String {
        serde_json::to_string(&self.manifest).unwrap_or_else(|_| "{}".to_string())
    }

    /// Check if entry is stale based on TTL
    pub fn is_stale(&self, ttl_seconds: i64, now_timestamp: i64) -> bool {
        now_timestamp - self.last_refreshed > ttl_seconds
    }

    /// Mark entry as stale
    pub fn mark_stale(&mut self) {
        self.sync_state = SyncState::Stale;
    }

    /// Mark entry as pending write (local changes need to be synced to storage)
    pub fn mark_pending_write(&mut self) {
        self.sync_state = SyncState::PendingWrite;
    }

    /// Mark entry as syncing (Parquet write in progress to temp location)
    /// 
    /// This state indicates the flush is in progress:
    /// - Parquet file is being written to a temp location
    /// - If crash occurs, temp files can be cleaned up on restart
    /// - Transitions to InSync after successful rename to final location
    pub fn mark_syncing(&mut self) {
        self.sync_state = SyncState::Syncing;
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

    /// Schema version when this segment was written (Phase 16)
    ///
    /// Links this Parquet file to a specific table schema version.
    /// Use `TablesStore::get_version(table_id, schema_version)` to
    /// retrieve the exact TableDefinition for reading this segment.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
}

/// Default schema version for backward compatibility with existing manifests
fn default_schema_version() -> u32 {
    1
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
            schema_version: 1, // Default to version 1
        }
    }

    /// Create a new segment with a specific schema version
    pub fn with_schema_version(
        id: String,
        path: String,
        column_stats: HashMap<String, ColumnStats>,
        min_seq: i64,
        max_seq: i64,
        row_count: u64,
        size_bytes: u64,
        schema_version: u32,
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
            schema_version,
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

    /// Last batch/segment index (used to generate next batch filename: batch-{N}.parquet)
    /// This is automatically updated when add_segment() is called.
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
        // Extract batch number from segment path and update last_sequence_number
        if let Some(batch_num) = Self::extract_batch_number(&segment.path) {
            if batch_num >= self.last_sequence_number {
                self.last_sequence_number = batch_num;
            }
        }
        
        self.segments.push(segment);
        self.updated_at = chrono::Utc::now().timestamp();
        self.version += 1;
    }

    /// Extract batch number from segment path (e.g., "batch-0.parquet" -> 0)
    fn extract_batch_number(path: &str) -> Option<u64> {
        let filename = std::path::Path::new(path).file_name()?.to_str()?;
        if filename.starts_with("batch-") && filename.ends_with(".parquet") {
            filename
                .strip_prefix("batch-")?
                .strip_suffix(".parquet")?
                .parse::<u64>()
                .ok()
        } else {
            None
        }
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
        let table_id = TableId::new(NamespaceId::new("test"), TableName::new("table"));
        let manifest = Manifest::new(table_id, None);
        let entry = ManifestCacheEntry::new(
            manifest,
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
        let table_id = TableId::new(NamespaceId::new("test"), TableName::new("table"));
        let manifest = Manifest::new(table_id, None);
        let mut entry = ManifestCacheEntry::new(
            manifest,
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
    fn test_sync_state_syncing_transition() {
        let table_id = TableId::new(NamespaceId::new("test"), TableName::new("table"));
        let manifest = Manifest::new(table_id, None);
        let mut entry = ManifestCacheEntry::new(
            manifest,
            None,
            1000,
            "path".to_string(),
            SyncState::PendingWrite,
        );

        // Transition: PendingWrite -> Syncing (flush started)
        entry.mark_syncing();
        assert_eq!(entry.sync_state, SyncState::Syncing);

        // Transition: Syncing -> InSync (flush completed successfully)
        entry.mark_in_sync(Some("etag-after-flush".to_string()), 2000);
        assert_eq!(entry.sync_state, SyncState::InSync);
        assert_eq!(entry.etag, Some("etag-after-flush".to_string()));
    }

    #[test]
    fn test_sync_state_syncing_to_error() {
        let table_id = TableId::new(NamespaceId::new("test"), TableName::new("table"));
        let manifest = Manifest::new(table_id, None);
        let mut entry = ManifestCacheEntry::new(
            manifest,
            None,
            1000,
            "path".to_string(),
            SyncState::InSync,
        );

        // Transition: InSync -> Syncing (new flush started)
        entry.mark_syncing();
        assert_eq!(entry.sync_state, SyncState::Syncing);

        // Transition: Syncing -> Error (flush failed)
        entry.mark_error();
        assert_eq!(entry.sync_state, SyncState::Error);
    }

    #[test]
    fn test_sync_state_display() {
        assert_eq!(format!("{}", SyncState::InSync), "in_sync");
        assert_eq!(format!("{}", SyncState::PendingWrite), "pending_write");
        assert_eq!(format!("{}", SyncState::Syncing), "syncing");
        assert_eq!(format!("{}", SyncState::Stale), "stale");
        assert_eq!(format!("{}", SyncState::Error), "error");
    }

    #[test]
    fn test_sync_state_serialization() {
        // Test that SyncState serializes/deserializes correctly with snake_case
        let states = vec![
            (SyncState::InSync, "\"in_sync\""),
            (SyncState::PendingWrite, "\"pending_write\""),
            (SyncState::Syncing, "\"syncing\""),
            (SyncState::Stale, "\"stale\""),
            (SyncState::Error, "\"error\""),
        ];

        for (state, expected_json) in states {
            let json = serde_json::to_string(&state).unwrap();
            assert_eq!(json, expected_json, "SyncState {:?} should serialize to {}", state, expected_json);

            let deserialized: SyncState = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, state, "SyncState should round-trip through JSON");
        }
    }

    #[test]
    fn test_sync_state_full_flush_lifecycle() {
        // Simulate the complete flush lifecycle with the new Syncing state
        let table_id = TableId::new(NamespaceId::new("myns"), TableName::new("mytable"));
        let manifest = Manifest::new(table_id, None);
        let mut entry = ManifestCacheEntry::new(
            manifest,
            None,
            1000,
            "myns/mytable/manifest.json".to_string(),
            SyncState::InSync,
        );

        // 1. Data is written to hot storage (RocksDB), manifest goes PendingWrite
        entry.mark_pending_write();
        assert_eq!(entry.sync_state, SyncState::PendingWrite);

        // 2. Flush job starts, write Parquet to temp location
        entry.mark_syncing();
        assert_eq!(entry.sync_state, SyncState::Syncing);

        // 3a. Success path: rename temp -> final, update manifest
        entry.mark_in_sync(Some("etag-v2".to_string()), 2000);
        assert_eq!(entry.sync_state, SyncState::InSync);

        // OR 3b. Failure path: mark as error
        entry.mark_syncing();
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

    #[test]
    fn test_manifest_tracks_batch_numbers() {
        let table_id = TableId::new(NamespaceId::new("test"), TableName::new("table"));
        let mut manifest = Manifest::new(table_id, None);

        assert_eq!(manifest.last_sequence_number, 0);

        // Add batch-0.parquet
        let segment0 = SegmentMetadata::new(
            "batch-0".to_string(),
            "batch-0.parquet".to_string(),
            HashMap::new(),
            1000,
            2000,
            50,
            512,
        );
        manifest.add_segment(segment0);
        assert_eq!(manifest.last_sequence_number, 0);
        assert_eq!(manifest.segments.len(), 1);

        // Add batch-1.parquet
        let segment1 = SegmentMetadata::new(
            "batch-1".to_string(),
            "batch-1.parquet".to_string(),
            HashMap::new(),
            2001,
            3000,
            50,
            512,
        );
        manifest.add_segment(segment1);
        assert_eq!(manifest.last_sequence_number, 1);
        assert_eq!(manifest.segments.len(), 2);

        // Add batch-5.parquet (skip some numbers)
        let segment5 = SegmentMetadata::new(
            "batch-5".to_string(),
            "batch-5.parquet".to_string(),
            HashMap::new(),
            5001,
            6000,
            50,
            512,
        );
        manifest.add_segment(segment5);
        assert_eq!(manifest.last_sequence_number, 5);
        assert_eq!(manifest.segments.len(), 3);
    }

    #[test]
    fn test_extract_batch_number() {
        assert_eq!(Manifest::extract_batch_number("batch-0.parquet"), Some(0));
        assert_eq!(Manifest::extract_batch_number("batch-1.parquet"), Some(1));
        assert_eq!(Manifest::extract_batch_number("batch-42.parquet"), Some(42));
        assert_eq!(Manifest::extract_batch_number("batch-999.parquet"), Some(999));
        
        // Invalid formats
        assert_eq!(Manifest::extract_batch_number("other-0.parquet"), None);
        assert_eq!(Manifest::extract_batch_number("batch-0.csv"), None);
        assert_eq!(Manifest::extract_batch_number("batch.parquet"), None);
    }
}
