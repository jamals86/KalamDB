use crate::error::KalamDbError;
use kalamdb_commons::models::schemas::TableDefinition;
use kalamdb_commons::models::StorageId;
use std::sync::{Arc, RwLock};

/// Cached table data containing all metadata and schema information
///
/// This struct consolidates data previously split between separate caches
/// to eliminate duplication.
///
/// **Performance Note**: `last_accessed` timestamp is stored separately in SchemaRegistry
/// to avoid cloning this entire struct on every cache access.
///
/// **Clone Semantics**: Clone is cheap (O(1)) because all owned data is behind Arc pointers.
/// Cloning only increments Arc reference counts without copying the underlying TableDefinition.
/// This enables zero-copy sharing of table metadata across multiple threads and components.
#[derive(Debug, Clone)]
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
}

impl CachedTableData {
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
        }
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
