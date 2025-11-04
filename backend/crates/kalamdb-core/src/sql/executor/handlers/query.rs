//! Query handlers
//!
//! SELECT, SHOW, DESCRIBE query operations.

// These functions will be moved from executor/mod.rs during migration
// Placeholder module for now

use crate::error::KalamDbError;
use crate::sql::executor::SqlExecutor;
use datafusion::prelude::SessionContext;

use super::types::{ExecutionContext, ExecutionResult};

// TODO: Migrate these functions from executor/mod.rs:
// - SELECT logic → select
// - execute_show_namespaces → show_namespaces
// - execute_show_tables → show_tables
// - execute_show_storages → show_storages
// - execute_show_table_stats → show_stats
// - execute_describe_table → describe_table
