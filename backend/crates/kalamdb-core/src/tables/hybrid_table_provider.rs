//! Hybrid table provider that queries both RocksDB and Parquet
//!
//! This provider combines hot data from RocksDB with cold data from Parquet files.

use crate::catalog::TableMetadata;
use crate::storage::{RocksDBBackend, StorageBackend};
use datafusion::arrow::datatypes::SchemaRef;
use std::sync::Arc;

/// Hybrid table provider that queries both RocksDB and Parquet (placeholder)
///
/// Full DataFusion TableProvider implementation will be added when needed
pub struct HybridTableProvider {
    /// Table metadata
    table_metadata: TableMetadata,

    /// Arrow schema
    schema: SchemaRef,

    /// Storage backend
    _backend: Arc<dyn StorageBackend>,

    /// Parquet file paths (cold storage)
    _parquet_paths: Vec<String>,
}

impl HybridTableProvider {
    /// Create a new hybrid table provider
    pub fn new(
        table_metadata: TableMetadata,
        schema: SchemaRef,
        backend: Arc<dyn StorageBackend>,
        parquet_paths: Vec<String>,
    ) -> Self {
        Self {
            table_metadata,
            schema,
            _backend: backend,
            _parquet_paths: parquet_paths,
        }
    }

    // Convenience constructor for RocksDB-backed backend
    pub fn from_rocks_backend(
        table_metadata: TableMetadata,
        schema: SchemaRef,
        backend: RocksDBBackend,
        parquet_paths: Vec<String>,
    ) -> Self {
        Self::new(table_metadata, schema, Arc::new(backend), parquet_paths)
    }

    /// Get the column family name for this table
    pub fn column_family_name(&self) -> String {
        self.table_metadata.column_family_name()
    }

    /// Get the schema
    pub fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{NamespaceId, TableName, TableType};
    use crate::flush::FlushPolicy;
    use kalamdb_store::{RocksDbInit, RocksDBBackend};
    use chrono::Utc;
    use datafusion::arrow::datatypes::{DataType, Field, Schema};
    use std::env;
    use std::fs;

    #[test]
    fn test_hybrid_table_provider_creation() {
        let temp_dir = env::temp_dir().join("kalamdb_hybrid_provider_test");
        let _ = fs::remove_dir_all(&temp_dir);

        let init = RocksDbInit::new(temp_dir.to_str().unwrap());
        let db = init.open().unwrap();

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("content", DataType::Utf8, true),
        ]));

        let table_metadata = TableMetadata {
            table_name: TableName::new("test_table"),
            table_type: TableType::User,
            namespace: NamespaceId::new("test_ns"),
            created_at: Utc::now(),
            storage_location: "/data/test".to_string(),
            flush_policy: FlushPolicy::row_limit(1000).unwrap(),
            schema_version: 1,
            deleted_retention_hours: Some(720),
        };

        let backend = RocksDBBackend::new(db.clone());
        let provider =
            HybridTableProvider::from_rocks_backend(table_metadata, schema.clone(), backend, vec![]);

        assert_eq!(provider.schema(), schema);

        RocksDbInit::close(db);
        let _ = fs::remove_dir_all(temp_dir);
    }
}
