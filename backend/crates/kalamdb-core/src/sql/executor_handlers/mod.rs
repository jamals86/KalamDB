//! Handler modules for SQL execution (PROPOSED STRUCTURE)
//!
//! This is the proposed modular structure for the SQL executor.
//! To activate this structure:
//! 1. Rename `sql/executor.rs` to `sql/executor/mod.rs`
//! 2. Move this directory to `sql/executor/handlers/`
//! 3. Add `pub mod handlers;` to executor/mod.rs
//! 4. Migrate execute_* methods to handler modules

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

pub use types::{ExecutionContext, ExecutionMetadata, ExecutionResult};
