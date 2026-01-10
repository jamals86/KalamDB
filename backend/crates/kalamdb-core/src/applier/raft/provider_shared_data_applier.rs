//! Implementation of SharedDataApplier for provider persistence
//!
//! This module provides the concrete implementation of `kalamdb_raft::SharedDataApplier`
//! that persists shared table data to the actual providers (RocksDB-backed stores).
//!
//! Called by SharedDataStateMachine after Raft consensus on all nodes.

use async_trait::async_trait;
use std::sync::Arc;

use crate::app_context::AppContext;
use crate::applier::executor::CommandExecutorImpl;
use kalamdb_commons::TableId;
use kalamdb_raft::{RaftError, SharedDataApplier};

/// SharedDataApplier implementation using Unified Command Executor
///
/// This is called by the Raft state machine when applying committed commands.
/// It delegates to `CommandExecutorImpl::dml()` for actual logic.
pub struct ProviderSharedDataApplier {
    executor: CommandExecutorImpl,
}

impl ProviderSharedDataApplier {
    /// Create a new ProviderSharedDataApplier
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self {
            executor: CommandExecutorImpl::new(app_context),
        }
    }
}

impl Default for ProviderSharedDataApplier {
    fn default() -> Self {
        if let Ok(app_ctx) = std::panic::catch_unwind(|| AppContext::get()) {
            Self::new(app_ctx)
        } else {
             panic!("ProviderSharedDataApplier::default() called but AppContext not available. Use new(Arc<AppContext>) instead.");
        }
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

        self.executor.dml()
            .insert_shared_data(table_id, rows_data)
            .await
            .map_err(|e| RaftError::provider(e.to_string()))
    }

    async fn update(
        &self,
        table_id: &TableId,
        updates_data: &[u8],
        filter_data: Option<&[u8]>,
    ) -> Result<usize, RaftError> {
        log::debug!(
            "ProviderSharedDataApplier: Updating {} ({} bytes)",
            table_id,
            updates_data.len()
        );

        self.executor.dml()
            .update_shared_data(table_id, updates_data, filter_data)
            .await
            .map_err(|e| RaftError::provider(e.to_string()))
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

        self.executor.dml()
            .delete_shared_data(table_id, filter_data)
            .await
            .map_err(|e| RaftError::provider(e.to_string()))
    }
}
