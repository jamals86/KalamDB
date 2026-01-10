//! DML Executor - Handles data manipulation (INSERT/UPDATE/DELETE)
//!
//! This module contains the logic for executing data operations against
//! specific table providers. It is designed to be stateless and thread-safe,
//! allowing concurrent execution from multiple Data Raft groups.
//!
//! ## Design
//!
//! - Stateless: No internal locks, safe for parallel execution across shards
//! - Unified: Same code path for standalone and cluster modes
//! - Provider-agnostic: Handles User, Stream, and Shared table types

use std::sync::Arc;

use kalamdb_commons::models::{Row, UserId};
use kalamdb_commons::TableId;

use crate::app_context::AppContext;
use crate::applier::error::ApplierError;
use crate::providers::base::BaseTableProvider;
use crate::providers::{SharedTableProvider, StreamTableProvider, UserTableProvider};

/// Executor for DML operations (Data Plane)
///
/// This executor is stateless and designed for concurrent access from
/// multiple Data Raft groups. Each method accesses the schema registry
/// to get the appropriate provider and performs the operation.
pub struct DmlExecutor {
    app_context: Arc<AppContext>,
}

impl DmlExecutor {
    /// Create a new DmlExecutor
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }

    // =========================================================================
    // User Table Operations (per-user sharded data)
    // =========================================================================

    /// Insert rows into a user table
    pub async fn insert_user_data(
        &self,
        table_id: &TableId,
        user_id: &UserId,
        rows_data: &[u8],
    ) -> Result<usize, ApplierError> {
        let rows: Vec<Row> = bincode::serde::decode_from_slice(rows_data, bincode::config::standard())
            .map(|(rows, _)| rows)
            .map_err(|e| ApplierError::Serialization(format!("Failed to deserialize rows: {}", e)))?;

        if rows.is_empty() {
            return Ok(0);
        }

        let schema_registry = self.app_context.schema_registry();
        let provider_arc = schema_registry.get_provider(table_id).ok_or_else(|| {
            ApplierError::not_found("Table provider", table_id)
        })?;

        // Try UserTableProvider first, then StreamTableProvider
        if let Some(provider) = provider_arc.as_any().downcast_ref::<UserTableProvider>() {
            let row_ids = provider
                .insert_batch(user_id, rows)
                .map_err(|e| ApplierError::Execution(format!("Failed to insert batch: {}", e)))?;
            log::info!("DmlExecutor: Inserted {} rows into {}", row_ids.len(), table_id);
            Ok(row_ids.len())
        } else if let Some(provider) = provider_arc.as_any().downcast_ref::<StreamTableProvider>() {
            let row_ids = provider
                .insert_batch(user_id, rows)
                .map_err(|e| ApplierError::Execution(format!("Failed to insert stream batch: {}", e)))?;
            log::info!("DmlExecutor: Inserted {} stream rows into {}", row_ids.len(), table_id);
            Ok(row_ids.len())
        } else {
            Err(ApplierError::Execution(format!(
                "Provider type mismatch for user table {}",
                table_id
            )))
        }
    }

    /// Update rows in a user table
    pub async fn update_user_data(
        &self,
        table_id: &TableId,
        user_id: &UserId,
        updates_data: &[u8],
        filter_data: Option<&[u8]>,
    ) -> Result<usize, ApplierError> {
        let pk_value: String = filter_data
            .ok_or_else(|| ApplierError::Validation("Update requires filter_data with PK value".to_string()))
            .and_then(|data| {
                bincode::serde::decode_from_slice(data, bincode::config::standard())
                    .map(|(v, _)| v)
                    .map_err(|e| ApplierError::Serialization(format!("Failed to deserialize PK value: {}", e)))
            })?;

        let mut rows: Vec<Row> = bincode::serde::decode_from_slice(updates_data, bincode::config::standard())
            .map(|(rows, _)| rows)
            .map_err(|e| ApplierError::Serialization(format!("Failed to deserialize updates: {}", e)))?;

        let updates = rows
            .pop()
            .ok_or_else(|| ApplierError::Validation("Update requires at least one update row".to_string()))?;

        let schema_registry = self.app_context.schema_registry();
        let provider_arc = schema_registry.get_provider(table_id).ok_or_else(|| {
            ApplierError::not_found("Table provider", table_id)
        })?;

        if let Some(provider) = provider_arc.as_any().downcast_ref::<UserTableProvider>() {
            self.update_user_provider(provider, user_id, &pk_value, updates)
        } else if let Some(provider) = provider_arc.as_any().downcast_ref::<StreamTableProvider>() {
            self.update_stream_provider(provider, user_id, &pk_value, updates)
        } else {
            Err(ApplierError::Execution(format!(
                "Provider type mismatch for user table {}",
                table_id
            )))
        }
    }

    /// Delete rows from a user table
    pub async fn delete_user_data(
        &self,
        table_id: &TableId,
        user_id: &UserId,
        filter_data: Option<&[u8]>,
    ) -> Result<usize, ApplierError> {
        let pk_values: Vec<String> = if let Some(data) = filter_data {
            bincode::serde::decode_from_slice(data, bincode::config::standard())
                .map(|(pks, _)| pks)
                .map_err(|e| ApplierError::Serialization(format!("Failed to deserialize PK list: {}", e)))?
        } else {
            return Err(ApplierError::Validation("Delete requires filter_data with PK list".to_string()));
        };

        if pk_values.is_empty() {
            return Ok(0);
        }

        let schema_registry = self.app_context.schema_registry();
        let provider_arc = schema_registry.get_provider(table_id).ok_or_else(|| {
            ApplierError::not_found("Table provider", table_id)
        })?;

        if let Some(provider) = provider_arc.as_any().downcast_ref::<UserTableProvider>() {
            let mut deleted_count = 0;
            for pk_value in &pk_values {
                if provider
                    .delete_by_id_field(user_id, pk_value)
                    .map_err(|e| ApplierError::Execution(format!("Failed to delete row: {}", e)))?
                {
                    deleted_count += 1;
                }
            }
            log::info!("DmlExecutor: Deleted {} rows from {}", deleted_count, table_id);
            Ok(deleted_count)
        } else if let Some(provider) = provider_arc.as_any().downcast_ref::<StreamTableProvider>() {
            let mut deleted_count = 0;
            for pk_value in &pk_values {
                if provider
                    .delete_by_id_field(user_id, pk_value)
                    .map_err(|e| ApplierError::Execution(format!("Failed to delete row: {}", e)))?
                {
                    deleted_count += 1;
                }
            }
            log::info!("DmlExecutor: Deleted {} stream rows from {}", deleted_count, table_id);
            Ok(deleted_count)
        } else {
            Err(ApplierError::Execution(format!(
                "Provider type mismatch for user table {}",
                table_id
            )))
        }
    }

    // =========================================================================
    // Shared Table Operations (global data, not per-user)
    // =========================================================================

    /// Insert rows into a shared table
    pub async fn insert_shared_data(
        &self,
        table_id: &TableId,
        rows_data: &[u8],
    ) -> Result<usize, ApplierError> {
        let rows: Vec<Row> = bincode::serde::decode_from_slice(rows_data, bincode::config::standard())
            .map(|(rows, _)| rows)
            .map_err(|e| ApplierError::Serialization(format!("Failed to deserialize rows: {}", e)))?;

        if rows.is_empty() {
            return Ok(0);
        }

        let schema_registry = self.app_context.schema_registry();
        let provider_arc = schema_registry.get_provider(table_id).ok_or_else(|| {
            ApplierError::not_found("Shared table provider", table_id)
        })?;

        if let Some(provider) = provider_arc.as_any().downcast_ref::<SharedTableProvider>() {
            let system_user = UserId::from("system");
            let row_ids = provider
                .insert_batch(&system_user, rows)
                .map_err(|e| ApplierError::Execution(format!("Failed to insert batch: {}", e)))?;
            log::info!("DmlExecutor: Inserted {} shared rows into {}", row_ids.len(), table_id);
            Ok(row_ids.len())
        } else {
            Err(ApplierError::Execution(format!(
                "Provider type mismatch for shared table {}",
                table_id
            )))
        }
    }

    /// Update rows in a shared table
    pub async fn update_shared_data(
        &self,
        table_id: &TableId,
        updates_data: &[u8],
        filter_data: Option<&[u8]>,
    ) -> Result<usize, ApplierError> {
        let rows: Vec<Row> = bincode::serde::decode_from_slice(updates_data, bincode::config::standard())
            .map(|(rows, _)| rows)
            .map_err(|e| ApplierError::Serialization(format!("Failed to deserialize updates: {}", e)))?;

        if rows.is_empty() {
            return Ok(0);
        }

        let pk_value: String = filter_data
            .ok_or_else(|| ApplierError::Validation("Update requires filter_data with PK value".to_string()))
            .and_then(|data| {
                bincode::serde::decode_from_slice(data, bincode::config::standard())
                    .map(|(v, _)| v)
                    .map_err(|e| ApplierError::Serialization(format!("Failed to deserialize PK value: {}", e)))
            })?;

        let schema_registry = self.app_context.schema_registry();
        let provider_arc = schema_registry.get_provider(table_id).ok_or_else(|| {
            ApplierError::not_found("Shared table provider", table_id)
        })?;

        if let Some(provider) = provider_arc.as_any().downcast_ref::<SharedTableProvider>() {
            let system_user = UserId::from("system");
            let updates = rows.into_iter().next().unwrap();

            provider
                .update_by_id_field(&system_user, &pk_value, updates)
                .map_err(|e| ApplierError::Execution(format!("Failed to update row: {}", e)))?;

            log::info!("DmlExecutor: Updated 1 shared row in {} (pk={})", table_id, pk_value);
            Ok(1)
        } else {
            Err(ApplierError::Execution(format!(
                "Provider type mismatch for shared table {}",
                table_id
            )))
        }
    }

    /// Delete rows from a shared table
    pub async fn delete_shared_data(
        &self,
        table_id: &TableId,
        filter_data: Option<&[u8]>,
    ) -> Result<usize, ApplierError> {
        let pk_values: Vec<String> = if let Some(data) = filter_data {
            bincode::serde::decode_from_slice(data, bincode::config::standard())
                .map(|(pks, _)| pks)
                .map_err(|e| ApplierError::Serialization(format!("Failed to deserialize PK list: {}", e)))?
        } else {
            return Err(ApplierError::Validation("Delete requires filter_data with PK list".to_string()));
        };

        if pk_values.is_empty() {
            return Ok(0);
        }

        let schema_registry = self.app_context.schema_registry();
        let provider_arc = schema_registry.get_provider(table_id).ok_or_else(|| {
            ApplierError::not_found("Shared table provider", table_id)
        })?;

        if let Some(provider) = provider_arc.as_any().downcast_ref::<SharedTableProvider>() {
            let system_user = UserId::from("system");
            let mut deleted_count = 0;

            for pk_value in &pk_values {
                if provider
                    .delete_by_id_field(&system_user, pk_value)
                    .map_err(|e| ApplierError::Execution(format!("Failed to delete row: {}", e)))?
                {
                    deleted_count += 1;
                }
            }

            log::info!("DmlExecutor: Deleted {} shared rows from {}", deleted_count, table_id);
            Ok(deleted_count)
        } else {
            Err(ApplierError::Execution(format!(
                "Provider type mismatch for shared table {}",
                table_id
            )))
        }
    }

    // =========================================================================
    // Helper Methods
    // =========================================================================

    /// Update with fallback for UserTableProvider
    fn update_user_provider(
        &self,
        provider: &UserTableProvider,
        user_id: &UserId,
        pk_value: &str,
        updates: Row,
    ) -> Result<usize, ApplierError> {
        match provider.update_by_id_field(user_id, pk_value, updates.clone()) {
            Ok(_) => Ok(1),
            Err(crate::error::KalamDbError::NotFound(_)) => {
                if let Some(key) = provider
                    .find_row_key_by_id_field(user_id, pk_value)
                    .map_err(|e| ApplierError::Execution(format!("Failed to find row key: {}", e)))?
                {
                    provider
                        .update(user_id, &key, updates)
                        .map_err(|e| ApplierError::Execution(format!("Failed to update row: {}", e)))?;
                    Ok(1)
                } else {
                    Ok(0)
                }
            }
            Err(e) => Err(ApplierError::Execution(format!("Failed to update row: {}", e))),
        }
    }

    /// Update with fallback for StreamTableProvider
    fn update_stream_provider(
        &self,
        provider: &StreamTableProvider,
        user_id: &UserId,
        pk_value: &str,
        updates: Row,
    ) -> Result<usize, ApplierError> {
        match provider.update_by_id_field(user_id, pk_value, updates.clone()) {
            Ok(_) => Ok(1),
            Err(crate::error::KalamDbError::NotFound(_)) => {
                if let Some(key) = provider
                    .find_row_key_by_id_field(user_id, pk_value)
                    .map_err(|e| ApplierError::Execution(format!("Failed to find row key: {}", e)))?
                {
                    provider
                        .update(user_id, &key, updates)
                        .map_err(|e| ApplierError::Execution(format!("Failed to update row: {}", e)))?;
                    Ok(1)
                } else {
                    Ok(0)
                }
            }
            Err(e) => Err(ApplierError::Execution(format!("Failed to update row: {}", e))),
        }
    }
}
