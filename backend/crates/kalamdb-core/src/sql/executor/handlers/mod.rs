//! Handler modules for SQL execution
//!
//! This module organizes SQL execution logic into focused handler modules:
//! - `ddl`: CREATE, ALTER, DROP operations
//! - `dml`: INSERT, UPDATE, DELETE operations
//! - `query`: SELECT, SHOW, DESCRIBE queries
//! - `transaction`: BEGIN, COMMIT, ROLLBACK
//! - `subscription`: SUBSCRIBE TO, KILL LIVE QUERY
//! - `authorization`: RBAC authorization checks
//! - `table_registry`: Table registration with DataFusion
//! - `audit`: Audit event logging
//! - `helpers`: Parsing and utility functions
//! - `types`: Common types (ExecutionResult, ExecutionContext, ExecutionMetadata)

pub mod authorization;
pub mod audit;
pub mod ddl;
pub mod dml;
pub mod helpers;
pub mod query;
pub mod subscription;
pub mod system;
pub mod table_registry;
pub mod transaction;
pub mod types;

// Re-export commonly used types
pub use types::{ExecutionContext, ExecutionMetadata, ExecutionResult};
