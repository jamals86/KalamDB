//! Base trait for table providers with unified DML operations
//!
//! This module provides:
//! - BaseTableProvider<K, V> trait for generic table operations
//! - TableProviderCore shared structure for common services
//!
//! **Design Rationale**:
//! - Eliminates ~1200 lines of duplicate code across User/Shared/Stream providers
//! - Generic over storage key (K) and value (V) types
//! - No separate handlers - DML logic implemented directly in providers
//! - Shared core reduces memory overhead (Arc<TableProviderCore> vs per-provider fields)

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::live_query::LiveQueryManager;
use crate::manifest::ManifestAccessPlanner;
use crate::providers::arrow_json_conversion::json_rows_to_arrow_batch;
use crate::providers::unified_dml;
use crate::schema_registry::TableType;
use crate::storage::storage_registry::StorageRegistry;
use async_trait::async_trait;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::catalog::Session;
use datafusion::datasource::TableProvider;
use datafusion::logical_expr::Expr;
use kalamdb_commons::models::{NamespaceId, TableName, UserId};
use kalamdb_commons::types::ManifestFile;
use kalamdb_commons::{StorageKey, TableId};
use kalamdb_tables::{SharedTableRow, StreamTableRow, UserTableRow};
use once_cell::sync::Lazy;
use serde_json::{Map, Value as JsonValue};
use std::path::PathBuf;
use std::sync::Arc;

static SYSTEM_USER_ID: Lazy<UserId> = Lazy::new(|| UserId::from("_system"));

/// Shared core state for all table providers
///
/// **Memory Optimization**: All provider types share this core structure,
/// reducing per-table memory footprint from 3× allocation to 1× allocation.
///
/// **Phase 12 Refactoring**: Uses kalamdb-registry services directly
///
/// **Services**:
/// - `app_context`: Application context for global services (required for trait methods)
/// - `schema_registry`: Table schema management and caching (from kalamdb-registry)
/// - `system_columns`: SeqId generation, _deleted flag handling (from kalamdb-registry)
/// - `live_query_manager`: WebSocket notifications (optional, from kalamdb-core)
/// - `storage_registry`: Storage path resolution (optional, from kalamdb-core)
pub struct TableProviderCore {
    /// Application context for global services (kept for BaseTableProvider trait)
    pub app_context: Arc<AppContext>,

    /// Table identity shared by provider implementations
    table_id: Arc<TableId>,

    /// Logical table type used for routing/storage decisions
    table_type: TableType,

    /// Schema registry for table metadata and Arrow schema caching
    pub schema_registry: Arc<crate::schema_registry::SchemaRegistry>,

    /// System columns service for _seq and _deleted management
    pub system_columns: Arc<crate::system_columns::SystemColumnsService>,

    /// LiveQueryManager for WebSocket notifications (optional)
    pub live_query_manager: Option<Arc<LiveQueryManager>>,

    /// Storage registry for resolving full storage paths (optional)
    pub storage_registry: Option<Arc<StorageRegistry>>,
}

impl TableProviderCore {
    /// Create new core with required services from AppContext
    pub fn new(app_context: Arc<AppContext>, table_id: TableId, table_type: TableType) -> Self {
        Self {
            table_id: Arc::new(table_id),
            table_type,
            schema_registry: app_context.schema_registry(),
            system_columns: app_context.system_columns_service(),
            live_query_manager: None,
            storage_registry: None,
            app_context,
        }
    }

    /// Backwards-compatible helper to build from borrowed AppContext
    pub fn from_app_context(
        app_context: &Arc<AppContext>,
        table_id: TableId,
        table_type: TableType,
    ) -> Self {
        Self::new(app_context.clone(), table_id, table_type)
    }

    /// Add LiveQueryManager to core
    pub fn with_live_query_manager(mut self, manager: Arc<LiveQueryManager>) -> Self {
        self.live_query_manager = Some(manager);
        self
    }

    /// Add StorageRegistry to core
    pub fn with_storage_registry(mut self, registry: Arc<StorageRegistry>) -> Self {
        self.storage_registry = Some(registry);
        self
    }

    /// TableId accessor (shared across providers)
    pub fn table_id(&self) -> &TableId {
        self.table_id.as_ref()
    }

    /// Cloneable TableId handle (avoids leaking Arc internals to callers)
    pub fn table_id_arc(&self) -> Arc<TableId> {
        self.table_id.clone()
    }

    /// TableType accessor
    pub fn table_type(&self) -> TableType {
        self.table_type
    }
}

/// Unified trait for all table providers with generic storage abstraction
///
/// **Key Design Decisions**:
/// - Generic K: StorageKey (UserTableRowId, SharedTableRowId, StreamTableRowId)
/// - Generic V: Row type (UserTableRow, SharedTableRow, StreamTableRow)
/// - Extends DataFusion::TableProvider (same struct serves both custom DML + SQL)
/// - No separate handlers - all DML logic in provider implementations
/// - Stateless providers - user_id passed per-operation, not stored per-user
///
/// **Architecture**:
/// ```text
/// ExecutionContext → SessionState.extensions (SessionUserContext)
///                 ↓
/// Provider.scan_rows(state) → extract_user_context(state)
///                           ↓
/// Provider.scan_with_version_resolution_to_kvs(user_id, filter)
/// ```
#[async_trait]
pub trait BaseTableProvider<K: StorageKey, V>: Send + Sync + TableProvider {
    // ===========================
    // Core Metadata (read-only)
    // ===========================

    /// Table identifier (namespace + table name)
    fn table_id(&self) -> &TableId;

    /// Memoized Arrow schema (Phase 10 optimization: 50-100× faster than recomputation)
    fn schema_ref(&self) -> SchemaRef;

    /// Logical table type (User, Shared, Stream)
    ///
    /// Named differently from DataFusion's TableProvider::table_type to avoid ambiguity.
    fn provider_table_type(&self) -> TableType;

    /// Get namespace ID from table_id (default implementation)
    fn namespace_id(&self) -> &NamespaceId {
        self.table_id().namespace_id()
    }

    /// Get table name from table_id (default implementation)
    fn table_name(&self) -> &TableName {
        self.table_id().table_name()
    }

    /// Get RocksDB column family name (default implementation)
    fn column_family_name(&self) -> String {
        format!(
            "{}:{}:{}",
            match <Self as BaseTableProvider<K, V>>::provider_table_type(self) {
                TableType::User => "user_table",
                TableType::Shared => "shared_table",
                TableType::Stream => "stream_table",
                _ => "table",
            },
            self.namespace_id().as_str(),
            self.table_name().as_str()
        )
    }

    // ===========================
    // Storage Access
    // ===========================

    /// Access to AppContext for SystemColumnsService, SnowflakeGenerator, etc.
    fn app_context(&self) -> &Arc<AppContext>;

    /// Primary key field name from schema definition (e.g., "id", "email")
    fn primary_key_field_name(&self) -> &str;

    // ===========================
    // DML Operations (Synchronous - No Handlers)
    // ===========================

    /// Insert a single row (auto-generates system columns: _seq, _deleted)
    ///
    /// **Implementation**: Calls unified_dml helpers directly
    ///
    /// # Arguments
    /// * `user_id` - Subject user ID for RLS (User/Stream use it, Shared ignores it)
    /// * `row_data` - JSON object containing user-defined columns
    ///
    /// # Returns
    /// Generated storage key (UserTableRowId, SharedTableRowId, or StreamTableRowId)
    ///
    /// # Architecture Note
    /// Providers are stateless. The user_id is passed per-operation by the SQL executor
    /// from ExecutionContext, enabling:
    /// - AS USER impersonation (executor passes subject_user_id)
    /// - Per-request user scoping without per-user provider instances
    /// - Clean separation: executor handles auth/context, provider handles storage
    fn insert(&self, user_id: &UserId, row_data: JsonValue) -> Result<K, KalamDbError>;

    /// Insert multiple rows in a batch (optimized for bulk operations)
    ///
    /// # Arguments
    /// * `user_id` - Subject user ID for RLS
    /// * `rows` - Vector of JSON objects
    ///
    /// # Default Implementation
    /// Iterates over rows and calls insert() for each. Providers may override
    /// with batch-optimized implementation.
    fn insert_batch(&self, user_id: &UserId, rows: Vec<JsonValue>) -> Result<Vec<K>, KalamDbError> {
        rows.into_iter()
            .map(|row| self.insert(user_id, row))
            .collect()
    }

    /// Update a row by key (appends new version with incremented _seq)
    ///
    /// **Implementation**: Uses version_resolution helpers + unified_dml
    ///
    /// # Arguments
    /// * `user_id` - Subject user ID for RLS
    /// * `key` - Storage key identifying the row
    /// * `updates` - JSON object with column updates
    ///
    /// # Returns
    /// New storage key (new SeqId for versioning)
    fn update(&self, user_id: &UserId, key: &K, updates: JsonValue) -> Result<K, KalamDbError>;

    /// Delete a row by key (appends tombstone with _deleted=true)
    ///
    /// **Implementation**: Uses version_resolution helpers + unified_dml
    ///
    /// # Arguments
    /// * `user_id` - Subject user ID for RLS
    /// * `key` - Storage key identifying the row
    fn delete(&self, user_id: &UserId, key: &K) -> Result<(), KalamDbError>;

    /// Update multiple rows in a batch (default implementation)
    fn update_batch(
        &self,
        user_id: &UserId,
        updates: Vec<(K, JsonValue)>,
    ) -> Result<Vec<K>, KalamDbError> {
        updates
            .into_iter()
            .map(|(key, update)| self.update(user_id, &key, update))
            .collect()
    }

    /// Delete multiple rows in a batch (default implementation)
    fn delete_batch(&self, user_id: &UserId, keys: Vec<K>) -> Result<Vec<()>, KalamDbError> {
        keys.into_iter()
            .map(|key| self.delete(user_id, &key))
            .collect()
    }

    // ===========================
    // Convenience Methods (with default implementations)
    // ===========================

    /// Find row key by ID field value
    ///
    /// Scans rows with version resolution and returns the key of the first row
    /// where `fields.id == id_value`. The returned key K already contains user_id
    /// for user/stream tables (embedded in UserTableRowId/StreamTableRowId).
    ///
    /// # Arguments
    /// * `user_id` - Subject user ID for RLS scoping
    /// * `id_value` - Value to search for in the ID field
    ///
    /// # Performance
    /// - User tables: RocksDB prefix scan on {user_id} for efficient scoping
    /// - Shared tables: Full table scan (consider adding index for large tables)
    fn find_row_key_by_id_field(
        &self,
        user_id: &UserId,
        id_value: &str,
    ) -> Result<Option<K>, KalamDbError> {
        // Default implementation: scan rows with user scoping and version resolution
        let rows = self.scan_with_version_resolution_to_kvs(user_id, None)?;

        for (key, row) in rows {
            if let Some(fields) = Self::extract_fields(&row) {
                if let Some(id) = fields.get(self.primary_key_field_name()) {
                    // Compare robustly: support numeric and string IDs
                    let matches = match id {
                        serde_json::Value::String(s) => s == id_value,
                        serde_json::Value::Number(n) => {
                            // Compare as exact string, and also as i64 if parseable
                            let num_str = n.to_string();
                            if num_str == id_value {
                                true
                            } else if let Ok(iv) = id_value.parse::<i64>() {
                                n.as_i64().map(|x| x == iv).unwrap_or(false)
                            } else {
                                false
                            }
                        }
                        _ => false,
                    };
                    if matches {
                        return Ok(Some(key));
                    }
                }
            }
        }

        Ok(None)
    }

    /// Update a row by searching for matching ID field value
    fn update_by_id_field(
        &self,
        user_id: &UserId,
        id_value: &str,
        updates: JsonValue,
    ) -> Result<K, KalamDbError> {
        let key = self
            .find_row_key_by_id_field(user_id, id_value)?
            .ok_or_else(|| {
                KalamDbError::NotFound(format!(
                    "Row with {}={} not found",
                    self.primary_key_field_name(),
                    id_value
                ))
            })?;
        self.update(user_id, &key, updates)
    }

    /// Delete a row by searching for matching ID field value
    fn delete_by_id_field(&self, user_id: &UserId, id_value: &str) -> Result<(), KalamDbError> {
        let key = self
            .find_row_key_by_id_field(user_id, id_value)?
            .ok_or_else(|| {
                KalamDbError::NotFound(format!(
                    "Row with {}={} not found",
                    self.primary_key_field_name(),
                    id_value
                ))
            })?;
        self.delete(user_id, &key)
    }

    // ===========================
    // Scan Operations (with version resolution)
    // ===========================

    /// Scan rows with optional filter (merges hot + cold storage with version resolution)
    ///
    /// **Called by DataFusion during query execution via TableProvider::scan()**
    ///
    /// The `state` parameter contains SessionUserContext in extensions,
    /// which providers extract to apply RLS filtering.
    ///
    /// **User/Shared Tables**:
    /// 1. Extract user_id from SessionState.config().options().extensions
    /// 2. Scan RocksDB (hot storage)
    /// 3. Scan Parquet files (cold storage)
    /// 4. Apply version resolution (MAX(_seq) per primary key) via DataFusion
    /// 5. Filter _deleted = false
    /// 6. Apply user filter expression
    /// 7. For User tables: Apply RLS (user_id = subject)
    ///
    /// **Stream Tables**:
    /// 1. Extract user_id from SessionState
    /// 2. Scan ONLY RocksDB (hot storage, no Parquet)
    /// 3. Apply TTL filtering
    /// 4. Filter _deleted = false (if applicable)
    /// 5. Apply user filter expression
    /// 6. Apply RLS (user_id = subject)
    ///
    /// # Arguments
    /// * `state` - DataFusion SessionState (contains SessionUserContext)
    /// * `filter` - Optional DataFusion expression for filtering
    ///
    /// # Returns
    /// RecordBatch with resolved, filtered rows
    ///
    /// # Note
    /// Called by DataFusion's TableProvider::scan(). For direct DML operations,
    /// use scan_with_version_resolution_to_kvs().
    fn scan_rows(
        &self,
        state: &dyn Session,
        filter: Option<&Expr>,
    ) -> Result<RecordBatch, KalamDbError>;

    /// Scan with version resolution returning key-value pairs (for internal DML use)
    ///
    /// Used by UPDATE/DELETE to find current version before appending new version.
    /// Unlike scan_rows(), this is called directly by DML operations with user_id
    /// passed explicitly.
    ///
    /// # Arguments
    /// * `user_id` - Subject user ID for RLS scoping
    /// * `filter` - Optional DataFusion expression for filtering
    fn scan_with_version_resolution_to_kvs(
        &self,
        user_id: &UserId,
        filter: Option<&Expr>,
    ) -> Result<Vec<(K, V)>, KalamDbError>;

    /// Extract fields JSON from row (provider-specific)
    ///
    /// Each provider implements this to access the `fields: JsonValue` from their row type.
    fn extract_fields(row: &V) -> Option<&JsonValue>;
}

/// Helper function to inject system columns (_seq, _deleted) into JSON object
///
/// This consolidates the duplicated logic across UserTableProvider, SharedTableProvider,
/// and StreamTableProvider scan_rows() methods.
///
/// # Arguments
/// * `schema` - Arrow schema to check which system columns exist
/// * `obj` - Mutable JSON object to inject system columns into
/// * `seq_value` - _seq column value (i64)
/// * `deleted_value` - _deleted column value (bool)
///
/// # Note
/// Only injects columns that exist in the schema to avoid adding unexpected fields.
pub fn inject_system_columns(
    schema: &SchemaRef,
    obj: &mut serde_json::Map<String, JsonValue>,
    seq_value: i64,
    deleted_value: bool,
) {
    let has_seq = schema.field_with_name("_seq").is_ok();
    let has_deleted = schema.field_with_name("_deleted").is_ok();

    if has_seq {
        obj.insert("_seq".to_string(), serde_json::json!(seq_value));
    }
    if has_deleted {
        obj.insert("_deleted".to_string(), serde_json::json!(deleted_value));
    }
}

/// Trait implemented by provider row types to expose system columns and JSON payload
pub trait ScanRow {
    fn field_map(&self) -> Option<&Map<String, JsonValue>>;
    fn seq_value(&self) -> i64;
    fn deleted_flag(&self) -> bool;
}

impl ScanRow for SharedTableRow {
    fn field_map(&self) -> Option<&Map<String, JsonValue>> {
        self.fields.as_object()
    }

    fn seq_value(&self) -> i64 {
        self._seq.as_i64()
    }

    fn deleted_flag(&self) -> bool {
        self._deleted
    }
}

impl ScanRow for UserTableRow {
    fn field_map(&self) -> Option<&Map<String, JsonValue>> {
        self.fields.as_object()
    }

    fn seq_value(&self) -> i64 {
        self._seq.as_i64()
    }

    fn deleted_flag(&self) -> bool {
        self._deleted
    }
}

impl ScanRow for StreamTableRow {
    fn field_map(&self) -> Option<&Map<String, JsonValue>> {
        self.fields.as_object()
    }

    fn seq_value(&self) -> i64 {
        self._seq.as_i64()
    }

    fn deleted_flag(&self) -> bool {
        false
    }
}

/// Convert resolved key-value rows into an Arrow RecordBatch with system columns injected
pub fn rows_to_arrow_batch<K, R, F>(
    schema: &SchemaRef,
    kvs: Vec<(K, R)>,
    mut enrich_row: F,
) -> Result<RecordBatch, KalamDbError>
where
    R: ScanRow,
    F: FnMut(&mut Map<String, JsonValue>, &R),
{
    let mut rows: Vec<JsonValue> = Vec::with_capacity(kvs.len());

    for (_key, row) in kvs.into_iter() {
        let mut obj = row.field_map().cloned().unwrap_or_else(|| Map::new());

        enrich_row(&mut obj, &row);

        inject_system_columns(schema, &mut obj, row.seq_value(), row.deleted_flag());
        rows.push(JsonValue::Object(obj));
    }

    json_rows_to_arrow_batch(schema, rows)
        .map_err(|e| KalamDbError::InvalidOperation(format!("Failed to build Arrow batch: {}", e)))
}

/// Shared helper for loading Parquet batches via ManifestAccessPlanner.
pub(crate) fn scan_parquet_files_as_batch(
    core: &TableProviderCore,
    table_id: &TableId,
    table_type: TableType,
    user_id: Option<&UserId>,
    schema: SchemaRef,
    filter: Option<&Expr>,
) -> Result<RecordBatch, KalamDbError> {
    use datafusion::logical_expr::Operator;

    let namespace = table_id.namespace_id();
    let table = table_id.table_name();
    let scope_label = user_id
        .map(|uid| format!("user={}", uid.as_str()))
        .unwrap_or_else(|| format!("scope={}", table_type.as_str()));

    let storage_path = core
        .app_context
        .schema_registry()
        .get_storage_path(table_id, user_id, None)?;
    let storage_dir = PathBuf::from(&storage_path);

    if !storage_dir.exists() {
        log::trace!(
            "No Parquet directory exists ({}) for table {}.{} - returning empty batch",
            scope_label,
            namespace.as_str(),
            table.as_str()
        );
        return Ok(RecordBatch::new_empty(schema.clone()));
    }

    let manifest_cache_service = core.app_context.manifest_cache_service();
    let cache_result = manifest_cache_service.get_or_load(namespace, table, user_id);
    let mut manifest_opt: Option<ManifestFile> = None;
    let mut use_degraded_mode = false;

    match &cache_result {
        Ok(Some(entry)) => match ManifestFile::from_json(&entry.manifest_json) {
            Ok(manifest) => {
                if let Err(e) = manifest.validate() {
                    log::warn!(
                        "⚠️  [MANIFEST CORRUPTION] table={}.{} {} error={} | Triggering rebuild",
                        namespace.as_str(),
                        table.as_str(),
                        scope_label,
                        e
                    );
                    use_degraded_mode = true;
                    let manifest_service = core.app_context.manifest_service();
                    let ns = namespace.clone();
                    let tbl = table.clone();
                    let uid = user_id.cloned();
                    let scope_for_spawn = scope_label.clone();
                    let manifest_table_type = table_type.clone();
                    tokio::spawn(async move {
                        log::info!(
                            "🔧 [MANIFEST REBUILD STARTED] table={}.{} {}",
                            ns.as_str(),
                            tbl.as_str(),
                            scope_for_spawn
                        );
                        match manifest_service.rebuild_manifest(
                            &ns,
                            &tbl,
                            manifest_table_type,
                            uid.as_ref(),
                        ) {
                            Ok(_) => {
                                log::info!(
                                    "✅ [MANIFEST REBUILD COMPLETED] table={}.{} {}",
                                    ns.as_str(),
                                    tbl.as_str(),
                                    scope_for_spawn
                                );
                            }
                            Err(e) => {
                                log::error!(
                                    "❌ [MANIFEST REBUILD FAILED] table={}.{} {} error={}",
                                    ns.as_str(),
                                    tbl.as_str(),
                                    scope_for_spawn,
                                    e
                                );
                            }
                        }
                    });
                } else {
                    manifest_opt = Some(manifest);
                }
            }
            Err(e) => {
                log::warn!(
                    "⚠️  Failed to parse manifest JSON for table={}.{} {}: {} | Using degraded mode",
                    namespace.as_str(),
                    table.as_str(),
                    scope_label,
                    e
                );
                use_degraded_mode = true;
            }
        },
        Ok(None) => {
            log::debug!(
                "⚠️  Manifest cache MISS | table={}.{} | {} | fallback=directory_scan",
                namespace.as_str(),
                table.as_str(),
                scope_label
            );
            use_degraded_mode = true;
        }
        Err(e) => {
            log::warn!(
                "⚠️  Manifest cache ERROR | table={}.{} | {} | error={} | fallback=directory_scan",
                namespace.as_str(),
                table.as_str(),
                scope_label,
                e
            );
            use_degraded_mode = true;
        }
    }

    if let Some(ref manifest) = manifest_opt {
        log::debug!(
            "✅ Manifest cache HIT | table={}.{} | {} | batches={}",
            namespace.as_str(),
            table.as_str(),
            scope_label,
            manifest.batches.len()
        );
    }

    fn extract_seq_bounds(expr: &Expr) -> (Option<i64>, Option<i64>) {
        use datafusion::logical_expr::Expr::Column;
        use datafusion::scalar::ScalarValue;

        fn lit_to_i64(e: &Expr) -> Option<i64> {
            if let Expr::Literal(lit, _) = e {
                match lit {
                    ScalarValue::Int64(Some(v)) => Some(*v),
                    ScalarValue::Int32(Some(v)) => Some(*v as i64),
                    ScalarValue::UInt64(Some(v)) => Some(*v as i64),
                    ScalarValue::UInt32(Some(v)) => Some(*v as i64),
                    _ => None,
                }
            } else {
                None
            }
        }

        match expr {
            Expr::BinaryExpr(be) => {
                let left = &be.left;
                let right = &be.right;
                let op = &be.op;

                if *op == Operator::And {
                    let (a_min, a_max) = extract_seq_bounds(left);
                    let (b_min, b_max) = extract_seq_bounds(right);
                    let min = match (a_min, b_min) {
                        (Some(a), Some(b)) => Some(a.max(b)),
                        (Some(a), None) => Some(a),
                        (None, Some(b)) => Some(b),
                        _ => None,
                    };
                    let max = match (a_max, b_max) {
                        (Some(a), Some(b)) => Some(a.min(b)),
                        (Some(a), None) => Some(a),
                        (None, Some(b)) => Some(b),
                        _ => None,
                    };
                    return (min, max);
                }

                let is_seq_left = matches!(left.as_ref(), Column(c) if c.name.as_str() == "_seq");
                let is_seq_right = matches!(right.as_ref(), Column(c) if c.name.as_str() == "_seq");

                if is_seq_left {
                    if let Some(val) = lit_to_i64(right.as_ref()) {
                        return match op {
                            Operator::Gt | Operator::GtEq => (Some(val), None),
                            Operator::Lt | Operator::LtEq => (None, Some(val)),
                            Operator::Eq => (Some(val), Some(val)),
                            _ => (None, None),
                        };
                    }
                } else if is_seq_right {
                    if let Some(val) = lit_to_i64(left.as_ref()) {
                        return match op {
                            Operator::Lt | Operator::LtEq => (Some(val), None),
                            Operator::Gt | Operator::GtEq => (None, Some(val)),
                            Operator::Eq => (Some(val), Some(val)),
                            _ => (None, None),
                        };
                    }
                }
            }
            _ => {}
        }

        (None, None)
    }

    let planner = ManifestAccessPlanner::new();
    let (min_seq, max_seq) = filter
        .map(|f| extract_seq_bounds(f))
        .unwrap_or((None, None));
    let seq_range = match (min_seq, max_seq) {
        (Some(min), Some(max)) => Some((min, max)),
        _ => None,
    };

    let (combined, (total_batches, skipped, scanned)) = planner.scan_parquet_files(
        manifest_opt.as_ref(),
        &storage_dir,
        seq_range,
        use_degraded_mode,
        schema.clone(),
    )?;

    if total_batches > 0 {
        log::debug!(
            "[Manifest Pruning] table={}.{} {} batches_total={} skipped={} scanned={} rows={}",
            namespace.as_str(),
            table.as_str(),
            scope_label,
            total_batches,
            skipped,
            scanned,
            combined.num_rows()
        );
    }

    Ok(combined)
}

/// Return a shared `_system` user identifier for scope-agnostic operations
pub fn system_user_id() -> &'static UserId {
    &SYSTEM_USER_ID
}

/// Resolve user scope, defaulting to the shared system identifier for scope-less tables
pub fn resolve_user_scope(scope: Option<&UserId>) -> &UserId {
    scope.unwrap_or_else(|| system_user_id())
}

fn json_value_matches_pk(value: &JsonValue, target: &str) -> bool {
    match value {
        JsonValue::String(s) => s == target,
        JsonValue::Number(n) => n.to_string() == target,
        JsonValue::Bool(b) => b.to_string() == target,
        _ => false,
    }
}

/// Locate the latest non-deleted row matching the provided primary-key value
pub fn find_row_by_pk<P, K, V>(
    provider: &P,
    scope: Option<&UserId>,
    pk_value: &str,
) -> Result<Option<(K, V)>, KalamDbError>
where
    P: BaseTableProvider<K, V>,
    K: StorageKey,
{
    let user_scope = resolve_user_scope(scope);
    let resolved = provider.scan_with_version_resolution_to_kvs(user_scope, None)?;
    let pk_name = provider.primary_key_field_name();

    for (key, row) in resolved.into_iter() {
        if let Some(fields) = P::extract_fields(&row) {
            if let Some(val) = fields.get(pk_name) {
                if json_value_matches_pk(val, pk_value) {
                    return Ok(Some((key, row)));
                }
            }
        }
    }

    Ok(None)
}

/// Ensure an INSERT payload either auto-generates or provides a unique primary-key value
pub fn ensure_unique_pk_value<P, K, V>(
    provider: &P,
    scope: Option<&UserId>,
    row_data: &JsonValue,
) -> Result<(), KalamDbError>
where
    P: BaseTableProvider<K, V>,
    K: StorageKey,
{
    let pk_name = provider.primary_key_field_name();
    if let Some(pk_value) = row_data.get(pk_name) {
        if !pk_value.is_null() {
            let pk_str = unified_dml::extract_user_pk_value(row_data, pk_name)?;
            if find_row_by_pk(provider, scope, &pk_str)?.is_some() {
                return Err(KalamDbError::AlreadyExists(format!(
                    "Primary key violation: value '{}' already exists in column '{}'",
                    pk_str, pk_name
                )));
            }
        }
    }
    Ok(())
}
