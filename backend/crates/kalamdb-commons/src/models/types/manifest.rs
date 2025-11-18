//! Manifest cache models for query optimization (Phase 4 - US6).
//!
//! This module defines ManifestFile and ManifestCacheEntry structures for
//! tracking Parquet batch file metadata and caching manifests in RocksDB.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::models::schemas::TableType;
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

/// Status of a batch file in manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchStatus {
    /// Batch file is active and queryable
    Active,
    /// Batch file is being compacted
    Compacting,
    /// Batch file has been archived after compaction
    Archived,
}

impl Default for BatchStatus {
    fn default() -> Self {
        Self::Active
    }
}

/// Row group level pruning stats for advanced indexing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RowGroupPruningStats {
    /// Row group index within the Parquet file
    pub index: u32,

    /// Total number of rows in the row group
    pub row_count: u64,

    /// Minimum sequence id observed in the row group
    pub min_seq: i64,

    /// Maximum sequence id observed in the row group
    pub max_seq: i64,

    /// Column min/max bounds for predicate pruning
    #[serde(default)]
    pub column_min_max: HashMap<String, (serde_json::Value, serde_json::Value)>,

    /// Optional byte range for fast IO planning (start, length)
    #[serde(default)]
    pub byte_range: Option<(u64, u64)>,
}

impl RowGroupPruningStats {
    /// Create a new row group pruning stats entry
    pub fn new(
        index: u32,
        row_count: u64,
        min_seq: i64,
        max_seq: i64,
        column_min_max: HashMap<String, (serde_json::Value, serde_json::Value)>,
        byte_range: Option<(u64, u64)>,
    ) -> Self {
        Self {
            index,
            row_count,
            min_seq,
            max_seq,
            column_min_max,
            byte_range,
        }
    }
}

impl Default for RowGroupPruningStats {
    fn default() -> Self {
        Self {
            index: 0,
            row_count: 0,
            min_seq: 0,
            max_seq: 0,
            column_min_max: HashMap::new(),
            byte_range: None,
        }
    }
}

/// Entry for a single batch file in the manifest (Phase 5 - US2, extended for DataFusion indexing).
///
/// Tracks metadata about a Parquet batch file for query optimization:
/// - Row counts and size for cost estimation
/// - Min/max timestamps for temporal pruning
/// - Column min/max values for predicate pushdown
/// - Optional row-group level statistics for fine grained pruning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchFileEntry {
    /// Batch number (sequential, starting from 0)
    pub batch_number: u64,

    /// Relative file path (e.g., "batch-0.parquet")
    pub file_path: String,

    /// Minimum _seq value in this batch (for MVCC version pruning)
    pub min_seq: i64,

    /// Maximum _seq value in this batch (for MVCC version pruning)
    pub max_seq: i64,

    /// Number of rows in this batch
    pub row_count: u64,

    /// File size in bytes
    pub size_bytes: u64,

    /// Schema version at time of flush
    pub schema_version: u32,

    /// Batch file status
    pub status: BatchStatus,

    /// Indicates whether the Parquet writer stored page index metadata
    #[serde(default)]
    pub has_page_index: bool,

    /// Optional cached footer size to avoid re-reading file metadata
    #[serde(default)]
    pub footer_size: Option<u64>,

    /// Row group level pruning metadata (empty when not collected)
    #[serde(default)]
    pub row_groups: Vec<RowGroupPruningStats>,
}

impl BatchFileEntry {
    /// Create a new batch file entry
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        batch_number: u64,
        file_path: String,
        min_seq: i64,
        max_seq: i64,
        row_count: u64,
        size_bytes: u64,
        schema_version: u32,
    ) -> Self {
        Self {
            batch_number,
            file_path,
            min_seq,
            max_seq,
            row_count,
            size_bytes,
            schema_version,
            status: BatchStatus::Active,
            has_page_index: false,
            footer_size: None,
            row_groups: Vec::new(),
        }
    }

    /// Check if batch overlaps with sequence range (for MVCC version pruning)
    pub fn overlaps_seq_range(&self, min_seq: i64, max_seq: i64) -> bool {
        !(self.max_seq < min_seq || self.min_seq > max_seq)
    }
}

/// Manifest file tracking batch files for a table (Phase 5 - US2).
///
/// The manifest file is the single source of truth for which Parquet batch files
/// exist for a table, along with metadata for query optimization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestFile {
    /// Table identifier (namespace + table name)
    pub table_id: TableId,

    /// Table type (User or Shared)
    pub table_type: TableType,

    /// Scope: user_id for USER tables, None for SHARED tables
    pub user_id: Option<UserId>,

    /// Manifest format version (for backward compatibility)
    pub version: u32,

    /// Timestamp when manifest was generated (Unix seconds)
    pub generated_at: i64,

    /// Highest batch number (max(batches.batch_number))
    pub max_batch: u64,

    /// List of batch file entries
    pub batches: Vec<BatchFileEntry>,
}

impl ManifestFile {
    /// Create a new empty manifest
    pub fn new(table_id: TableId, table_type: TableType, user_id: Option<UserId>) -> Self {
        Self {
            table_id,
            table_type,
            user_id,
            version: 1,
            generated_at: chrono::Utc::now().timestamp(),
            max_batch: 0,
            batches: Vec::new(),
        }
    }

    /// Add a new batch entry and increment max_batch
    pub fn add_batch(&mut self, batch: BatchFileEntry) {
        self.max_batch = self.max_batch.max(batch.batch_number);
        self.batches.push(batch);
        self.generated_at = chrono::Utc::now().timestamp();
    }

    /// Validate that max_batch matches actual batches
    pub fn validate(&self) -> Result<(), String> {
        if let Some(max) = self.batches.iter().map(|b| b.batch_number).max() {
            if max != self.max_batch {
                return Err(format!(
                    "max_batch mismatch: manifest says {}, actual max is {}",
                    self.max_batch, max
                ));
            }
        } else if self.max_batch != 0 {
            return Err(format!(
                "max_batch is {} but no batches found",
                self.max_batch
            ));
        }
        Ok(())
    }

    /// Serialize to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserialize from JSON string
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Get next batch number for new flush
    pub fn next_batch_number(&self) -> u64 {
        self.max_batch + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_batch_file_entry_overlaps_seq_range() {
        let batch = BatchFileEntry::new(0, "batch-0.parquet".to_string(), 1000, 2000, 100, 1024, 1);

        // Overlaps
        assert!(batch.overlaps_seq_range(1500, 2500));
        assert!(batch.overlaps_seq_range(500, 1500));
        assert!(batch.overlaps_seq_range(1000, 2000));

        // No overlap
        assert!(!batch.overlaps_seq_range(2001, 3000));
        assert!(!batch.overlaps_seq_range(0, 999));
    }

    #[test]
    fn test_manifest_file_add_batch() {
        use crate::models::schemas::TableType;
        use crate::{NamespaceId, TableName, UserId};

        let table_id = TableId::new(NamespaceId::new("test"), TableName::new("table"));
        let mut manifest =
            ManifestFile::new(table_id, TableType::User, Some(UserId::from("user_123")));

        assert_eq!(manifest.max_batch, 0);
        assert_eq!(manifest.batches.len(), 0);

        let batch = BatchFileEntry::new(1, "batch-1.parquet".to_string(), 1000, 2000, 50, 512, 1);

        manifest.add_batch(batch);
        assert_eq!(manifest.max_batch, 1);
        assert_eq!(manifest.batches.len(), 1);
    }

    #[test]
    fn test_manifest_file_validate() {
        use crate::models::schemas::TableType;
        use crate::{NamespaceId, TableName};

        let table_id = TableId::new(NamespaceId::new("test"), TableName::new("table"));
        let mut manifest = ManifestFile::new(table_id, TableType::Shared, None);

        // Empty manifest is valid
        assert!(manifest.validate().is_ok());

        // Add batch
        let batch = BatchFileEntry::new(1, "batch-1.parquet".to_string(), 1000, 2000, 50, 512, 1);
        manifest.add_batch(batch);
        assert!(manifest.validate().is_ok());

        // Manually corrupt max_batch
        manifest.max_batch = 99;
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_manifest_file_json_roundtrip() {
        use crate::models::schemas::TableType;
        use crate::{NamespaceId, TableName, UserId};

        let table_id = TableId::new(NamespaceId::new("test"), TableName::new("table"));
        let mut manifest =
            ManifestFile::new(table_id, TableType::User, Some(UserId::from("user_456")));

        let batch = BatchFileEntry::new(0, "batch-0.parquet".to_string(), 1000, 2000, 100, 1024, 1);
        manifest.add_batch(batch);

        let json = manifest.to_json().unwrap();
        let roundtrip = ManifestFile::from_json(&json).unwrap();

        assert_eq!(manifest.table_id, roundtrip.table_id);
        assert_eq!(manifest.table_type, roundtrip.table_type);
        assert_eq!(manifest.user_id, roundtrip.user_id);
        assert_eq!(manifest.max_batch, roundtrip.max_batch);
        assert_eq!(manifest.batches.len(), roundtrip.batches.len());
    }
}
