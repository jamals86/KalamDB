//! System.storage_locations table provider
//!
//! This module provides a DataFusion TableProvider implementation for the system.storage_locations table,
//! backed by RocksDB column family system_storage_locations.
//!
//! **Architecture Note**: This provider now uses `kalamdb-sql` for all system table operations,
//! eliminating direct RocksDB coupling from kalamdb-core.

use crate::catalog::StorageLocation;
use crate::error::KalamDbError;
use crate::tables::system::storage_locations::StorageLocationsTable;
use async_trait::async_trait;
use datafusion::arrow::array::{ArrayRef, RecordBatch, StringBuilder};
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::execution::context::SessionState;
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_sql::KalamSql;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::sync::Arc;

/// Storage location data structure stored in RocksDB
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageLocationRecord {
    pub location_name: String,
    pub location_type: String, // "Filesystem" or "S3"
    pub path: String,
    pub credentials_ref: Option<String>,
    pub usage_count: u32,
    pub created_at: i64, // timestamp in milliseconds
    pub updated_at: i64, // timestamp in milliseconds
}

impl From<StorageLocation> for StorageLocationRecord {
    fn from(loc: StorageLocation) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            location_name: loc.location_name,
            location_type: format!("{:?}", loc.location_type),
            path: loc.path,
            credentials_ref: loc.credentials_ref,
            usage_count: loc.usage_count,
            created_at: now,
            updated_at: now,
        }
    }
}

impl From<StorageLocationRecord> for StorageLocation {
    fn from(rec: StorageLocationRecord) -> Self {
        use crate::catalog::LocationType;

        let location_type = match rec.location_type.as_str() {
            "Filesystem" => LocationType::Filesystem,
            "S3" => LocationType::S3,
            _ => LocationType::Filesystem, // Default
        };

        Self {
            location_name: rec.location_name,
            location_type,
            path: rec.path,
            credentials_ref: rec.credentials_ref,
            usage_count: rec.usage_count,
        }
    }
}

/// System.storage_locations table provider backed by RocksDB
pub struct StorageLocationsTableProvider {
    kalam_sql: Arc<KalamSql>,
    schema: SchemaRef,
}

impl StorageLocationsTableProvider {
    /// Create a new storage locations table provider
    pub fn new(kalam_sql: Arc<KalamSql>) -> Self {
        Self {
            kalam_sql,
            schema: StorageLocationsTable::schema(),
        }
    }

    /// Insert a new storage location
    pub fn insert_location(&self, location: StorageLocationRecord) -> Result<(), KalamDbError> {
        // Validate location name format
        StorageLocation::validate_name(&location.location_name)?;

        // Check if location already exists via kalamdb-sql
        let existing = self
            .kalam_sql
            .get_storage_location(&location.location_name)
            .map_err(|e| {
                KalamDbError::Other(format!("Failed to check existing location: {}", e))
            })?;

        if existing.is_some() {
            return Err(KalamDbError::AlreadyExists(format!(
                "Storage location already exists: {}",
                location.location_name
            )));
        }

        // Convert to kalamdb-sql model and insert
        let sql_location = kalamdb_sql::StorageLocation {
            location_name: location.location_name.clone(),
            location_type: location.location_type.clone(),
            path: location.path.clone(),
            credentials_ref: location.credentials_ref.clone(),
            usage_count: location.usage_count as i32,
            created_at: chrono::Utc::now().timestamp(),
            updated_at: chrono::Utc::now().timestamp(),
        };

        self.kalam_sql
            .insert_storage_location(&sql_location)
            .map_err(|e| KalamDbError::Other(format!("Failed to insert storage location: {}", e)))
    }

    /// Update an existing storage location
    pub fn update_location(&self, location: StorageLocationRecord) -> Result<(), KalamDbError> {
        // Check if location exists
        let existing = self
            .kalam_sql
            .get_storage_location(&location.location_name)
            .map_err(|e| {
                KalamDbError::Other(format!("Failed to check existing location: {}", e))
            })?;

        if existing.is_none() {
            return Err(KalamDbError::NotFound(format!(
                "Storage location not found: {}",
                location.location_name
            )));
        }

        // Convert and update
        let sql_location = kalamdb_sql::StorageLocation {
            location_name: location.location_name.clone(),
            location_type: location.location_type.clone(),
            path: location.path.clone(),
            credentials_ref: location.credentials_ref.clone(),
            usage_count: location.usage_count as i32,
            created_at: chrono::Utc::now().timestamp(),
            updated_at: chrono::Utc::now().timestamp(),
        };

        self.kalam_sql
            .insert_storage_location(&sql_location)
            .map_err(|e| KalamDbError::Other(format!("Failed to update storage location: {}", e)))
    }

    /// Delete a storage location
    pub fn delete_location(&self, location_name: &str) -> Result<(), KalamDbError> {
        // Check if location exists and get usage count
        let existing = self
            .kalam_sql
            .get_storage_location(location_name)
            .map_err(|e| KalamDbError::Other(format!("Failed to get storage location: {}", e)))?;

        match existing {
            Some(loc) => {
                if loc.usage_count > 0 {
                    return Err(KalamDbError::InvalidOperation(format!(
                        "Cannot delete storage location '{}' because it is referenced by {} table(s)",
                        location_name, loc.usage_count
                    )));
                }
                self.kalam_sql
                    .delete_storage_location(location_name)
                    .map_err(|e| {
                        KalamDbError::Other(format!("Failed to delete storage location: {}", e))
                    })?;
                Ok(())
            }
            None => Err(KalamDbError::NotFound(format!(
                "Storage location not found: {}",
                location_name
            ))),
        }
    }

    /// Get a storage location by name
    pub fn get_location(
        &self,
        location_name: &str,
    ) -> Result<Option<StorageLocationRecord>, KalamDbError> {
        let sql_location = self
            .kalam_sql
            .get_storage_location(location_name)
            .map_err(|e| KalamDbError::Other(format!("Failed to get storage location: {}", e)))?;

        Ok(sql_location.map(|loc| StorageLocationRecord {
            location_name: loc.location_name,
            location_type: loc.location_type, // Already a String
            path: loc.path,
            credentials_ref: loc.credentials_ref,
            usage_count: loc.usage_count as u32,
            created_at: loc.created_at,
            updated_at: loc.updated_at,
        }))
    }

    /// Increment usage count for a location
    pub fn increment_usage(&self, location_name: &str) -> Result<(), KalamDbError> {
        let mut location = self.get_location(location_name)?.ok_or_else(|| {
            KalamDbError::NotFound(format!("Storage location not found: {}", location_name))
        })?;

        location.usage_count += 1;

        // Convert back to kalamdb-sql model and update
        let sql_location = kalamdb_sql::StorageLocation {
            location_name: location.location_name,
            location_type: location.location_type,
            path: location.path,
            credentials_ref: location.credentials_ref,
            usage_count: location.usage_count as i32,
            created_at: location.created_at,
            updated_at: chrono::Utc::now().timestamp(),
        };

        self.kalam_sql
            .insert_storage_location(&sql_location)
            .map_err(|e| KalamDbError::Other(format!("Failed to increment usage: {}", e)))
    }

    /// Decrement usage count for a location
    pub fn decrement_usage(&self, location_name: &str) -> Result<(), KalamDbError> {
        let mut location = self.get_location(location_name)?.ok_or_else(|| {
            KalamDbError::NotFound(format!("Storage location not found: {}", location_name))
        })?;

        if location.usage_count > 0 {
            location.usage_count -= 1;
        }

        // Convert back to kalamdb-sql model and update
        let sql_location = kalamdb_sql::StorageLocation {
            location_name: location.location_name,
            location_type: location.location_type,
            path: location.path,
            credentials_ref: location.credentials_ref,
            usage_count: location.usage_count as i32,
            created_at: location.created_at,
            updated_at: chrono::Utc::now().timestamp(),
        };

        self.kalam_sql
            .insert_storage_location(&sql_location)
            .map_err(|e| KalamDbError::Other(format!("Failed to decrement usage: {}", e)))
    }

    /// Scan all storage locations and return as RecordBatch
    pub fn scan_all_locations(&self) -> Result<RecordBatch, KalamDbError> {
        let locations = self
            .kalam_sql
            .scan_all_storage_locations()
            .map_err(|e| KalamDbError::Other(format!("Failed to scan storage locations: {}", e)))?;

        let mut location_names = StringBuilder::new();
        let mut location_types = StringBuilder::new();
        let mut paths = StringBuilder::new();
        let mut credentials_refs = StringBuilder::new();
        let mut usage_counts = Vec::new();
        let mut created_ats = Vec::new();
        let mut updated_ats = Vec::new();

        for location in locations {
            location_names.append_value(&location.location_name);
            location_types.append_value(&location.location_type);
            paths.append_value(&location.path);
            if let Some(creds) = &location.credentials_ref {
                credentials_refs.append_value(creds);
            } else {
                credentials_refs.append_null();
            }
            usage_counts.push(Some(location.usage_count as i64));
            created_ats.push(Some(location.created_at));
            updated_ats.push(Some(location.updated_at));
        }

        let batch = RecordBatch::try_new(
            self.schema.clone(),
            vec![
                Arc::new(location_names.finish()) as ArrayRef,
                Arc::new(location_types.finish()) as ArrayRef,
                Arc::new(paths.finish()) as ArrayRef,
                Arc::new(credentials_refs.finish()) as ArrayRef,
                Arc::new(datafusion::arrow::array::Int64Array::from(usage_counts)) as ArrayRef,
                Arc::new(datafusion::arrow::array::TimestampMillisecondArray::from(
                    created_ats,
                )) as ArrayRef,
                Arc::new(datafusion::arrow::array::TimestampMillisecondArray::from(
                    updated_ats,
                )) as ArrayRef,
            ],
        )
        .map_err(|e| KalamDbError::Other(format!("Failed to create RecordBatch: {}", e)))?;

        Ok(batch)
    }
}

#[async_trait]
impl TableProvider for StorageLocationsTableProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn table_type(&self) -> TableType {
        TableType::Base
    }

    async fn scan(
        &self,
        _state: &SessionState,
        _projection: Option<&Vec<usize>>,
        _filters: &[Expr],
        _limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        // For now, we'll return an error indicating that scanning is not yet implemented
        // This will be implemented in a future task with proper ExecutionPlan
        Err(DataFusionError::NotImplemented(
            "System.storage_locations table scanning not yet implemented. Use get_location() or scan_all_locations() methods instead.".to_string()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::RocksDbInit;
    use tempfile::TempDir;

    fn setup_test_provider() -> (StorageLocationsTableProvider, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let init = RocksDbInit::new(temp_dir.path().to_str().unwrap());
        let db = init.open().unwrap();
        let kalam_sql = Arc::new(KalamSql::new(db).unwrap());
        let provider = StorageLocationsTableProvider::new(kalam_sql);
        (provider, temp_dir)
    }

    #[test]
    fn test_insert_location() {
        let (provider, _temp_dir) = setup_test_provider();

        let now = chrono::Utc::now().timestamp_millis();
        let location = StorageLocationRecord {
            location_name: "local_disk".to_string(),
            location_type: "Filesystem".to_string(),
            path: "/data/tables".to_string(),
            credentials_ref: None,
            usage_count: 0,
            created_at: now,
            updated_at: now,
        };

        provider.insert_location(location.clone()).unwrap();

        let retrieved = provider.get_location("local_disk").unwrap().unwrap();
        assert_eq!(retrieved.location_name, "local_disk");
        assert_eq!(retrieved.path, "/data/tables");
    }

    #[test]
    fn test_insert_duplicate_location() {
        let (provider, _temp_dir) = setup_test_provider();

        let now = chrono::Utc::now().timestamp_millis();
        let location = StorageLocationRecord {
            location_name: "local_disk".to_string(),
            location_type: "Filesystem".to_string(),
            path: "/data/tables".to_string(),
            credentials_ref: None,
            usage_count: 0,
            created_at: now,
            updated_at: now,
        };

        provider.insert_location(location.clone()).unwrap();
        let result = provider.insert_location(location);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            KalamDbError::AlreadyExists(_)
        ));
    }

    #[test]
    fn test_update_location() {
        let (provider, _temp_dir) = setup_test_provider();

        let now = chrono::Utc::now().timestamp_millis();
        let location = StorageLocationRecord {
            location_name: "local_disk".to_string(),
            location_type: "Filesystem".to_string(),
            path: "/data/tables".to_string(),
            credentials_ref: None,
            usage_count: 0,
            created_at: now,
            updated_at: now,
        };

        provider.insert_location(location).unwrap();

        let updated = StorageLocationRecord {
            location_name: "local_disk".to_string(),
            location_type: "Filesystem".to_string(),
            path: "/new/path".to_string(),
            credentials_ref: Some("creds123".to_string()),
            usage_count: 0,
            created_at: now,
            updated_at: now + 1000,
        };

        provider.update_location(updated).unwrap();

        let retrieved = provider.get_location("local_disk").unwrap().unwrap();
        assert_eq!(retrieved.path, "/new/path");
        assert_eq!(retrieved.credentials_ref, Some("creds123".to_string()));
    }

    #[test]
    #[ignore] // Delete not yet implemented in kalamdb-sql adapter
    fn test_delete_location() {
        let (provider, _temp_dir) = setup_test_provider();

        let now = chrono::Utc::now().timestamp_millis();
        let location = StorageLocationRecord {
            location_name: "local_disk".to_string(),
            location_type: "Filesystem".to_string(),
            path: "/data/tables".to_string(),
            credentials_ref: None,
            usage_count: 0,
            created_at: now,
            updated_at: now,
        };

        provider.insert_location(location).unwrap();
        provider.delete_location("local_disk").unwrap();

        let retrieved = provider.get_location("local_disk").unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    #[ignore] // Delete not yet implemented in kalamdb-sql adapter
    fn test_delete_location_with_usage() {
        let (provider, _temp_dir) = setup_test_provider();

        let now = chrono::Utc::now().timestamp_millis();
        let location = StorageLocationRecord {
            location_name: "local_disk".to_string(),
            location_type: "Filesystem".to_string(),
            path: "/data/tables".to_string(),
            credentials_ref: None,
            usage_count: 2,
            created_at: now,
            updated_at: now,
        };

        provider.insert_location(location).unwrap();
        let result = provider.delete_location("local_disk");

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            KalamDbError::InvalidOperation(_)
        ));
    }

    #[test]
    fn test_increment_usage() {
        let (provider, _temp_dir) = setup_test_provider();

        let now = chrono::Utc::now().timestamp_millis();
        let location = StorageLocationRecord {
            location_name: "local_disk".to_string(),
            location_type: "Filesystem".to_string(),
            path: "/data/tables".to_string(),
            credentials_ref: None,
            usage_count: 0,
            created_at: now,
            updated_at: now,
        };

        provider.insert_location(location).unwrap();
        provider.increment_usage("local_disk").unwrap();

        let retrieved = provider.get_location("local_disk").unwrap().unwrap();
        assert_eq!(retrieved.usage_count, 1);
    }

    #[test]
    fn test_decrement_usage() {
        let (provider, _temp_dir) = setup_test_provider();

        let now = chrono::Utc::now().timestamp_millis();
        let location = StorageLocationRecord {
            location_name: "local_disk".to_string(),
            location_type: "Filesystem".to_string(),
            path: "/data/tables".to_string(),
            credentials_ref: None,
            usage_count: 2,
            created_at: now,
            updated_at: now,
        };

        provider.insert_location(location).unwrap();
        provider.decrement_usage("local_disk").unwrap();

        let retrieved = provider.get_location("local_disk").unwrap().unwrap();
        assert_eq!(retrieved.usage_count, 1);
    }

    #[test]
    fn test_scan_all_locations() {
        let (provider, _temp_dir) = setup_test_provider();

        let now = chrono::Utc::now().timestamp_millis();
        let location1 = StorageLocationRecord {
            location_name: "local_disk".to_string(),
            location_type: "Filesystem".to_string(),
            path: "/data/tables".to_string(),
            credentials_ref: None,
            usage_count: 0,
            created_at: now,
            updated_at: now,
        };

        let location2 = StorageLocationRecord {
            location_name: "s3_bucket".to_string(),
            location_type: "S3".to_string(),
            path: "s3://bucket/tables".to_string(),
            credentials_ref: Some("aws_creds".to_string()),
            usage_count: 3,
            created_at: now,
            updated_at: now,
        };

        provider.insert_location(location1).unwrap();
        provider.insert_location(location2).unwrap();

        let batch = provider.scan_all_locations().unwrap();
        assert_eq!(batch.num_rows(), 2);
        assert_eq!(batch.num_columns(), 7); // Updated from 5 to 7
    }

    #[test]
    fn test_invalid_location_name() {
        let (provider, _temp_dir) = setup_test_provider();

        let now = chrono::Utc::now().timestamp_millis();
        let location = StorageLocationRecord {
            location_name: "Invalid-Name".to_string(),
            location_type: "Filesystem".to_string(),
            path: "/data/tables".to_string(),
            credentials_ref: None,
            usage_count: 0,
            created_at: now,
            updated_at: now,
        };

        let result = provider.insert_location(location);
        assert!(result.is_err());
    }
}
