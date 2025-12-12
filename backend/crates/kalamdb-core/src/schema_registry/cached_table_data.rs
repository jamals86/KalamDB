use crate::error::KalamDbError;
use kalamdb_commons::models::schemas::TableDefinition;
use kalamdb_commons::models::StorageId;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

/// Cached table data containing all metadata and schema information
///
/// This struct consolidates data previously split between separate caches
/// to eliminate duplication.
///
/// **Performance Note**: Cache access updates `last_accessed` via an atomic field.
/// This avoids a separate "LRU timestamps" map (which would otherwise duplicate keys and add
/// extra DashMap overhead) while keeping per-access work O(1).
#[derive(Debug)]
pub struct CachedTableData {
    /// Full schema definition with all columns
    pub table: Arc<TableDefinition>,

    /// Reference to storage configuration in system.storages
    pub storage_id: Option<StorageId>,

    /// Partially-resolved storage path template
    /// Static placeholders substituted ({namespace}, {tableName}), dynamic ones remain ({userId}, {shard})
    pub storage_path_template: String, //TODO: Use PathBuf instead of String

    /// Current schema version number
    pub schema_version: u32,

    /// Memoized Arrow schema for DataFusion integration (Phase 10, US1, FR-002)
    ///
    /// Computed once on first access via lazy initialization with double-check locking.
    /// Eliminates repeated `to_arrow_schema()` calls (50-100μs each) after initial computation.
    ///
    /// **Performance**: 50-100× speedup for repeated schema access (75μs → 1.5μs)
    /// **Thread Safety**: RwLock allows concurrent reads, exclusive writes for initialization
    arrow_schema: Arc<RwLock<Option<Arc<datafusion::arrow::datatypes::Schema>>>>,

    /// Last access timestamp in milliseconds since Unix epoch.
    ///
    /// Used for LRU eviction in `TableCache` without maintaining a separate timestamp map.
    last_accessed_ms: AtomicU64,
}

impl Clone for CachedTableData {
    fn clone(&self) -> Self {
        Self {
            table: Arc::clone(&self.table),
            storage_id: self.storage_id.clone(),
            storage_path_template: self.storage_path_template.clone(),
            schema_version: self.schema_version,
            arrow_schema: Arc::clone(&self.arrow_schema),
            last_accessed_ms: AtomicU64::new(self.last_accessed_ms.load(Ordering::Relaxed)),
        }
    }
}

impl CachedTableData {
    fn now_millis() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    /// Create new cached table data
    #[allow(clippy::too_many_arguments)]
    pub fn new(schema: Arc<TableDefinition>) -> Self {
        let schema_version = schema.schema_version;
        Self {
            table: schema,
            storage_id: None,
            storage_path_template: String::new(),
            schema_version,
            arrow_schema: Arc::new(RwLock::new(None)), // Phase 10, US1, FR-002: Lazy init on first access
            last_accessed_ms: AtomicU64::new(Self::now_millis()),
        }
    }

    pub fn touch_at(&self, timestamp_ms: u64) {
        self.last_accessed_ms
            .store(timestamp_ms, Ordering::Relaxed);
    }

    pub fn last_accessed_ms(&self) -> u64 {
        self.last_accessed_ms.load(Ordering::Relaxed)
    }

    /// Get or compute Arrow schema with double-check locking (Phase 10, US1, FR-003)
    ///
    /// **Performance**: First call computes schema (~75μs), subsequent calls return cached Arc (~1.5μs)
    /// This achieves 50-100× speedup for repeated access.
    ///
    /// **Thread Safety**: Uses double-check locking pattern:
    /// 1. Fast path: Read lock → check if Some → return Arc::clone (concurrent reads)
    /// 2. Slow path: Write lock → double-check → compute → cache → return (exclusive write)
    ///
    /// # Returns
    /// Arc-wrapped Arrow Schema for zero-copy sharing across TableProvider instances
    ///
    /// # Panics
    /// Panics if RwLock is poisoned (unrecoverable lock corruption)
    pub fn arrow_schema(&self) -> Result<Arc<datafusion::arrow::datatypes::Schema>, KalamDbError> {
        // Fast path: Check if already computed (concurrent reads allowed)
        {
            let read_guard = self
                .arrow_schema
                .read()
                .expect("RwLock poisoned: arrow_schema read lock failed");
            if let Some(schema) = read_guard.as_ref() {
                return Ok(Arc::clone(schema)); // 1.5μs cached access
            }
        }

        // Slow path: Compute and cache (exclusive write)
        {
            let mut write_guard = self
                .arrow_schema
                .write()
                .expect("RwLock poisoned: arrow_schema write lock failed");

            // Double-check: Another thread may have computed while we waited for write lock
            if let Some(schema) = write_guard.as_ref() {
                return Ok(Arc::clone(schema));
            }

            // Compute Arrow schema from TableDefinition (~75μs first time)
            let arrow_schema = self.table.to_arrow_schema().map_err(|e| {
                KalamDbError::SchemaError(format!("Failed to convert to Arrow schema: {}", e))
            })?;

            // Cache for future access
            *write_guard = Some(Arc::clone(&arrow_schema));

            Ok(arrow_schema)
        }
    }
}
