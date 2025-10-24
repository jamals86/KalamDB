//! RocksDB column family manager
//!
//! This module manages creation and deletion of RocksDB column families
//! with proper naming conventions for table isolation.

use crate::catalog::{NamespaceId, TableName, TableType};
use crate::error::KalamDbError;
use rocksdb::{Options, DB};
use std::sync::Arc;

/// Column family manager for RocksDB
///
/// Manages column families for different table types with proper naming conventions.
pub struct ColumnFamilyManager {
    db: Arc<DB>,
}

/// System column families that must be created on DB initialization
///
/// **ARCHITECTURE NOTE (Phase 2a → Phase 2b Migration):**
/// - `system_tables` is TEMPORARY - exists only for backward compatibility during migration
/// - `information_schema_tables` is the TARGET - will be the SINGLE SOURCE OF TRUTH
/// - Once Phase 2b is complete, `system_tables` CF will be removed entirely
/// - The migration path: system_tables (Phase 2a) → information_schema_tables (Phase 2b)
pub const SYSTEM_COLUMN_FAMILIES: &[&str] = &[
    "system_users",             // User management (user_id, username, email, created_at)
    "system_live_queries",      // Live query subscriptions (live_id, connection_id, table_name, query_id, user_id, query, options, created_at, updated_at, changes, node)
    "system_jobs",              // Background job tracking (job_id, job_type, table_name, status, start_time, end_time, parameters, result, trace, memory_used_mb, cpu_used_percent, node_id, error_message)
    "system_namespaces",        // Namespace metadata (namespace_id, name, created_at, options, table_count)
    "system_tables",            // TEMPORARY: Basic table metadata (will be removed in Phase 2b)
    "system_storages",          // Storage backend configurations (storage_id, storage_name, description, storage_type, base_directory, shared_tables_template, user_tables_template, created_at, updated_at)
    "information_schema_tables", // TARGET (Phase 2b): SINGLE SOURCE OF TRUTH - Complete table definitions (replaces system_tables + system_table_schemas - includes metadata + schema + columns + defaults + constraints + history)
    "user_table_counters",      // Per-user flush tracking (user_id, table_name, row_count)
];

impl ColumnFamilyManager {
    /// Create a new column family manager
    pub fn new(db: Arc<DB>) -> Self {
        Self { db }
    }

    /// Generate column family name from table metadata
    ///
    /// Naming convention:
    /// - User tables: `user_table:{namespace}:{table_name}`
    /// - Shared tables: `shared_table:{namespace}:{table_name}`
    /// - Stream tables: `stream_table:{namespace}:{table_name}`
    /// - System tables: `system_{table_name}` (e.g., system_users, system_live_queries)
    /// - User table counters: `user_table_counters` (tracks per-user flush state)
    pub fn column_family_name(
        table_type: TableType,
        namespace: Option<&NamespaceId>,
        table_name: &TableName,
    ) -> String {
        match table_type {
            TableType::System => {
                format!("system_{}", table_name.as_str())
            }
            TableType::User | TableType::Shared | TableType::Stream => {
                let namespace = namespace.expect("Non-system tables require a namespace");
                format!(
                    "{}_table:{}:{}",
                    table_type.as_str(),
                    namespace.as_str(),
                    table_name.as_str()
                )
            }
        }
    }

    /// Parse column family name to extract components
    ///
    /// Returns (table_type, namespace, table_name)
    pub fn parse_column_family_name(
        cf_name: &str,
    ) -> Result<(TableType, Option<NamespaceId>, TableName), KalamDbError> {
        // Handle special case: user_table_counters (not a regular table CF)
        if cf_name == "user_table_counters" {
            return Err(KalamDbError::CatalogError(
                "user_table_counters is a metadata CF, not a table CF".to_string(),
            ));
        }

        // Handle system tables (new naming: system_{table_name})
        if let Some(table_name) = cf_name.strip_prefix("system_") {
            return Ok((TableType::System, None, TableName::new(table_name)));
        }

        // Handle user/shared/stream tables (old naming: {type}_table:{namespace}:{table_name})
        let parts: Vec<&str> = cf_name.split(':').collect();

        if parts.is_empty() {
            return Err(KalamDbError::CatalogError(format!(
                "Invalid column family name: {}",
                cf_name
            )));
        }

        // Parse table type
        let table_type_str = parts[0].strip_suffix("_table").ok_or_else(|| {
            KalamDbError::CatalogError(format!("Invalid column family prefix: {}", parts[0]))
        })?;

        let table_type = TableType::from_str(table_type_str).ok_or_else(|| {
            KalamDbError::CatalogError(format!("Unknown table type: {}", table_type_str))
        })?;

        if parts.len() != 3 {
            return Err(KalamDbError::CatalogError(format!(
                "Invalid column family name: {}",
                cf_name
            )));
        }

        Ok((
            table_type,
            Some(NamespaceId::new(parts[1])),
            TableName::new(parts[2]),
        ))
    }

    /// Create a new column family
    pub fn create_column_family(
        &self,
        table_type: TableType,
        namespace: Option<&NamespaceId>,
        table_name: &TableName,
        _options: &Options,
    ) -> Result<(), KalamDbError> {
        let _cf_name = Self::column_family_name(table_type, namespace, table_name);

        // RocksDB's create_cf requires &mut access but Arc prevents that
        // We need to use the DB directly in the calling code instead
        // For now, this is a placeholder that will be refactored

        Err(KalamDbError::Other(
            "Column family creation must be done during DB initialization".to_string(),
        ))
    }

    /// Delete a column family
    pub fn delete_column_family(
        &self,
        table_type: TableType,
        namespace: Option<&NamespaceId>,
        table_name: &TableName,
    ) -> Result<(), KalamDbError> {
        let _cf_name = Self::column_family_name(table_type, namespace, table_name);

        // RocksDB's drop_cf requires &mut access but Arc prevents that
        // We need to use the DB directly in the calling code instead
        // For now, this is a placeholder that will be refactored

        Err(KalamDbError::Other(
            "Column family deletion must be done directly on DB".to_string(),
        ))
    }

    /// Check if a column family exists
    pub fn column_family_exists(
        &self,
        table_type: TableType,
        namespace: Option<&NamespaceId>,
        table_name: &TableName,
    ) -> bool {
        let cf_name = Self::column_family_name(table_type, namespace, table_name);
        self.db.cf_handle(&cf_name).is_some()
    }

    /// List all column families in the database
    pub fn list_column_families(&self) -> Vec<String> {
        // RocksDB doesn't provide a direct way to list CFs from an open DB
        // This would need to be tracked separately or queried from DB metadata
        // For now, return empty vector - this will be implemented when needed
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_column_family_naming() {
        // User table
        let cf_name = ColumnFamilyManager::column_family_name(
            TableType::User,
            Some(&NamespaceId::new("app")),
            &TableName::new("messages"),
        );
        assert_eq!(cf_name, "user_table:app:messages");

        // Shared table
        let cf_name = ColumnFamilyManager::column_family_name(
            TableType::Shared,
            Some(&NamespaceId::new("app")),
            &TableName::new("config"),
        );
        assert_eq!(cf_name, "shared_table:app:config");

        // Stream table
        let cf_name = ColumnFamilyManager::column_family_name(
            TableType::Stream,
            Some(&NamespaceId::new("events")),
            &TableName::new("logs"),
        );
        assert_eq!(cf_name, "stream_table:events:logs");

        // System table
        let cf_name = ColumnFamilyManager::column_family_name(
            TableType::System,
            None,
            &TableName::new("users"),
        );
        assert_eq!(cf_name, "system_users");
    }

    #[test]
    fn test_parse_column_family_name() {
        // Parse user table
        let (table_type, namespace, table_name) =
            ColumnFamilyManager::parse_column_family_name("user_table:app:messages").unwrap();
        assert_eq!(table_type, TableType::User);
        assert_eq!(namespace.unwrap().as_str(), "app");
        assert_eq!(table_name.as_str(), "messages");

        // Parse system table
        let (table_type, namespace, table_name) =
            ColumnFamilyManager::parse_column_family_name("system_users").unwrap();
        assert_eq!(table_type, TableType::System);
        assert!(namespace.is_none());
        assert_eq!(table_name.as_str(), "users");

        // Parse stream table
        let (table_type, namespace, table_name) =
            ColumnFamilyManager::parse_column_family_name("stream_table:events:logs").unwrap();
        assert_eq!(table_type, TableType::Stream);
        assert_eq!(namespace.unwrap().as_str(), "events");
        assert_eq!(table_name.as_str(), "logs");
    }

    #[test]
    fn test_parse_invalid_column_family_name() {
        assert!(ColumnFamilyManager::parse_column_family_name("invalid").is_err());
        assert!(ColumnFamilyManager::parse_column_family_name("user_table").is_err());
        assert!(ColumnFamilyManager::parse_column_family_name("unknown_table:app:test").is_err());
    }
}
