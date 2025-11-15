//! Shared table provider implementation without RLS
//!
//! This module provides SharedTableProvider implementing BaseTableProvider<SharedTableRowId, SharedTableRow>
//! for cross-user shared tables (no Row-Level Security).
//!
//! **Key Features**:
//! - Direct fields (no wrapper layer)
//! - Shared core via Arc<TableProviderCore>
//! - No handlers - all DML logic inline
//! - NO RLS - ignores user_id parameter (operates on all rows)
//! - SessionState NOT extracted in scan_rows() (scans all rows)

use super::base::{BaseTableProvider, TableProviderCore};
use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::schema_registry::TableType;
use kalamdb_tables::{SharedTableRow, SharedTableStore};
use async_trait::async_trait;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::catalog::Session;
use datafusion::datasource::TableProvider;
use datafusion::datasource::memory::MemTable;
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::Expr;
use datafusion::parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_commons::ids::SharedTableRowId;
use kalamdb_commons::models::UserId;
use kalamdb_commons::TableId;
use kalamdb_store::entity_store::EntityStore;
use serde_json::Value as JsonValue;
use std::any::Any;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

// Arrow <-> JSON helpers
use crate::providers::arrow_json_conversion::json_rows_to_arrow_batch;
use serde_json::json;

/// Shared table provider without RLS
///
/// **Architecture**:
/// - Stateless provider (user context passed but ignored)
/// - Direct fields (no wrapper layer)
/// - Shared core via Arc<TableProviderCore>
/// - NO RLS - user_id parameter ignored in all operations
pub struct SharedTableProvider {
    /// Shared core (app_context, live_query_manager, storage_registry)
    core: Arc<TableProviderCore>,
    
    /// Table identifier
    table_id: Arc<TableId>,
    
    /// Logical table type
    table_type: TableType,
    
    /// SharedTableStore for DML operations
    store: Arc<SharedTableStore>,
    
    /// Cached primary key field name
    primary_key_field_name: String,
}

impl SharedTableProvider {
    /// Create a new shared table provider
    ///
    /// # Arguments
    /// * `core` - Shared core with app_context and optional services
    /// * `table_id` - Table identifier
    /// * `store` - SharedTableStore for this table
    /// * `primary_key_field_name` - Primary key field name from schema
    pub fn new(
        core: Arc<TableProviderCore>,
        table_id: TableId,
        store: Arc<SharedTableStore>,
        primary_key_field_name: String,
    ) -> Self {
        Self {
            core,
            table_id: Arc::new(table_id),
            table_type: TableType::Shared,
            store,
            primary_key_field_name,
        }
    }
    
    /// Scan Parquet files from cold storage for shared table
    ///
    /// Lists all *.parquet files in the table's storage directory and merges them into a single RecordBatch.
    /// Returns an empty batch if no Parquet files exist.
    ///
    /// **Difference from user tables**: Shared tables have NO user_id partitioning,
    /// so all Parquet files are in the same directory (no subdirectories per user).
    ///
    /// **Phase 4 (US6, T082-T084)**: Integrated with ManifestCacheService for manifest caching.
    /// Logs cache hits/misses and updates last_accessed timestamp. Full query optimization
    /// (batch file pruning based on manifest metadata) implemented in Phase 5 (US2, T119-T123).
    fn scan_parquet_files_as_batch(&self, filter: Option<&Expr>) -> Result<RecordBatch, KalamDbError> {
        use datafusion::logical_expr::{Expr, Operator};
        use kalamdb_commons::types::ManifestFile;

        // Phase 4 (T082): Integrate with ManifestCacheService
        // Try to load manifest from cache (hot cache → RocksDB → None)
        let namespace = self.table_id.namespace_id();
        let table = self.table_id.table_name();
        
        let manifest_cache_service = self.core.app_context.manifest_cache_service();
        let cache_result = manifest_cache_service.get_or_load(namespace, table, None);
        
        // T124-T127: Manifest recovery - validate and rebuild on corruption
        let mut manifest_opt: Option<ManifestFile> = None;
        let mut use_degraded_mode = false;
        
        match &cache_result {
            Ok(Some(entry)) => {
                match ManifestFile::from_json(&entry.manifest_json) {
                    Ok(manifest) => {
                        // T124: Validate manifest consistency
                        if let Err(e) = manifest.validate() {
                            log::warn!(
                                "⚠️  [MANIFEST CORRUPTION] table={}.{} scope=shared error={} | Triggering rebuild",
                                namespace.as_str(),
                                table.as_str(),
                                e
                            );
                            
                            // T125: Trigger rebuild on validation failure
                            // T126: Enable degraded mode during rebuild
                            use_degraded_mode = true;
                            
                            // Spawn rebuild in background (non-blocking)
                            let manifest_service = self.core.app_context.manifest_service();
                            let ns = namespace.clone();
                            let tbl = table.clone();
                            tokio::spawn(async move {
                                log::info!(
                                    "🔧 [MANIFEST REBUILD STARTED] table={}.{} scope=shared",
                                    ns.as_str(),
                                    tbl.as_str()
                                );
                                match manifest_service.rebuild_manifest(&ns, &tbl, None) {
                                    Ok(_) => {
                                        log::info!(
                                            "✅ [MANIFEST REBUILD COMPLETED] table={}.{} scope=shared",
                                            ns.as_str(),
                                            tbl.as_str()
                                        );
                                    }
                                    Err(e) => {
                                        log::error!(
                                            "❌ [MANIFEST REBUILD FAILED] table={}.{} scope=shared error={}",
                                            ns.as_str(),
                                            tbl.as_str(),
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
                            "⚠️  Failed to parse manifest JSON for table={}.{} scope=shared: {} | Using degraded mode",
                            namespace.as_str(),
                            table.as_str(),
                            e
                        );
                        use_degraded_mode = true;
                    }
                }
            }
            Ok(None) => {
                log::debug!(
                    "⚠️  Manifest cache MISS | table={}.{} | scope=shared | fallback=directory_scan",
                    namespace.as_str(),
                    table.as_str()
                );
                use_degraded_mode = true;
            }
            Err(e) => {
                log::warn!(
                    "⚠️  Manifest cache ERROR | table={}.{} | scope=shared | error={} | fallback=directory_scan",
                    namespace.as_str(),
                    table.as_str(),
                    e
                );
                use_degraded_mode = true;
            }
        }
        
        // Log cache hit after validation
        if let Some(ref manifest) = manifest_opt {
            log::debug!(
                "✅ Manifest cache HIT | table={}.{} | scope=shared | batches={}",
                namespace.as_str(),
                table.as_str(),
                manifest.batches.len()
            );
        }

        // Helper: extract conservative _seq bounds from filter for pruning
        fn extract_seq_bounds(expr: &Expr) -> (Option<i64>, Option<i64>) {
            use datafusion::logical_expr::Expr::Column;
            let mut min_seq: Option<i64> = None;
            let mut max_seq: Option<i64> = None;

            fn lit_to_i64(e: &Expr) -> Option<i64> {
                use datafusion::scalar::ScalarValue;
                if let Expr::Literal(l, _) = e {
                    match l {
                        ScalarValue::Int64(Some(v)) => Some(*v),
                        ScalarValue::Int32(Some(v)) => Some(*v as i64),
                        ScalarValue::UInt64(Some(v)) => Some(*v as i64),
                        ScalarValue::UInt32(Some(v)) => Some(*v as i64),
                        _ => None,
                    }
                } else { None }
            }

            match expr {
                Expr::BinaryExpr(be) => {
                    let left = &be.left;
                    let op = &be.op;
                    let right = &be.right;
                    if *op == Operator::And {
                        let (a_min, a_max) = extract_seq_bounds(left);
                        let (b_min, b_max) = extract_seq_bounds(right);
                        let min = match (a_min, b_min) { (Some(a), Some(b)) => Some(a.max(b)), (Some(a), None) => Some(a), (None, Some(b)) => Some(b), _ => None };
                        let max = match (a_max, b_max) { (Some(a), Some(b)) => Some(a.min(b)), (Some(a), None) => Some(a), (None, Some(b)) => Some(b), _ => None };
                        return (min, max);
                    } else {
                        let is_seq_left = matches!(left.as_ref(), Column(c) if c.name.as_str() == "_seq");
                        let is_seq_right = matches!(right.as_ref(), Column(c) if c.name.as_str() == "_seq");
                        if is_seq_left {
                            if let Some(val) = lit_to_i64(right.as_ref()) {
                                match op {
                                    Operator::Gt | Operator::GtEq => { return (Some(val), None); }
                                    Operator::Lt | Operator::LtEq => { return (None, Some(val)); }
                                    Operator::Eq => { return (Some(val), Some(val)); }
                                    _ => {}
                                }
                            }
                        } else if is_seq_right {
                            if let Some(val) = lit_to_i64(left.as_ref()) {
                                match op {
                                    Operator::Lt | Operator::LtEq => { return (Some(val), None); }
                                    Operator::Gt | Operator::GtEq => { return (None, Some(val)); }
                                    Operator::Eq => { return (Some(val), Some(val)); }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                _ => {}
            }

            (None, None)
        }
        
        // Resolve storage path for shared table (no user_id)
        let storage_path = self.core.app_context.schema_registry()
            .get_storage_path(&*self.table_id, None, None)?;
        
        let storage_dir = PathBuf::from(&storage_path);
        
        // If directory doesn't exist, return empty batch
        if !storage_dir.exists() {
            log::debug!("No Parquet directory found for shared table: {}", storage_path);
            let schema = self.schema_ref();
            return Ok(RecordBatch::new_empty(schema));
        }
        
        // Prefer manifest for file enumeration + pruning; fallback to directory scan
        let mut parquet_files: Vec<PathBuf> = Vec::new();
        let (mut total_batches, mut skipped, mut scanned) = (0usize, 0usize, 0usize);
        
        if !use_degraded_mode {
            if let Some(manifest) = manifest_opt {
                total_batches = manifest.batches.len();
                let (min_seq, max_seq) = filter.map(|f| extract_seq_bounds(f)).unwrap_or((None, None));

                for b in manifest.batches.iter() {
                    let mut include = true;
                    if let (Some(minv), Some(maxv)) = (min_seq, max_seq) {
                        include = b.overlaps_seq_range(minv, maxv);
                    } else if let (Some(minv), None) = (min_seq, max_seq) {
                        include = b.max_seq >= minv;
                    } else if let (None, Some(maxv)) = (min_seq, max_seq) {
                        include = b.min_seq <= maxv;
                    }

                    if include {
                        let p = storage_dir.join(&b.file_path);
                        parquet_files.push(p);
                        scanned += 1;
                    } else {
                        skipped += 1;
                    }
                }

                log::debug!(
                    "[Manifest Pruning] table={}.{} scope=shared batches_total={} skipped={} scanned={}",
                    namespace.as_str(),
                    table.as_str(),
                    total_batches,
                    skipped,
                    scanned
                );
            }
        }

        if parquet_files.is_empty() {
            // List all .parquet files in directory
            let entries = fs::read_dir(&storage_dir)
                .map_err(|e| KalamDbError::Io(e))?;
            
            for entry in entries {
                let entry = entry.map_err(|e| KalamDbError::Io(e))?;
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("parquet") {
                    parquet_files.push(path);
                }
            }
        }
        
        if parquet_files.is_empty() {
            log::debug!("No Parquet files found for shared table in {}", storage_path);
            let schema = self.schema_ref();
            return Ok(RecordBatch::new_empty(schema));
        }
        
        let parquet_count = parquet_files.len();
        log::debug!("Found {} Parquet files for shared table in {}", parquet_count, storage_path);
        
        // Read all Parquet files and merge batches
        let mut all_batches = Vec::new();
        let schema = self.schema_ref();
        
        for parquet_file in parquet_files {
            log::debug!("Reading Parquet file: {}", parquet_file.display());
            
            let file = fs::File::open(&parquet_file)
                .map_err(|e| KalamDbError::Io(e))?;
            
            let builder = ParquetRecordBatchReaderBuilder::try_new(file)
                .map_err(|e| KalamDbError::Other(format!("Failed to create Parquet reader: {}", e)))?;
            
            let mut reader = builder.build()
                .map_err(|e| KalamDbError::Other(format!("Failed to build Parquet reader: {}", e)))?;
            
            // Read all batches from this file
            while let Some(batch_result) = reader.next() {
                let batch = batch_result
                    .map_err(|e| KalamDbError::Other(format!("Failed to read Parquet batch: {}", e)))?;
                all_batches.push(batch);
            }
        }
        
        if all_batches.is_empty() {
            log::debug!("All Parquet files were empty for shared table");
            return Ok(RecordBatch::new_empty(schema));
        }
        
        // Concatenate all batches
        let combined = datafusion::arrow::compute::concat_batches(&schema, &all_batches)
            .map_err(|e| KalamDbError::Other(format!("Failed to concatenate Parquet batches: {}", e)))?;
        
        log::debug!("Loaded {} total rows from {} Parquet files for shared table", 
                   combined.num_rows(), parquet_count);
        
        Ok(combined)
    }
}

impl SharedTableProvider {
    /// Check if a primary key value already exists (non-deleted version)
    ///
    /// **Performance**: O(log n) lookup by PK value using version resolution.
    ///
    /// # Arguments
    /// * `pk_value` - String representation of PK value to check
    ///
    /// # Returns
    /// * `Ok(true)` - PK value exists with non-deleted version
    /// * `Ok(false)` - PK value does not exist or only deleted versions exist
    /// * `Err` - Storage error
    fn pk_value_exists(&self, pk_value: &str) -> Result<bool, KalamDbError> {
        let pk_name = self.primary_key_field_name();
        
        // Shared tables don't have user scoping, so we use a dummy user_id
        let dummy_user_id = UserId::from("_system");
        
        // Scan with version resolution (returns only latest non-deleted versions)
        let resolved = self.scan_with_version_resolution_to_kvs(&dummy_user_id, None)?;
        
        // Check if any resolved row has this PK value
        for (_key, row) in resolved.into_iter() {
            if let Some(val) = row.fields.get(pk_name) {
                // Compare as strings to handle different JSON types
                let row_pk_str = match val {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    _ => continue,
                };
                
                if row_pk_str == pk_value {
                    return Ok(true); // PK value already exists
                }
            }
        }
        
        Ok(false) // PK value does not exist
    }
}

impl BaseTableProvider<SharedTableRowId, SharedTableRow> for SharedTableProvider {
    fn table_id(&self) -> &TableId {
        &self.table_id
    }
    
    fn schema_ref(&self) -> SchemaRef {
        // Get memoized Arrow schema from SchemaRegistry via AppContext
        self.core.app_context
            .schema_registry()
            .get_arrow_schema(&self.table_id)
            .expect("Failed to get Arrow schema from registry")
    }
    
    fn provider_table_type(&self) -> TableType { self.table_type }
    
    fn app_context(&self) -> &Arc<AppContext> {
        &self.core.app_context
    }
    
    fn primary_key_field_name(&self) -> &str {
        &self.primary_key_field_name
    }
    
    fn insert(&self, _user_id: &UserId, row_data: JsonValue) -> Result<SharedTableRowId, KalamDbError> {
        // IGNORE user_id parameter - no RLS for shared tables
        
        // T060: Validate PRIMARY KEY uniqueness if user provided PK value
        let pk_name = self.primary_key_field_name();
        if let Some(pk_value) = row_data.get(pk_name) {
            // User provided PK value - check if it already exists (non-deleted version)
            if !pk_value.is_null() {
                let pk_str = crate::providers::unified_dml::extract_user_pk_value(&row_data, pk_name)?;
                
                // Check if this PK value already exists with a non-deleted version
                if self.pk_value_exists(&pk_str)? {
                    return Err(KalamDbError::AlreadyExists(format!(
                        "Primary key violation: value '{}' already exists in column '{}'",
                        pk_str, pk_name
                    )));
                }
            }
        }
        // If PK not provided, it must have a DEFAULT value (validated at CREATE TABLE time)
        
        // Generate new SeqId via SystemColumnsService
        let sys_cols = self.core.system_columns.clone();
        let seq_id = sys_cols.generate_seq_id()?;
        
        // Create SharedTableRow directly
        let entity = SharedTableRow {
            _seq: seq_id,
            _deleted: false,
            fields: row_data,
        };
        
        // Key is just the SeqId (SharedTableRowId is type alias for SeqId)
        let row_key = seq_id;
        
        // Store the entity in RocksDB (hot storage)
        self.store.put(&row_key, &entity).map_err(|e| {
            KalamDbError::InvalidOperation(format!(
                "Failed to insert shared table row: {}",
                e
            ))
        })?;
        
        log::debug!(
            "Inserted shared table row with _seq {}",
            seq_id
        );
        
        Ok(row_key)
    }
    
    fn update(&self, _user_id: &UserId, key: &SharedTableRowId, updates: JsonValue) -> Result<SharedTableRowId, KalamDbError> {
        // IGNORE user_id parameter - no RLS for shared tables
        // Merge updates onto latest resolved row by PK
        let update_obj = updates.as_object().cloned().ok_or_else(|| {
            KalamDbError::InvalidSql("UPDATE requires object of column assignments".to_string())
        })?;

        let pk_name = self.primary_key_field_name().to_string();
        // Load referenced prior version to derive PK value if not present in updates
        let prior = EntityStore::get(&*self.store, key)
            .map_err(|e| KalamDbError::Other(format!("Failed to load prior version: {}", e)))?
            .ok_or_else(|| KalamDbError::NotFound("Row not found for update".to_string()))?;
        let pk_value = crate::providers::unified_dml::extract_user_pk_value(&prior.fields, &pk_name)?;
        // Resolve latest per PK
        let resolved = self.scan_with_version_resolution_to_kvs(&_user_id.clone(), None)?;

        let mut latest_opt: Option<(SharedTableRowId, SharedTableRow)> = None;
        for (k, r) in resolved.into_iter() {
            if let Some(val) = r.fields.get(&pk_name) {
                if val.to_string() == pk_value {
                    latest_opt = Some((k, r));
                    break;
                }
            }
        }
        let (_latest_key, latest_row) = latest_opt
            .ok_or_else(|| KalamDbError::NotFound(format!("Row with {}={} not found", pk_name, pk_value)))?;

        let mut merged = latest_row.fields.as_object().cloned().unwrap_or_default();
        for (k, v) in update_obj.into_iter() { merged.insert(k, v); }
        let new_fields = JsonValue::Object(merged);

        let sys_cols = self.core.system_columns.clone();
        let seq_id = sys_cols.generate_seq_id()?;
        let entity = SharedTableRow { _seq: seq_id, _deleted: false, fields: new_fields };
        let row_key = seq_id;
        self.store.put(&row_key, &entity).map_err(|e| {
            KalamDbError::InvalidOperation(format!("Failed to update shared table row: {}", e))
        })?;
        Ok(row_key)
    }
    
    fn delete(&self, _user_id: &UserId, key: &SharedTableRowId) -> Result<(), KalamDbError> {
        // IGNORE user_id parameter - no RLS for shared tables
        // Load referenced version to extract PK so tombstone groups with same logical row
        let prior = EntityStore::get(&*self.store, key)
            .map_err(|e| KalamDbError::Other(format!("Failed to load prior version: {}", e)))?
            .ok_or_else(|| KalamDbError::NotFound("Row not found for delete".to_string()))?;

        let pk_name = self.primary_key_field_name().to_string();
        // Preserve existing PK value if present; may be null if malformed
        let pk_json_val = prior
            .fields
            .get(&pk_name)
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        let sys_cols = self.core.system_columns.clone();
        let seq_id = sys_cols.generate_seq_id()?;
        // Include PK in tombstone fields so version resolution collapses correctly
        let entity = SharedTableRow {
            _seq: seq_id,
            _deleted: true,
            fields: serde_json::json!({ pk_name: pk_json_val }),
        };
        let row_key = seq_id;
        self.store.put(&row_key, &entity).map_err(|e| {
            KalamDbError::InvalidOperation(format!("Failed to delete shared table row: {}", e))
        })?;
        Ok(())
    }
    
    fn scan_rows(&self, _state: &dyn Session, filter: Option<&Expr>) -> Result<RecordBatch, KalamDbError> {
        // NO user_id extraction - shared tables scan ALL rows
        let kvs = self.scan_with_version_resolution_to_kvs(&UserId::new("_ignored"), filter)?;

        // Convert to JSON rows aligned with schema
        let schema = self.schema_ref();
        let mut rows: Vec<JsonValue> = Vec::with_capacity(kvs.len());
        let has_seq = schema.field_with_name("_seq").is_ok();
        let has_deleted = schema.field_with_name("_deleted").is_ok();

        for (_key, row) in kvs.into_iter() {
            let mut obj = row
                .fields
                .as_object()
                .cloned()
                .unwrap_or_default();
            if has_seq { obj.insert("_seq".to_string(), json!(row._seq.as_i64())); }
            if has_deleted { obj.insert("_deleted".to_string(), json!(row._deleted)); }
            rows.push(JsonValue::Object(obj));
        }

        let batch = json_rows_to_arrow_batch(&schema, rows)
            .map_err(|e| KalamDbError::InvalidOperation(format!("Failed to build Arrow batch: {}", e)))?;
        Ok(batch)
    }
    
    fn scan_with_version_resolution_to_kvs(
        &self,
    _user_id: &UserId,
    _filter: Option<&Expr>,
    ) -> Result<Vec<(SharedTableRowId, SharedTableRow)>, KalamDbError> {
        // IGNORE user_id parameter - scan ALL rows (hot storage)
        let raw = self
            .store
            .scan_all()
            .map_err(|e| KalamDbError::InvalidOperation(format!(
                "Failed to scan shared table hot storage: {}",
                e
            )))?;
        log::debug!(
            "[SharedProvider] RocksDB scan returned {} rows",
            raw.len()
        );

        // Scan cold storage (Parquet files)
        let parquet_batch = self.scan_parquet_files_as_batch(_filter)?;

        // Version resolution: MAX(_seq) per PK; honor tombstones
        use std::collections::HashMap;
        let pk_name = self.primary_key_field_name().to_string();
        let mut best: HashMap<String, (SharedTableRowId, SharedTableRow)> = HashMap::new();

        // Process RocksDB rows
        for (key_bytes, row) in raw.into_iter() {
            // Parse key (SeqId) from bytes
            let seq = match kalamdb_commons::ids::SeqId::from_bytes(&key_bytes) {
                Ok(s) => s,
                Err(err) => {
                    log::warn!("Skipping invalid SeqId key bytes: {}", err);
                    continue;
                }
            };
            let key: SharedTableRowId = seq;

            // Determine grouping key: prefer declared PK when present and non-null; else fall back to unique _seq
            let pk_key = match row.fields.get(&pk_name).filter(|v| !v.is_null()) {
                Some(v) => v.to_string(),
                None => format!("_seq:{}", row._seq.as_i64()),
            };

            match best.get(&pk_key) {
                Some((_existing_key, existing_row)) => {
                    if row._seq > existing_row._seq {
                        best.insert(pk_key, (key, row));
                    }
                }
                None => {
                    best.insert(pk_key, (key, row));
                }
            }
        }

        // Process Parquet rows (cold storage)
        log::debug!(
            "[SharedProvider] Processing {} Parquet rows for version resolution",
            parquet_batch.num_rows()
        );
        
        if parquet_batch.num_rows() > 0 {
            use crate::providers::arrow_json_conversion::arrow_value_to_json;
            use datafusion::arrow::array::{Array, BooleanArray, Int64Array};
            use serde_json::Map;
            
            // Extract column arrays
            let schema = parquet_batch.schema();
            
            // Find _seq column index
            let seq_idx = schema.fields().iter().position(|f| f.name() == "_seq")
                .ok_or_else(|| KalamDbError::Other("Missing _seq column in Parquet batch".to_string()))?;
            
            // Find _deleted column index (optional)
            let deleted_idx = schema.fields().iter().position(|f| f.name() == "_deleted");
            
            // Get _seq array
            let seq_array = parquet_batch.column(seq_idx)
                .as_any()
                .downcast_ref::<Int64Array>()
                .ok_or_else(|| KalamDbError::Other("_seq column is not Int64Array".to_string()))?;
            
            // Get _deleted array if exists
            let deleted_array = deleted_idx.and_then(|idx| {
                parquet_batch.column(idx)
                    .as_any()
                    .downcast_ref::<BooleanArray>()
            });
            
            // Convert each row to SharedTableRow
            for row_idx in 0..parquet_batch.num_rows() {
                let seq_val = seq_array.value(row_idx);
                let seq_id = kalamdb_commons::ids::SeqId::from_i64(seq_val);
                
                let deleted = deleted_array
                    .and_then(|arr| if !arr.is_null(row_idx) { Some(arr.value(row_idx)) } else { None })
                    .unwrap_or(false);
                
                // Build JSON object from all columns
                let mut json_row = Map::new();
                for (col_idx, field) in schema.fields().iter().enumerate() {
                    let col_name = field.name();
                    // Skip system columns (_seq, _deleted) - they're stored separately
                    if col_name == "_seq" || col_name == "_deleted" {
                        continue;
                    }
                    
                    let array = parquet_batch.column(col_idx);
                    match arrow_value_to_json(array.as_ref(), row_idx) {
                        Ok(val) => {
                            json_row.insert(col_name.clone(), val);
                        }
                        Err(e) => {
                            log::warn!("Failed to convert column {} for row {}: {}", col_name, row_idx, e);
                        }
                    }
                }
                
                // Determine grouping key: declared PK value (non-null) or fallback to unique _seq
                let pk_key = match json_row.get(&pk_name).filter(|v| !v.is_null()) {
                    Some(v) => v.to_string(),
                    None => format!("_seq:{}", seq_val),
                };
                
                // Create SharedTableRow
                let row = SharedTableRow {
                    _seq: seq_id,
                    _deleted: deleted,
                    fields: JsonValue::Object(json_row),
                };
                
                let key: SharedTableRowId = seq_id;
                
                // Merge with existing data using MAX(_seq)
                match best.get(&pk_key) {
                    Some((_existing_key, existing_row)) => {
                        if row._seq > existing_row._seq {
                            best.insert(pk_key, (key, row));
                        }
                    }
                    None => {
                        best.insert(pk_key, (key, row));
                    }
                }
            }
        }

        let result: Vec<(SharedTableRowId, SharedTableRow)> = best
            .into_values()
            .filter(|(_k, r)| !r._deleted)
            .collect();
        log::debug!(
            "[SharedProvider] Version-resolved rows (post-tombstone filter): {}",
            result.len()
        );
        Ok(result)
    }
    
    fn extract_fields(row: &SharedTableRow) -> Option<&JsonValue> {
        Some(&row.fields)
    }
}

// Manual Debug to satisfy DataFusion's TableProvider: Debug bound
impl std::fmt::Debug for SharedTableProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedTableProvider")
            .field("table_id", &self.table_id)
            .field("table_type", &self.table_type)
            .field("primary_key_field_name", &self.primary_key_field_name)
            .finish()
    }
}

// Implement DataFusion TableProvider trait
#[async_trait]
impl TableProvider for SharedTableProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }
    
    fn schema(&self) -> SchemaRef {
        self.schema_ref()
    }
    
    fn table_type(&self) -> datafusion::logical_expr::TableType {
        datafusion::logical_expr::TableType::Base
    }
    
    async fn scan(
        &self,
        state: &dyn Session,
        projection: Option<&Vec<usize>>,
        filters: &[Expr],
        limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        // Combine filters (AND) for pruning and pass to scan_rows
        let combined_filter: Option<Expr> = if filters.is_empty() {
            None
        } else {
            let first = filters[0].clone();
            Some(filters[1..].iter().cloned().fold(first, |acc, e| acc.and(e)))
        };

        let batch = self
            .scan_rows(state, combined_filter.as_ref())
            .map_err(|e| DataFusionError::Execution(format!("scan_rows failed: {}", e)))?;

        let mem = MemTable::try_new(self.schema_ref(), vec![vec![batch]])?;
        mem.scan(state, projection, filters, limit).await
    }
}
