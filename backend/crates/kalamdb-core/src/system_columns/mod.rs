//! System Column Management Service
//!
//! Centralizes all system column logic (`_id`, `_updated`, `_deleted`) for KalamDB tables.
//! 
//! ## Responsibilities
//! - Generate unique Snowflake IDs for `_id` column
//! - Manage `_updated` timestamps with nanosecond monotonicity
//! - Handle `_deleted` soft delete flags
//! - Inject system columns into table schemas
//! - Apply deletion filters to queries
//!
//! ## Architecture
//! - **SnowflakeGenerator**: Generates time-ordered unique IDs (41-bit timestamp + 10-bit worker + 12-bit sequence)
//! - **Monotonic Timestamps**: Ensures `_updated` never decreases, +1ns bump on collision
//! - **Soft Deletes**: Records marked `_deleted=true` are filtered from queries unless explicitly requested
//!
//! ## Usage
//! ```rust
//! use kalamdb_core::system_columns::SystemColumnsService;
//! use kalamdb_core::app_context::AppContext;
//!
//! let app_ctx = AppContext::get();
//! let sys_cols = SystemColumnsService::new(app_ctx);
//!
//! // Generate unique ID
//! let id = sys_cols.generate_id()?;
//!
//! // Handle INSERT operation
//! let (record_id, updated_ts, deleted_flag) = sys_cols.handle_insert(None)?;
//!
//! // Handle UPDATE operation  
//! let (updated_ts, deleted_flag) = sys_cols.handle_update(existing_id, previous_updated)?;
//!
//! // Handle DELETE operation
//! let (updated_ts, deleted_flag) = sys_cols.handle_delete(existing_id, previous_updated)?;
//! ```

use kalamdb_commons::ids::snowflake::SnowflakeGenerator;
use kalamdb_commons::models::schemas::{ColumnDefinition, ColumnDefault, TableDefinition};
use crate::error::KalamDbError;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// System Columns Service
///
/// Manages all system column operations: `_id`, `_updated`, `_deleted`.
/// Thread-safe via interior mutability in SnowflakeGenerator.
pub struct SystemColumnsService {
    /// Snowflake ID generator for `_id` column
    snowflake_gen: Arc<SnowflakeGenerator>,

    /// Worker ID from AppContext config (for logging/debugging)
    worker_id: u16,
}

impl SystemColumnsService {
    /// System column names
    pub const COL_ID: &'static str = "_id";
    pub const COL_UPDATED: &'static str = "_updated";
    pub const COL_DELETED: &'static str = "_deleted";

    /// Create a new SystemColumnsService
    ///
    /// # Arguments
    /// * `worker_id` - Node identifier from config.toml [server.node_id]
    ///
    /// # Returns
    /// A new SystemColumnsService instance
    pub fn new(worker_id: u16) -> Self {
        let snowflake_gen = Arc::new(SnowflakeGenerator::new(worker_id));

        Self {
            snowflake_gen,
            worker_id,
        }
    }

    /// Generate a unique Snowflake ID for `_id` column
    ///
    /// # Returns
    /// A 64-bit signed integer (i64) containing:
    /// - 41 bits: timestamp in milliseconds since 2024-01-01
    /// - 10 bits: worker/node ID
    /// - 12 bits: sequence number
    ///
    /// # Errors
    /// Returns `KalamDbError::SystemColumnViolation` if clock moves backwards
    pub fn generate_id(&self) -> Result<i64, KalamDbError> {
        self.snowflake_gen
            .next_id()
            .map_err(|e| KalamDbError::SystemColumnViolation(format!("Snowflake ID generation failed: {}", e)))
    }

    /// Add system columns to a table definition
    ///
    /// Injects `_id BIGINT PRIMARY KEY`, `_updated TIMESTAMP`, `_deleted BOOLEAN` columns
    /// if they don't already exist.
    ///
    /// # Arguments
    /// * `table_def` - Mutable reference to table definition
    ///
    /// # Errors
    /// Returns error if column names conflict with user-defined columns
    pub fn add_system_columns(&self, table_def: &mut TableDefinition) -> Result<(), KalamDbError> {
        // Check for conflicts
        for col in &table_def.columns {
            if col.column_name == Self::COL_ID
                || col.column_name == Self::COL_UPDATED
                || col.column_name == Self::COL_DELETED
            {
                return Err(KalamDbError::SystemColumnViolation(format!(
                    "Column name '{}' is reserved for system columns",
                    col.column_name
                )));
            }
        }

        let next_ordinal = table_def.columns.len() as u32 + 1;

        // Add _id column (BIGINT, PRIMARY KEY, NOT NULL)
        table_def.columns.push(ColumnDefinition {
            column_name: Self::COL_ID.to_string(),
            ordinal_position: next_ordinal,
            data_type: kalamdb_commons::models::datatypes::KalamDataType::BigInt,
            is_nullable: false,
            is_primary_key: true,
            is_partition_key: false,
            default_value: ColumnDefault::None,
            column_comment: Some("System-generated Snowflake ID (auto-assigned on INSERT)".to_string()),
        });

        // Add _updated column (TIMESTAMP, NOT NULL)
        table_def.columns.push(ColumnDefinition {
            column_name: Self::COL_UPDATED.to_string(),
            ordinal_position: next_ordinal + 1,
            data_type: kalamdb_commons::models::datatypes::KalamDataType::Timestamp,
            is_nullable: false,
            is_primary_key: false,
            is_partition_key: false,
            default_value: ColumnDefault::None,
            column_comment: Some("Nanosecond timestamp of last modification (auto-updated)".to_string()),
        });

        // Add _deleted column (BOOLEAN, NOT NULL, DEFAULT FALSE)
        table_def.columns.push(ColumnDefinition {
            column_name: Self::COL_DELETED.to_string(),
            ordinal_position: next_ordinal + 2,
            data_type: kalamdb_commons::models::datatypes::KalamDataType::Boolean,
            is_nullable: false,
            is_primary_key: false,
            is_partition_key: false,
            default_value: ColumnDefault::Literal(serde_json::json!(false)),
            column_comment: Some("Soft delete flag (true = deleted, false = active)".to_string()),
        });

        Ok(())
    }

    /// Handle INSERT operation - generate system column values
    ///
    /// # Arguments
    /// * `user_provided_id` - Optional user-provided `_id` (MUST be None, manual assignment forbidden)
    ///
    /// # Returns
    /// Tuple of (`_id`, `_updated` nanosecond timestamp, `_deleted` = false)
    ///
    /// # Errors
    /// - `SystemColumnViolation` if user tries to manually assign `_id`
    /// - `SystemColumnViolation` if Snowflake ID generation fails
    pub fn handle_insert(&self, user_provided_id: Option<i64>) -> Result<(i64, i64, bool), KalamDbError> {
        // Reject manual _id assignments (FR-071)
        if user_provided_id.is_some() {
            return Err(KalamDbError::SystemColumnViolation(
                "Manual assignment of '_id' column is forbidden. System auto-generates Snowflake IDs.".to_string(),
            ));
        }

        // Generate unique ID
        let id = self.generate_id()?;

        // Current timestamp in nanoseconds
        let updated = Self::current_timestamp_nanos();

        // New records are not deleted
        let deleted = false;

        Ok((id, updated, deleted))
    }

    /// Handle UPDATE operation - update `_updated` timestamp with monotonicity check
    ///
    /// # Arguments
    /// * `record_id` - Existing record `_id` (preserved, not changed)
    /// * `previous_updated` - Previous `_updated` timestamp in nanoseconds
    ///
    /// # Returns
    /// Tuple of (new `_updated` nanosecond timestamp, `_deleted` = false)
    ///
    /// # Details
    /// Ensures `new_updated > previous_updated` by at least 1 nanosecond.
    /// If wall clock time hasn't advanced, bumps timestamp by +1ns.
    pub fn handle_update(&self, _record_id: i64, previous_updated: i64) -> Result<(i64, bool), KalamDbError> {
        let mut new_updated = Self::current_timestamp_nanos();

        // Enforce monotonicity: new_updated > previous_updated
        if new_updated <= previous_updated {
            new_updated = previous_updated + 1;
        }

        // UPDATE preserves _deleted=false (use DELETE to mark deleted)
        let deleted = false;

        Ok((new_updated, deleted))
    }

    /// Handle DELETE operation - set `_deleted = true` and update timestamp
    ///
    /// # Arguments
    /// * `record_id` - Existing record `_id` (preserved, not changed)
    /// * `previous_updated` - Previous `_updated` timestamp in nanoseconds
    ///
    /// # Returns
    /// Tuple of (new `_updated` nanosecond timestamp, `_deleted` = true)
    ///
    /// # Details
    /// Soft delete: record remains in storage with `_deleted=true`.
    /// Queries filter deleted records unless `include_deleted=true`.
    pub fn handle_delete(&self, _record_id: i64, previous_updated: i64) -> Result<(i64, bool), KalamDbError> {
        let mut new_updated = Self::current_timestamp_nanos();

        // Enforce monotonicity
        if new_updated <= previous_updated {
            new_updated = previous_updated + 1;
        }

        // Soft delete
        let deleted = true;

        Ok((new_updated, deleted))
    }

    /// Apply deletion filter to query
    ///
    /// Injects `WHERE _deleted = false` predicate into query AST unless explicitly disabled.
    ///
    /// # Arguments
    /// * `include_deleted` - If true, don't filter deleted records
    ///
    /// # Returns
    /// SQL predicate string to inject, or None if include_deleted=true
    pub fn apply_deletion_filter(&self, include_deleted: bool) -> Option<String> {
        if include_deleted {
            None
        } else {
            Some(format!("{} = false", Self::COL_DELETED))
        }
    }

    /// Get current timestamp in nanoseconds since Unix epoch
    ///
    /// # Returns
    /// Timestamp in nanoseconds (i64)
    fn current_timestamp_nanos() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System clock before Unix epoch")
            .as_nanos() as i64
    }

    /// Get worker ID (for debugging/logging)
    pub fn worker_id(&self) -> u16 {
        self.worker_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_id() {
        let svc = SystemColumnsService::new(1);
        let id1 = svc.generate_id().unwrap();
        let id2 = svc.generate_id().unwrap();

        assert!(id1 > 0);
        assert!(id2 > id1, "IDs should be strictly increasing");
    }

    #[test]
    fn test_add_system_columns() {
        use kalamdb_commons::models::schemas::{TableOptions, TableType};
        use kalamdb_commons::{NamespaceId, TableName};

        let svc = SystemColumnsService::new(1);
        let mut table_def = TableDefinition::new(
            NamespaceId::new("test"),
            TableName::new("table"),
            TableType::User,
            vec![],
            TableOptions::user(),
            None,
        )
        .unwrap();

        svc.add_system_columns(&mut table_def).unwrap();

        assert_eq!(table_def.columns.len(), 3);
        assert_eq!(table_def.columns[0].column_name, "_id");
        assert_eq!(table_def.columns[1].column_name, "_updated");
        assert_eq!(table_def.columns[2].column_name, "_deleted");
    }

    #[test]
    fn test_handle_insert_rejects_manual_id() {
        let svc = SystemColumnsService::new(1);
        let result = svc.handle_insert(Some(12345));

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Manual assignment"));
    }

    #[test]
    fn test_handle_insert_success() {
        let svc = SystemColumnsService::new(1);
        let (id, updated, deleted) = svc.handle_insert(None).unwrap();

        assert!(id > 0);
        assert!(updated > 0);
        assert!(!deleted);
    }

    #[test]
    fn test_handle_update_monotonicity() {
        let svc = SystemColumnsService::new(1);
        let previous_updated = SystemColumnsService::current_timestamp_nanos();

        // Fast execution might have same timestamp
        let (new_updated, deleted) = svc.handle_update(123, previous_updated).unwrap();

        assert!(new_updated > previous_updated, "Timestamp should increment");
        assert!(!deleted);
    }

    #[test]
    fn test_handle_delete() {
        let svc = SystemColumnsService::new(1);
        let previous_updated = SystemColumnsService::current_timestamp_nanos();

        let (new_updated, deleted) = svc.handle_delete(123, previous_updated).unwrap();

        assert!(new_updated > previous_updated);
        assert!(deleted);
    }

    #[test]
    fn test_apply_deletion_filter() {
        let svc = SystemColumnsService::new(1);

        let filter = svc.apply_deletion_filter(false);
        assert_eq!(filter, Some("_deleted = false".to_string()));

        let no_filter = svc.apply_deletion_filter(true);
        assert_eq!(no_filter, None);
    }
}
