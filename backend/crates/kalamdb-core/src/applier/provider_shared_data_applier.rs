//! Implementation of SharedDataApplier for provider persistence
//!
//! This module provides the concrete implementation of `kalamdb_raft::SharedDataApplier`
//! that persists shared table data to the actual providers (RocksDB-backed stores).
//!
//! Called by SharedDataStateMachine after Raft consensus on all nodes.

use async_trait::async_trait;

use crate::app_context::AppContext;
use crate::providers::base::BaseTableProvider;
use crate::providers::SharedTableProvider;
use kalamdb_commons::models::{Row, UserId};
use kalamdb_commons::TableId;
use kalamdb_raft::{RaftError, SharedDataApplier};

/// SharedDataApplier implementation using table providers
///
/// This is called by the Raft state machine when applying committed commands.
/// All nodes (leader and followers) execute this, ensuring consistent data.
///
/// Uses `AppContext::get()` to access the singleton at runtime, allowing the
/// applier to be created before AppContext is fully initialized.
pub struct ProviderSharedDataApplier;

impl ProviderSharedDataApplier {
    /// Create a new ProviderSharedDataApplier
    pub fn new() -> Self {
        Self
    }
}

impl Default for ProviderSharedDataApplier {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SharedDataApplier for ProviderSharedDataApplier {
    async fn insert(
        &self,
        table_id: &TableId,
        rows_data: &[u8],
    ) -> Result<usize, RaftError> {
        log::debug!(
            "ProviderSharedDataApplier: Inserting into {} ({} bytes)",
            table_id,
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
                RaftError::provider(format!("Shared table provider not found for: {}", table_id))
            })?;

        // Downcast to SharedTableProvider
        if let Some(provider) = provider_arc
            .as_any()
            .downcast_ref::<SharedTableProvider>()
        {
            // Shared tables use a system user for ownership
            let system_user = UserId::from("system");
            let row_ids = provider.insert_batch(&system_user, rows).map_err(|e| {
                RaftError::provider(format!("Failed to insert batch: {}", e))
            })?;

            log::info!(
                "ProviderSharedDataApplier: Inserted {} rows into {}",
                row_ids.len(),
                table_id
            );

            Ok(row_ids.len())
        } else {
            Err(RaftError::provider(format!(
                "Provider type mismatch for shared table {}",
                table_id
            )))
        }
    }

    async fn update(
        &self,
        table_id: &TableId,
        updates_data: &[u8],
        _filter_data: Option<&[u8]>,
    ) -> Result<usize, RaftError> {
        log::debug!(
            "ProviderSharedDataApplier: Updating {} ({} bytes)",
            table_id,
            updates_data.len()
        );

        // For now, updates use the same Row format
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
                RaftError::provider(format!("Shared table provider not found for: {}", table_id))
            })?;

        if let Some(provider) = provider_arc
            .as_any()
            .downcast_ref::<SharedTableProvider>()
        {
            let system_user = UserId::from("system");
            let row_ids = provider.insert_batch(&system_user, rows).map_err(|e| {
                RaftError::provider(format!("Failed to update batch: {}", e))
            })?;

            log::info!(
                "ProviderSharedDataApplier: Updated {} rows in {}",
                row_ids.len(),
                table_id
            );

            Ok(row_ids.len())
        } else {
            Err(RaftError::provider(format!(
                "Provider type mismatch for shared table {}",
                table_id
            )))
        }
    }

    async fn delete(
        &self,
        table_id: &TableId,
        filter_data: Option<&[u8]>,
    ) -> Result<usize, RaftError> {
        log::debug!(
            "ProviderSharedDataApplier: Deleting from {}",
            table_id
        );

        // filter_data contains serialized list of primary key values to delete
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

        // Get AppContext at runtime
        let app_context = AppContext::get();

        // Get the provider
        let provider_arc = app_context
            .schema_registry()
            .get_provider(table_id)
            .ok_or_else(|| {
                RaftError::provider(format!("Shared table provider not found for: {}", table_id))
            })?;

        if let Some(provider) = provider_arc
            .as_any()
            .downcast_ref::<SharedTableProvider>()
        {
            let system_user = UserId::from("system");
            let mut deleted_count = 0;
            
            for pk_value in &pk_values {
                if provider.delete_by_id_field(&system_user, pk_value).map_err(|e| {
                    RaftError::provider(format!("Failed to delete row: {}", e))
                })? {
                    deleted_count += 1;
                }
            }

            log::info!(
                "ProviderSharedDataApplier: Deleted {} rows from {}",
                deleted_count,
                table_id
            );

            Ok(deleted_count)
        } else {
            Err(RaftError::provider(format!(
                "Provider type mismatch for shared table {}",
                table_id
            )))
        }
    }
}
