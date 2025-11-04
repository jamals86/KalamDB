//! DML (Data Manipulation Language) handlers
//!
//! INSERT, UPDATE, DELETE operations.

// These functions will be moved from executor/mod.rs during migration
// Placeholder module for now

use crate::error::KalamDbError;
use crate::sql::executor::SqlExecutor;
use datafusion::prelude::SessionContext;

use super::types::{ExecutionContext, ExecutionResult};

// TODO: Migrate these functions from executor/mod.rs:
// - execute_update → update
// - execute_delete → delete
// - INSERT logic → insert
