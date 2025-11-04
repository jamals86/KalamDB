//! DDL (Data Definition Language) handlers
//!
//! CREATE, ALTER, DROP operations for tables, namespaces, storages, and users.

// These functions will be moved from executor/mod.rs during migration
// Placeholder module for now

use crate::error::KalamDbError;
use crate::sql::executor::SqlExecutor;
use datafusion::prelude::SessionContext;

use super::types::{ExecutionContext, ExecutionMetadata, ExecutionResult};

// TODO: Migrate these functions from executor/mod.rs:
// - execute_create_namespace → create_namespace
// - execute_alter_namespace → alter_namespace
// - execute_drop_namespace → drop_namespace
// - execute_create_table → create_table
// - execute_alter_table → alter_table
// - execute_drop_table → drop_table
// - execute_create_storage → create_storage
// - execute_alter_storage → alter_storage
// - execute_drop_storage → drop_storage
// - execute_create_user → create_user
// - execute_alter_user → alter_user
// - execute_drop_user → drop_user
