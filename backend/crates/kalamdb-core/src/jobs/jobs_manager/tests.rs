use super::types::JobsManager;
use crate::test_helpers::*;
use kalamdb_commons::models::schemas::TableOptions;
use kalamdb_commons::models::{NamespaceId, TableId, TableName};
use kalamdb_commons::schemas::TableType;
use std::sync::Arc;

// TODO: Re-enable after refactoring test utilities
// This test needs app_context() helper which was removed
#[ignore]
#[tokio::test]
async fn test_check_stream_eviction_finds_stream_table() {
    // Test disabled - needs refactoring to use proper AppContext setup
    // instead of global app_context() singleton
}
