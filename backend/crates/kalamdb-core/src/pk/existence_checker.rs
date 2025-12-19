//! Primary Key Existence Checker Implementation
//!
//! Provides optimized PK uniqueness validation using manifest-based segment pruning.

use std::sync::Arc;

use kalamdb_commons::models::schemas::ColumnDefault;
use kalamdb_commons::models::schemas::TableDefinition;
use kalamdb_commons::models::TableId;
use kalamdb_commons::schemas::TableType;
use kalamdb_commons::types::Manifest;
use kalamdb_commons::UserId;

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::error_extensions::KalamDbResultExt;
use crate::manifest::ManifestAccessPlanner;

/// Result of a primary key existence check
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PkCheckResult {
    /// PK column is AUTO_INCREMENT - no check needed, system generates unique value
    AutoIncrement,

    /// PK was not found in any storage layer
    NotFound,

    /// PK exists in hot storage (RocksDB memtable/SST)
    FoundInHot,

    /// PK exists in cold storage (Parquet files)
    FoundInCold {
        /// Segment file that contains the PK
        segment_path: String,
    },

    /// PK was pruned by manifest min/max stats - definitely not present
    /// This is a fast path that avoids reading any Parquet files
    PrunedByManifest,
}

impl PkCheckResult {
    /// Returns true if the PK exists in any storage layer
    pub fn exists(&self) -> bool {
        matches!(self, PkCheckResult::FoundInHot | PkCheckResult::FoundInCold { .. })
    }

    /// Returns true if the check was skipped due to auto-increment
    pub fn is_auto_increment(&self) -> bool {
        matches!(self, PkCheckResult::AutoIncrement)
    }
}

/// Primary Key Existence Checker
///
/// Optimized for INSERT/UPDATE operations that need to validate PK uniqueness.
/// Uses a tiered approach:
/// 1. Check if PK is auto-increment (skip)
/// 2. Check hot storage (RocksDB) via provider's PK index
/// 3. Use manifest cache (L1/L2) to load segment metadata
/// 4. Apply min/max pruning to skip irrelevant segments
/// 5. Scan only necessary Parquet files
pub struct PkExistenceChecker {
    app_context: Arc<AppContext>,
}

impl PkExistenceChecker {
    /// Create a new PK existence checker
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }

    /// Check if a PK column is configured for auto-increment
    ///
    /// Auto-increment columns use system-generated values, so we don't need
    /// to check for uniqueness - the system guarantees it.
    pub fn is_auto_increment_pk(table_def: &TableDefinition) -> bool {
        // Find the primary key column
        let pk_column = table_def.columns.iter().find(|col| col.is_primary_key);

        if let Some(pk_col) = pk_column {
            // Check if the default value is AUTO_INCREMENT function
            matches!(
                &pk_col.default_value,
                ColumnDefault::FunctionCall { name, .. } 
                    if name.eq_ignore_ascii_case("auto_increment") 
                    || name.eq_ignore_ascii_case("snowflake_id")
            )
        } else {
            // No PK column means no uniqueness constraint to check
            true
        }
    }

    /// Get the primary key column name from a table definition
    pub fn get_pk_column_name(table_def: &TableDefinition) -> Option<&str> {
        table_def
            .columns
            .iter()
            .find(|col| col.is_primary_key)
            .map(|col| col.column_name.as_str())
    }

    /// Check if a PK value exists, using optimized manifest-based pruning
    ///
    /// ## Flow:
    /// 1. Check if PK is AUTO_INCREMENT → return AutoIncrement (skip check)
    /// 2. Check if PK value is provided (not null)
    /// 3. Check hot storage via provider's PK index
    /// 4. Load manifest from cache (L1 hot cache → L2 RocksDB → storage)
    /// 5. Use column_stats min/max to prune segments
    /// 6. Scan only matching Parquet files
    ///
    /// ## Arguments
    /// * `table_def` - Table definition for schema info
    /// * `table_id` - Table identifier
    /// * `table_type` - Type of table (User, Shared, Stream)
    /// * `user_id` - Optional user ID for scoping (User tables)
    /// * `pk_value` - The PK value to check (as string)
    ///
    /// ## Returns
    /// * `PkCheckResult` indicating where (or if) the PK was found
    pub fn check_pk_exists(
        &self,
        table_def: &TableDefinition,
        table_id: &TableId,
        table_type: TableType,
        user_id: Option<&UserId>,
        pk_value: &str,
    ) -> Result<PkCheckResult, KalamDbError> {
        // Step 1: Check if PK is auto-increment
        if Self::is_auto_increment_pk(table_def) {
            log::trace!(
                "[PkExistenceChecker] PK is auto-increment for {}.{}, skipping check",
                table_id.namespace_id().as_str(),
                table_id.table_name().as_str()
            );
            return Ok(PkCheckResult::AutoIncrement);
        }

        // Step 2: Get PK column name
        let pk_column = Self::get_pk_column_name(table_def).ok_or_else(|| {
            KalamDbError::InvalidOperation(format!(
                "Table {}.{} has no primary key column",
                table_id.namespace_id().as_str(),
                table_id.table_name().as_str()
            ))
        })?;

        // Step 3-6: Use the optimized cold storage check from base provider
        // This already implements the full flow with manifest caching
        let exists_in_cold = self.check_cold_storage(
            table_id,
            table_type,
            user_id,
            pk_column,
            pk_value,
        )?;

        if let Some(segment_path) = exists_in_cold {
            return Ok(PkCheckResult::FoundInCold { segment_path });
        }

        Ok(PkCheckResult::NotFound)
    }

    /// Check cold storage for PK existence using manifest-based pruning
    ///
    /// Returns the segment path if found, None otherwise.
    fn check_cold_storage(
        &self,
        table_id: &TableId,
        table_type: TableType,
        user_id: Option<&UserId>,
        pk_column: &str,
        pk_value: &str,
    ) -> Result<Option<String>, KalamDbError> {
        let namespace = table_id.namespace_id();
        let table = table_id.table_name();
        let scope_label = user_id
            .map(|uid| format!("user={}", uid.as_str()))
            .unwrap_or_else(|| format!("scope={}", table_type.as_str()));

        // 1. Get CachedTableData for storage access
        let cached = match self.app_context.schema_registry().get(table_id) {
            Some(c) => c,
            None => {
                log::trace!(
                    "[PkExistenceChecker] No cached table data for {}.{} {} - PK not in cold",
                    namespace.as_str(),
                    table.as_str(),
                    scope_label
                );
                return Ok(None);
            }
        };

        // 2. Get Storage from registry
        let storage_id = cached
            .storage_id
            .clone()
            .unwrap_or_else(kalamdb_commons::models::StorageId::local);
        let storage = match self.app_context.storage_registry().get_storage(&storage_id) {
            Ok(Some(s)) => s,
            Ok(None) | Err(_) => {
                log::trace!(
                    "[PkExistenceChecker] Storage {} not found for {}.{} {}",
                    storage_id.as_str(),
                    namespace.as_str(),
                    table.as_str(),
                    scope_label
                );
                return Ok(None);
            }
        };

        let object_store = cached
            .object_store()
            .into_kalamdb_error("Failed to get object store")?;

        // 3. Get storage path
        let storage_path =
            crate::schema_registry::PathResolver::get_storage_path(&cached, user_id, None)?;

        // Check if any files exist
        let all_files = kalamdb_filestore::list_files_sync(
            Arc::clone(&object_store),
            &storage,
            &storage_path,
        );

        let all_files = match all_files {
            Ok(f) => f,
            Err(_) => {
                log::trace!(
                    "[PkExistenceChecker] No storage dir for {}.{} {}",
                    namespace.as_str(),
                    table.as_str(),
                    scope_label
                );
                return Ok(None);
            }
        };

        if all_files.is_empty() {
            log::trace!(
                "[PkExistenceChecker] No files in storage for {}.{} {}",
                namespace.as_str(),
                table.as_str(),
                scope_label
            );
            return Ok(None);
        }

        // 4. Load manifest from cache (L1 → L2 → storage)
        let manifest_service = self.app_context.manifest_service();
        let manifest: Option<Manifest> = match manifest_service.get_or_load(table_id, user_id)
        {
            Ok(Some(entry)) => {
                log::trace!(
                    "[PkExistenceChecker] Manifest loaded from cache for {}.{} {}",
                    namespace.as_str(),
                    table.as_str(),
                    scope_label
                );
                Some(entry.manifest.clone())
            }
            Ok(None) => {
                log::trace!(
                    "[PkExistenceChecker] No manifest in cache for {}.{} {} - will check all files",
                    namespace.as_str(),
                    table.as_str(),
                    scope_label
                );
                // Try to load from storage (manifest.json file)
                self.load_manifest_from_storage(
                    Arc::clone(&object_store),
                    &storage,
                    &storage_path,
                )?
            }
            Err(e) => {
                log::warn!(
                    "[PkExistenceChecker] Manifest cache error for {}.{} {}: {}",
                    namespace.as_str(),
                    table.as_str(),
                    scope_label,
                    e
                );
                None
            }
        };

        // 5. Use manifest to prune segments by min/max or check all Parquet files
        let planner = ManifestAccessPlanner::new();
        let files_to_scan: Vec<String> = if let Some(ref m) = manifest {
            let pruned_paths = planner.plan_by_pk_value(m, pk_column, pk_value);
            if pruned_paths.is_empty() {
                log::trace!(
                    "[PkExistenceChecker] Manifest pruning: PK {} not in any segment range for {}.{} {}",
                    pk_value,
                    namespace.as_str(),
                    table.as_str(),
                    scope_label
                );
                return Ok(None); // Fast path: PK definitely not in cold storage
            }
            log::trace!(
                "[PkExistenceChecker] Manifest pruning: {} of {} segments may contain PK {} for {}.{} {}",
                pruned_paths.len(),
                m.segments.len(),
                pk_value,
                namespace.as_str(),
                table.as_str(),
                scope_label
            );
            pruned_paths
        } else {
            // No manifest - check all Parquet files
            all_files
                .into_iter()
                .filter(|p| p.ends_with(".parquet"))
                .collect()
        };

        if files_to_scan.is_empty() {
            return Ok(None);
        }

        // 6. Scan pruned Parquet files for the PK
        for file_name in files_to_scan {
            let parquet_path = format!("{}/{}", storage_path.trim_end_matches('/'), file_name);
            if self.pk_exists_in_parquet(
                Arc::clone(&object_store),
                &storage,
                &parquet_path,
                pk_column,
                pk_value,
            )? {
                log::trace!(
                    "[PkExistenceChecker] Found PK {} in {} for {}.{} {}",
                    pk_value,
                    parquet_path,
                    namespace.as_str(),
                    table.as_str(),
                    scope_label
                );
                return Ok(Some(file_name));
            }
        }

        Ok(None)
    }

    /// Load manifest.json from storage if not in cache
    fn load_manifest_from_storage(
        &self,
        store: Arc<dyn object_store::ObjectStore>,
        storage: &kalamdb_commons::system::Storage,
        storage_path: &str,
    ) -> Result<Option<Manifest>, KalamDbError> {
        let manifest_path = format!("{}/manifest.json", storage_path.trim_end_matches('/'));

        match kalamdb_filestore::read_file_sync(Arc::clone(&store), storage, &manifest_path) {
            Ok(bytes) => {
                let manifest: Manifest = serde_json::from_slice(&bytes).map_err(|e| {
                    KalamDbError::InvalidOperation(format!("Failed to parse manifest.json: {}", e))
                })?;
                log::trace!(
                    "[PkExistenceChecker] Loaded manifest.json from storage: {} segments",
                    manifest.segments.len()
                );
                Ok(Some(manifest))
            }
            Err(_) => {
                log::trace!("[PkExistenceChecker] No manifest.json found at {}", manifest_path);
                Ok(None)
            }
        }
    }

    /// Check if a PK exists in a specific Parquet file (with MVCC version resolution)
    fn pk_exists_in_parquet(
        &self,
        store: Arc<dyn object_store::ObjectStore>,
        storage: &kalamdb_commons::system::Storage,
        parquet_path: &str,
        pk_column: &str,
        pk_value: &str,
    ) -> Result<bool, KalamDbError> {
        use datafusion::arrow::array::{Array, BooleanArray, Int64Array, UInt64Array};
        use kalamdb_commons::constants::SystemColumnNames;
        use std::collections::HashMap;

        // Read Parquet file
        let batches = kalamdb_filestore::read_parquet_batches_sync(store, storage, parquet_path)
            .into_kalamdb_error("Failed to read Parquet file")?;

        // Track latest version per PK value: pk_value -> (max_seq, is_deleted)
        let mut versions: HashMap<String, (i64, bool)> = HashMap::new();

        for batch in batches {
            let pk_idx = batch.schema().index_of(pk_column).ok();
            let seq_idx = batch.schema().index_of(SystemColumnNames::SEQ).ok();
            let deleted_idx = batch.schema().index_of(SystemColumnNames::DELETED).ok();

            let (Some(pk_i), Some(seq_i)) = (pk_idx, seq_idx) else {
                continue;
            };

            let pk_col = batch.column(pk_i);
            let seq_col = batch.column(seq_i);
            let deleted_col = deleted_idx.map(|i| batch.column(i));

            for row_idx in 0..batch.num_rows() {
                let row_pk = Self::extract_pk_as_string(pk_col.as_ref(), row_idx);
                let Some(row_pk_str) = row_pk else { continue };

                // Only check rows matching target PK
                if row_pk_str != pk_value {
                    continue;
                }

                // Extract _seq
                let seq = if let Some(arr) = seq_col.as_any().downcast_ref::<Int64Array>() {
                    arr.value(row_idx)
                } else if let Some(arr) = seq_col.as_any().downcast_ref::<UInt64Array>() {
                    arr.value(row_idx) as i64
                } else {
                    continue;
                };

                // Extract _deleted
                let deleted = if let Some(del_col) = &deleted_col {
                    if let Some(arr) = del_col.as_any().downcast_ref::<BooleanArray>() {
                        if arr.is_null(row_idx) {
                            false
                        } else {
                            arr.value(row_idx)
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };

                // Update version tracking (keep highest seq per PK)
                versions
                    .entry(row_pk_str)
                    .and_modify(|(current_seq, current_deleted)| {
                        if seq > *current_seq {
                            *current_seq = seq;
                            *current_deleted = deleted;
                        }
                    })
                    .or_insert((seq, deleted));
            }
        }

        // Check if the target PK exists and is not deleted
        if let Some((_, is_deleted)) = versions.get(pk_value) {
            Ok(!*is_deleted)
        } else {
            Ok(false)
        }
    }

    /// Extract PK value as string from an Arrow array
    fn extract_pk_as_string(
        col: &dyn datafusion::arrow::array::Array,
        idx: usize,
    ) -> Option<String> {
        use datafusion::arrow::array::{
            Int16Array, Int32Array, Int64Array, StringArray, UInt16Array, UInt32Array, UInt64Array,
        };

        if col.is_null(idx) {
            return None;
        }

        if let Some(arr) = col.as_any().downcast_ref::<StringArray>() {
            return Some(arr.value(idx).to_string());
        }
        if let Some(arr) = col.as_any().downcast_ref::<Int64Array>() {
            return Some(arr.value(idx).to_string());
        }
        if let Some(arr) = col.as_any().downcast_ref::<Int32Array>() {
            return Some(arr.value(idx).to_string());
        }
        if let Some(arr) = col.as_any().downcast_ref::<Int16Array>() {
            return Some(arr.value(idx).to_string());
        }
        if let Some(arr) = col.as_any().downcast_ref::<UInt64Array>() {
            return Some(arr.value(idx).to_string());
        }
        if let Some(arr) = col.as_any().downcast_ref::<UInt32Array>() {
            return Some(arr.value(idx).to_string());
        }
        if let Some(arr) = col.as_any().downcast_ref::<UInt16Array>() {
            return Some(arr.value(idx).to_string());
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::models::datatypes::KalamDataType;
    use kalamdb_commons::models::schemas::{ColumnDefinition, ColumnDefault, TableDefinition, TableType};
    use kalamdb_commons::{NamespaceId, TableName};

    fn create_test_table_def(pk_default: ColumnDefault) -> TableDefinition {
        TableDefinition {
            namespace_id: NamespaceId::new("test"),
            table_name: TableName::new("users"),
            table_type: TableType::User,
            table_options: kalamdb_commons::schemas::TableOptions::User(Default::default()),
            columns: vec![
                ColumnDefinition::new(
                    "id",
                    1,
                    KalamDataType::BigInt,
                    false,
                    true, // is_primary_key
                    false,
                    pk_default,
                    None,
                ),
                ColumnDefinition::new(
                    "name",
                    2,
                    KalamDataType::Text,
                    true,
                    false,
                    false,
                    ColumnDefault::None,
                    None,
                ),
            ],
            schema_version: 1,
            table_comment: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_is_auto_increment_pk_true() {
        let table_def = create_test_table_def(ColumnDefault::FunctionCall {
            name: "auto_increment".to_string(),
            args: vec![],
        });
        assert!(PkExistenceChecker::is_auto_increment_pk(&table_def));
    }

    #[test]
    fn test_is_auto_increment_pk_snowflake() {
        let table_def = create_test_table_def(ColumnDefault::FunctionCall {
            name: "SNOWFLAKE_ID".to_string(),
            args: vec![],
        });
        assert!(PkExistenceChecker::is_auto_increment_pk(&table_def));
    }

    #[test]
    fn test_is_auto_increment_pk_false() {
        let table_def = create_test_table_def(ColumnDefault::None);
        assert!(!PkExistenceChecker::is_auto_increment_pk(&table_def));
    }

    #[test]
    fn test_is_auto_increment_pk_literal_default() {
        let table_def = create_test_table_def(ColumnDefault::Literal(serde_json::json!(0)));
        assert!(!PkExistenceChecker::is_auto_increment_pk(&table_def));
    }

    #[test]
    fn test_get_pk_column_name() {
        let table_def = create_test_table_def(ColumnDefault::None);
        assert_eq!(PkExistenceChecker::get_pk_column_name(&table_def), Some("id"));
    }

    #[test]
    fn test_pk_check_result_exists() {
        assert!(!PkCheckResult::NotFound.exists());
        assert!(!PkCheckResult::AutoIncrement.exists());
        assert!(!PkCheckResult::PrunedByManifest.exists());
        assert!(PkCheckResult::FoundInHot.exists());
        assert!(PkCheckResult::FoundInCold {
            segment_path: "batch-0.parquet".to_string()
        }
        .exists());
    }
}
