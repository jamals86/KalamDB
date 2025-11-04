//! Table registry helpers
//!
//! Functions for registering tables with DataFusion and managing table metadata.

// These functions will be moved from executor/mod.rs during migration
// Placeholder module for now

use crate::error::KalamDbError;
use crate::sql::executor::SqlExecutor;

// TODO: Migrate these functions from executor/mod.rs:
// - register_table_with_datafusion
// - cache_table_metadata
// - extract_table_references
// - extract_tables_from_set_expr
// - extract_table_from_factor
// - check_write_permissions
