//! System.audit_log table module
//!
//! This module provides the complete implementation for the system.audit_log table:
//! - Schema definition with OnceLock caching (audit_logs_table.rs)
//! - SystemTableStore wrapper for storage operations (audit_logs_store.rs)
//! - DataFusion TableProvider implementation (audit_logs_provider.rs)
//!
//! ## Architecture
//!
//! Follows the EntityStore pattern established by users:
//! 1. Single source of truth for schema (audit_logs_table.rs)
//! 2. Type-safe store operations via SystemTableStore<AuditLogId, AuditLogEntry>
//! 3. DataFusion integration for SQL queries

pub mod audit_logs_provider;
pub mod audit_logs_store;
pub mod audit_logs_table;
pub mod models;

pub use audit_logs_provider::AuditLogsTableProvider;
pub use audit_logs_store::{new_audit_logs_store, AuditLogsStore};
pub use audit_logs_table::AuditLogsTableSchema;
pub use models::AuditLogEntry;
