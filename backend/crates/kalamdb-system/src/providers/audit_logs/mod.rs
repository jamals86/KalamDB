//! System.audit_log table module
//!
//! This module provides the complete implementation for the system.audit_log table:
//! - DataFusion TableProvider implementation (audit_logs_provider.rs)
//!
//! ## Architecture
//!
//! Follows the EntityStore pattern established by users:
//! 1. Schema source of truth on `AuditLogEntry::definition()`
//! 2. DataFusion integration for SQL queries

pub mod audit_logs_provider;
pub mod models;

pub use audit_logs_provider::AuditLogsTableProvider;
pub use models::AuditLogEntry;
