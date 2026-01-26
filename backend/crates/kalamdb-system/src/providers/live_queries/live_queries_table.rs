//! System.live_queries table schema
//!
//! This module defines the schema for the system.live_queries table.
//! - TableDefinition: Source of truth for columns, types, comments
//! - Arrow schema: Derived from TableDefinition, memoized via OnceLock

use datafusion::arrow::datatypes::SchemaRef;
use crate::providers::live_queries::models::LiveQuery;
use kalamdb_commons::schemas::TableDefinition;
use kalamdb_commons::SystemTable;
use std::sync::OnceLock;

/// Schema provider for system.live_queries table
///
/// Provides typed access to the table definition and Arrow schema.
/// Contains the full TableDefinition as the single source of truth.
#[derive(Debug, Clone, Copy)]
pub struct LiveQueriesTableSchema;

impl LiveQueriesTableSchema {
    /// Get the TableDefinition for system.live_queries
    ///
    /// This is the single source of truth for:
    /// - Column definitions (names, types, nullability)
    /// - Column ordering (ordinal_position)
    /// - Column comments/descriptions
    ///
    /// Schema:
    /// - live_id TEXT PRIMARY KEY
    /// - connection_id TEXT NOT NULL
    /// - subscription_id TEXT NOT NULL
    /// - namespace_id TEXT NOT NULL
    /// - table_name TEXT NOT NULL
    /// - user_id TEXT NOT NULL
    /// - query TEXT NOT NULL
    /// - options JSON (nullable, JSON)
    /// - status TEXT NOT NULL
    /// - created_at TIMESTAMP NOT NULL
    /// - last_update TIMESTAMP NOT NULL
    /// - changes BIGINT NOT NULL
    /// - node_id BIGINT NOT NULL (node identifier)
    /// - last_ping_at TIMESTAMP NOT NULL (for stale detection)
    ///
    /// Note: last_seq_id is tracked in-memory only (WebSocketSession), not persisted
    pub fn definition() -> TableDefinition {
        LiveQuery::definition()
    }

    /// Get the cached Arrow schema for system.live_queries table
    pub fn schema() -> SchemaRef {
        static SCHEMA: OnceLock<SchemaRef> = OnceLock::new();
        SCHEMA
            .get_or_init(|| {
                Self::definition()
                    .to_arrow_schema()
                    .expect("Failed to convert live_queries TableDefinition to Arrow schema")
            })
            .clone()
    }

    /// Get the table name
    pub fn table_name() -> &'static str {
        SystemTable::LiveQueries.table_name()
    }

    /// Get the column family name in RocksDB
    pub fn column_family_name() -> &'static str {
        SystemTable::LiveQueries
            .column_family_name()
            .expect("LiveQueries is a table, not a view")
    }

    /// Get the partition key for storage
    pub fn partition() -> &'static str {
        Self::column_family_name()
    }
}
