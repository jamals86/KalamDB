//! Implementation of UserDataApplier for provider persistence
//!
//! This module provides the concrete implementation of `kalamdb_raft::UserDataApplier`
//! that persists user table data to the actual providers (RocksDB-backed stores).
//!
//! Called by UserDataStateMachine after Raft consensus on all nodes.

use async_trait::async_trait;

use crate::app_context::AppContext;
use crate::providers::base::BaseTableProvider;
use crate::providers::UserTableProvider;
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

        // Downcast to UserTableProvider
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
        _filter_data: Option<&[u8]>,
    ) -> Result<usize, RaftError> {
        log::debug!(
            "ProviderUserDataApplier: Updating {} for user {} ({} bytes)",
            table_id,
            user_id,
            updates_data.len()
        );

        // For now, updates use the same Row format
        // Future: could have a separate UpdateOperation struct
        let rows: Vec<Row> = bincode::serde::decode_from_slice(updates_data, bincode::config::standard())
            .map(|(rows, _)| rows)
            .map_err(|e| RaftError::provider(format!("Failed to deserialize updates: {}", e)))?;

        if rows.is_empty() {
            return Ok(0);
        }

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
            // Use update_batch if available, otherwise insert (upsert behavior)
            let row_ids = provider.insert_batch(user_id, rows).map_err(|e| {
                RaftError::provider(format!("Failed to update batch: {}", e))
            })?;

            log::info!(
                "ProviderUserDataApplier: Updated {} rows in {}",
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

    async fn delete(
        &self,
        table_id: &TableId,
        user_id: &UserId,
        _filter_data: Option<&[u8]>,
    ) -> Result<usize, RaftError> {
        log::debug!(
            "ProviderUserDataApplier: Deleting from {} for user {}",
            table_id,
            user_id
        );

        // For delete operations, filter_data would contain the filter expression
        // For now, we log a warning since full delete support needs filter parsing
        log::warn!(
            "ProviderUserDataApplier: Delete operation not fully implemented for {}",
            table_id
        );

        // TODO: Implement delete by parsing filter_data and calling provider.delete()
        Ok(0)
    }
}
