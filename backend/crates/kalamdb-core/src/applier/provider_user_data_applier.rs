//! Implementation of UserDataApplier for provider persistence
//!
//! This module provides the concrete implementation of `kalamdb_raft::UserDataApplier`
//! that persists user table data to the actual providers (RocksDB-backed stores).
//!
//! Called by UserDataStateMachine after Raft consensus on all nodes.

use async_trait::async_trait;

use crate::app_context::AppContext;
use crate::providers::base::BaseTableProvider;
use crate::providers::{StreamTableProvider, UserTableProvider};
use kalamdb_commons::models::{Row, UserId};
use kalamdb_commons::TableId;
use kalamdb_raft::{RaftError, UserDataApplier};

/// UserDataApplier implementation using table providers
///
/// This is called by the Raft state machine when applying committed commands.
/// All nodes (leader and followers) execute this, ensuring consistent data.
///
/// Uses `AppContext::get()` to access the singleton at runtime, allowing the
/// applier to be created before AppContext is fully initialized.
pub struct ProviderUserDataApplier;

impl ProviderUserDataApplier {
    /// Create a new ProviderUserDataApplier
    pub fn new() -> Self {
        Self
    }
}

impl Default for ProviderUserDataApplier {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl UserDataApplier for ProviderUserDataApplier {
    async fn insert(
        &self,
        table_id: &TableId,
        user_id: &UserId,
        rows_data: &[u8],
    ) -> Result<usize, RaftError> {
        log::debug!(
            "ProviderUserDataApplier: Inserting into {} for user {} ({} bytes)",
            table_id,
            user_id,
            rows_data.len()
        );

        // Deserialize rows from bincode
        let rows: Vec<Row> = bincode::serde::decode_from_slice(rows_data, bincode::config::standard())
            .map(|(rows, _)| rows)
            .map_err(|e| RaftError::provider(format!("Failed to deserialize rows: {}", e)))?;

        if rows.is_empty() {
            return Ok(0);
        }

        // Get AppContext at runtime (allows creation before AppContext exists)
        let app_context = AppContext::get();

        // Get the provider
        let provider_arc = app_context
            .schema_registry()
            .get_provider(table_id)
            .ok_or_else(|| {
                RaftError::provider(format!("User table provider not found for: {}", table_id))
            })?;

        if let Some(provider) = provider_arc
            .as_any()
            .downcast_ref::<UserTableProvider>()
        {
            let row_ids = provider.insert_batch(user_id, rows).map_err(|e| {
                RaftError::provider(format!("Failed to insert batch: {}", e))
            })?;

            log::info!(
                "ProviderUserDataApplier: Inserted {} rows into {}",
                row_ids.len(),
                table_id
            );

            Ok(row_ids.len())
        } else if let Some(provider) = provider_arc
            .as_any()
            .downcast_ref::<StreamTableProvider>()
        {
            let row_ids = provider.insert_batch(user_id, rows).map_err(|e| {
                RaftError::provider(format!("Failed to insert stream batch: {}", e))
            })?;

            log::info!(
                "ProviderUserDataApplier: Inserted {} stream rows into {}",
                row_ids.len(),
                table_id
            );

            Ok(row_ids.len())
        } else {
            Err(RaftError::provider(format!(
                "Provider type mismatch for user table {}",
                table_id
            )))
        }
    }

    async fn update(
        &self,
        table_id: &TableId,
        user_id: &UserId,
        updates_data: &[u8],
        filter_data: Option<&[u8]>,
    ) -> Result<usize, RaftError> {
        log::debug!(
            "ProviderUserDataApplier: Updating {} for user {} ({} bytes)",
            table_id,
            user_id,
            updates_data.len()
        );

        let pk_value: String = filter_data
            .ok_or_else(|| RaftError::provider("Update requires filter_data with PK value".to_string()))
            .and_then(|data| {
                bincode::serde::decode_from_slice(data, bincode::config::standard())
                    .map(|(v, _)| v)
                    .map_err(|e| RaftError::provider(format!("Failed to deserialize PK value: {}", e)))
            })?;

        let mut rows: Vec<Row> =
            bincode::serde::decode_from_slice(updates_data, bincode::config::standard())
                .map(|(rows, _)| rows)
                .map_err(|e| RaftError::provider(format!("Failed to deserialize updates: {}", e)))?;

        let updates = rows
            .pop()
            .ok_or_else(|| RaftError::provider("Update requires at least one update row".to_string()))?;

        // Get AppContext at runtime
        let app_context = AppContext::get();

        // Get the provider
        let provider_arc = app_context
            .schema_registry()
            .get_provider(table_id)
            .ok_or_else(|| {
                RaftError::provider(format!("User table provider not found for: {}", table_id))
            })?;

        if let Some(provider) = provider_arc
            .as_any()
            .downcast_ref::<UserTableProvider>()
        {
            match provider.update_by_id_field(user_id, &pk_value, updates.clone()) {
                Ok(_) => Ok(1),
                Err(crate::error::KalamDbError::NotFound(_)) => {
                    if let Some(key) =
                        provider
                            .find_row_key_by_id_field(user_id, &pk_value)
                            .map_err(|e| {
                                RaftError::provider(format!("Failed to find row key: {}", e))
                            })?
                    {
                        provider.update(user_id, &key, updates).map_err(|e| {
                            RaftError::provider(format!("Failed to update row: {}", e))
                        })?;
                        Ok(1)
                    } else {
                        Ok(0)
                    }
                }
                Err(e) => Err(RaftError::provider(format!(
                    "Failed to update row: {}",
                    e
                ))),
            }
        } else if let Some(provider) = provider_arc
            .as_any()
            .downcast_ref::<StreamTableProvider>()
        {
            match provider.update_by_id_field(user_id, &pk_value, updates.clone()) {
                Ok(_) => Ok(1),
                Err(crate::error::KalamDbError::NotFound(_)) => {
                    if let Some(key) =
                        provider
                            .find_row_key_by_id_field(user_id, &pk_value)
                            .map_err(|e| {
                                RaftError::provider(format!("Failed to find row key: {}", e))
                            })?
                    {
                        provider.update(user_id, &key, updates).map_err(|e| {
                            RaftError::provider(format!("Failed to update row: {}", e))
                        })?;
                        Ok(1)
                    } else {
                        Ok(0)
                    }
                }
                Err(e) => Err(RaftError::provider(format!(
                    "Failed to update row: {}",
                    e
                ))),
            }
        } else {
            Err(RaftError::provider(format!(
                "Provider type mismatch for user table {}",
                table_id
            )))
        }
    }

    async fn delete(
        &self,
        table_id: &TableId,
        user_id: &UserId,
        filter_data: Option<&[u8]>,
    ) -> Result<usize, RaftError> {
        log::debug!(
            "ProviderUserDataApplier: Deleting from {} for user {}",
            table_id,
            user_id
        );

        let pk_values: Vec<String> = if let Some(data) = filter_data {
            bincode::serde::decode_from_slice(data, bincode::config::standard())
                .map(|(pks, _)| pks)
                .map_err(|e| RaftError::provider(format!("Failed to deserialize PK list: {}", e)))?
        } else {
            return Err(RaftError::provider(
                "Delete requires filter_data with PK list".to_string(),
            ));
        };

        if pk_values.is_empty() {
            return Ok(0);
        }

        let app_context = AppContext::get();
        let provider_arc = app_context
            .schema_registry()
            .get_provider(table_id)
            .ok_or_else(|| {
                RaftError::provider(format!("User table provider not found for: {}", table_id))
            })?;

        if let Some(provider) = provider_arc
            .as_any()
            .downcast_ref::<UserTableProvider>()
        {
            let mut deleted_count = 0;
            for pk_value in &pk_values {
                if provider
                    .delete_by_id_field(user_id, pk_value)
                    .map_err(|e| RaftError::provider(format!("Failed to delete row: {}", e)))?
                {
                    deleted_count += 1;
                }
            }

            Ok(deleted_count)
        } else if let Some(provider) = provider_arc
            .as_any()
            .downcast_ref::<StreamTableProvider>()
        {
            let mut deleted_count = 0;
            for pk_value in &pk_values {
                if provider
                    .delete_by_id_field(user_id, pk_value)
                    .map_err(|e| RaftError::provider(format!("Failed to delete row: {}", e)))?
                {
                    deleted_count += 1;
                }
            }

            Ok(deleted_count)
        } else {
            Err(RaftError::provider(format!(
                "Provider type mismatch for user table {}",
                table_id
            )))
        }
    }
}
