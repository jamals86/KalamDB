//! DDL Handlers Module
//!
//! This module contains all DDL (Data Definition Language) statement handlers,
//! organized into focused sub-modules for maintainability.
//!
//! # Architecture
//!
//! The DDL handlers are split into focused modules:
//! - `helpers` - Shared helper functions (validation, schema injection, persistence)
//! - `namespace` - CREATE/DROP NAMESPACE handlers (old function-based)
//! - `create_namespace` - CREATE NAMESPACE handler (TypedStatementHandler pattern)
//! - `storage` - CREATE STORAGE handlers
//! - `table` - CREATE/ALTER/DROP TABLE handlers (future)
//!
//! # Usage
//!
//! ```rust
//! use crate::sql::executor::handlers::ddl;
//!
//! // New TypedStatementHandler pattern:
//! let handler = ddl::create_namespace::CreateNamespaceHandler::new(app_ctx);
//! handler.execute(session, statement, sql_text, params, context).await?;
//!
//! // Old function-based (legacy):
//! ddl::namespace::execute_create_namespace(...).await?;
//! ddl::namespace::execute_drop_namespace(...).await?;
//!
//! // Execute storage operations
//! ddl::storage::execute_create_storage(...).await?;
//!
//! // Use helper functions
//! ddl::helpers::validate_table_name("users")?;
//! let schema = ddl::helpers::inject_auto_increment_field(schema)?;
//! ```

pub mod helpers;
pub mod namespace;
pub mod create_namespace;
pub mod storage;
// TODO: pub mod table; - Extract from ddl.rs (lines 490-1265)

// Re-export commonly used functions for convenience
pub use helpers::{
    inject_auto_increment_field, inject_system_columns, save_table_definition, validate_table_name,
};

pub use namespace::{execute_create_namespace, execute_drop_namespace};
pub use storage::execute_create_storage;
pub use create_namespace::CreateNamespaceHandler;
// TODO: pub use table::{execute_create_table, execute_alter_table, execute_drop_table};
