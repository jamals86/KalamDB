use crate::error::KalamDbError;
use kalamdb_commons::models::schemas::TableDefinition;
use kalamdb_commons::models::{StorageId, TableId};
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

    /// Create a fully initialized CachedTableData with storage configuration
    ///
    /// This is the preferred way to create CachedTableData as it ensures all
    /// storage-related fields are properly set. Use this method in:
    /// - CREATE TABLE
    /// - ALTER TABLE  
    /// - Startup/persistence loading
    ///
    /// # Arguments
    /// * `table_def` - The table definition
    /// * `storage_id` - Storage ID (from table options)
    /// * `storage_path_template` - Pre-resolved storage path template
    pub fn with_storage_config(
        table_def: Arc<TableDefinition>,
        storage_id: Option<StorageId>,
        storage_path_template: String,
    ) -> Self {
        let schema_version = table_def.schema_version;
        Self {
            table: table_def,
            storage_id,
            storage_path_template,
            schema_version,
            arrow_schema: Arc::new(RwLock::new(None)),
            last_accessed_ms: AtomicU64::new(Self::now_millis()),
        }
    }

    /// Extract storage ID from table definition options
    ///
    /// Returns the storage_id from the table's options, or None for system tables.
    pub fn extract_storage_id(table_def: &TableDefinition) -> Option<StorageId> {
        use kalamdb_commons::schemas::TableOptions;
        match &table_def.table_options {
            TableOptions::User(opts) => Some(opts.storage_id.clone()),
            TableOptions::Shared(opts) => Some(opts.storage_id.clone()),
            TableOptions::Stream(_) => Some(StorageId::from("local")), // Default for streams
            TableOptions::System(_) => None,
        }
    }

    /// Create CachedTableData from TableDefinition by resolving storage configuration
    ///
    /// This factory method handles the complete initialization including:
    /// 1. Extracting storage_id from table options
    /// 2. Resolving the storage path template via PathResolver
    ///
    /// Use this for loading tables from persistence (startup, cache miss).
    ///
    /// # Arguments
    /// * `table_id` - The table identifier
    /// * `table_def` - The table definition
    ///
    /// # Returns
    /// A fully initialized CachedTableData with storage fields populated
    pub fn from_table_definition(
        table_id: &TableId,
        table_def: Arc<TableDefinition>,
    ) -> Result<Self, KalamDbError> {
        use crate::schema_registry::PathResolver;

        let storage_id = Self::extract_storage_id(&table_def);
        let table_type = table_def.table_type;

        let storage_path_template = if let Some(ref sid) = storage_id {
            PathResolver::resolve_storage_path_template(table_id, table_type, sid).unwrap_or_else(
                |e| {
                    log::warn!(
                        "Failed to resolve storage path template for {}: {}. Using empty template.",
                        table_id,
                        e
                    );
                    String::new()
                },
            )
        } else {
            String::new()
        };

        Ok(Self::with_storage_config(
            table_def,
            storage_id,
            storage_path_template,
        ))
    }

    /// Create CachedTableData for an altered table, preserving storage config from old entry
    ///
    /// Use this when updating a table after ALTER TABLE. It preserves the storage
    /// configuration from the existing cache entry to avoid recalculation.
    ///
    /// # Arguments
    /// * `table_id` - The table identifier
    /// * `new_table_def` - The updated table definition
    /// * `old_entry` - Optional previous cache entry to copy storage config from
    ///
    /// # Returns
    /// A CachedTableData with updated schema but preserved storage config
    pub fn from_altered_table(
        table_id: &TableId,
        new_table_def: Arc<TableDefinition>,
        old_entry: Option<&CachedTableData>,
    ) -> Result<Self, KalamDbError> {
        if let Some(old) = old_entry {
            // Preserve storage config from old entry
            Ok(Self::with_storage_config(
                new_table_def,
                old.storage_id.clone(),
                old.storage_path_template.clone(),
            ))
        } else {
            // No old entry - fully resolve storage config
            log::warn!(
                "⚠️  No existing cache entry for {} during ALTER. Resolving storage config...",
                table_id
            );
            Self::from_table_definition(table_id, new_table_def)
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
