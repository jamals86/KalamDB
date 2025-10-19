//! System.users table provider
//!
//! This module provides a DataFusion TableProvider implementation for the system.users table,
//! backed by RocksDB column family system_users.

use crate::catalog::{CatalogStore, UserId};
use crate::error::KalamDbError;
use crate::tables::system::users::UsersTable;
use async_trait::async_trait;
use datafusion::arrow::array::{ArrayRef, RecordBatch, StringBuilder, TimestampMillisecondArray};
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::execution::context::SessionState;
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::sync::Arc;

/// User data structure stored in RocksDB
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRecord {
    pub user_id: String,
    pub username: String,
    pub email: Option<String>,
    pub created_at: i64, // timestamp in milliseconds
    pub updated_at: i64, // timestamp in milliseconds
}

/// System.users table provider backed by RocksDB
pub struct UsersTableProvider {
    catalog_store: Arc<CatalogStore>,
    schema: SchemaRef,
}

impl UsersTableProvider {
    /// Create a new users table provider
    pub fn new(catalog_store: Arc<CatalogStore>) -> Self {
        Self {
            catalog_store,
            schema: UsersTable::schema(),
        }
    }

    /// Insert a new user
    pub fn insert_user(&self, user: UserRecord) -> Result<(), KalamDbError> {
        let user_id = UserId::new(user.user_id.clone());
        self.catalog_store.put_user(&user_id, &user)
    }

    /// Update an existing user
    pub fn update_user(&self, user: UserRecord) -> Result<(), KalamDbError> {
        let user_id = UserId::new(user.user_id.clone());
        
        // Check if user exists
        let existing: Option<UserRecord> = self.catalog_store.get_user(&user_id)?;
        if existing.is_none() {
            return Err(KalamDbError::NotFound(format!(
                "User not found: {}",
                user.user_id
            )));
        }
        
        self.catalog_store.put_user(&user_id, &user)
    }

    /// Delete a user
    pub fn delete_user(&self, user_id: &str) -> Result<(), KalamDbError> {
        let user_id = UserId::new(user_id.to_string());
        self.catalog_store.delete_user(&user_id)
    }

    /// Get a user by ID
    pub fn get_user(&self, user_id: &str) -> Result<Option<UserRecord>, KalamDbError> {
        let user_id = UserId::new(user_id.to_string());
        self.catalog_store.get_user(&user_id)
    }

    /// Scan all users and return as RecordBatch
    pub fn scan_all_users(&self) -> Result<RecordBatch, KalamDbError> {
        let iter = self.catalog_store.iter("users")?;
        
        let mut user_ids = StringBuilder::new();
        let mut usernames = StringBuilder::new();
        let mut emails = StringBuilder::new();
        let mut created_ats = Vec::new();
        let mut updated_ats = Vec::new();
        
        for (_key, value) in iter {
            let user: UserRecord = serde_json::from_slice(&value)
                .map_err(|e| KalamDbError::SerializationError(format!(
                    "Failed to deserialize user: {}",
                    e
                )))?;
            
            user_ids.append_value(&user.user_id);
            usernames.append_value(&user.username);
            if let Some(email) = &user.email {
                emails.append_value(email);
            } else {
                emails.append_null();
            }
            created_ats.push(Some(user.created_at));
            updated_ats.push(Some(user.updated_at));
        }
        
        let batch = RecordBatch::try_new(
            self.schema.clone(),
            vec![
                Arc::new(user_ids.finish()) as ArrayRef,
                Arc::new(usernames.finish()) as ArrayRef,
                Arc::new(emails.finish()) as ArrayRef,
                Arc::new(TimestampMillisecondArray::from(created_ats)) as ArrayRef,
                Arc::new(TimestampMillisecondArray::from(updated_ats)) as ArrayRef,
            ],
        )
        .map_err(|e| KalamDbError::Other(format!("Failed to create RecordBatch: {}", e)))?;
        
        Ok(batch)
    }
}

#[async_trait]
impl TableProvider for UsersTableProvider {
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
            "System.users table scanning not yet implemented. Use get_user() or scan_all_users() methods instead.".to_string()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::RocksDbInit;
    use tempfile::TempDir;

    fn create_test_provider() -> (UsersTableProvider, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let init = RocksDbInit::new(temp_dir.path().to_str().unwrap());
        let db = init.open().unwrap();
        let catalog_store = Arc::new(CatalogStore::new(db));
        let provider = UsersTableProvider::new(catalog_store);
        (provider, temp_dir)
    }

    #[test]
    fn test_insert_and_get_user() {
        let (provider, _temp_dir) = create_test_provider();
        
        let user = UserRecord {
            user_id: "user1".to_string(),
            username: "testuser".to_string(),
            email: Some("test@example.com".to_string()),
            created_at: 1000,
            updated_at: 1000,
        };
        
        provider.insert_user(user.clone()).unwrap();
        
        let retrieved = provider.get_user("user1").unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.user_id, "user1");
        assert_eq!(retrieved.username, "testuser");
        assert_eq!(retrieved.email, Some("test@example.com".to_string()));
    }

    #[test]
    fn test_update_user() {
        let (provider, _temp_dir) = create_test_provider();
        
        let user = UserRecord {
            user_id: "user1".to_string(),
            username: "testuser".to_string(),
            email: Some("test@example.com".to_string()),
            created_at: 1000,
            updated_at: 1000,
        };
        
        provider.insert_user(user.clone()).unwrap();
        
        let updated_user = UserRecord {
            user_id: "user1".to_string(),
            username: "updateduser".to_string(),
            email: Some("updated@example.com".to_string()),
            created_at: 1000,
            updated_at: 2000,
        };
        
        provider.update_user(updated_user).unwrap();
        
        let retrieved = provider.get_user("user1").unwrap().unwrap();
        assert_eq!(retrieved.username, "updateduser");
        assert_eq!(retrieved.email, Some("updated@example.com".to_string()));
    }

    #[test]
    fn test_update_nonexistent_user() {
        let (provider, _temp_dir) = create_test_provider();
        
        let user = UserRecord {
            user_id: "nonexistent".to_string(),
            username: "testuser".to_string(),
            email: None,
            created_at: 1000,
            updated_at: 1000,
        };
        
        let result = provider.update_user(user);
        assert!(result.is_err());
    }

    #[test]
    fn test_delete_user() {
        let (provider, _temp_dir) = create_test_provider();
        
        let user = UserRecord {
            user_id: "user1".to_string(),
            username: "testuser".to_string(),
            email: None,
            created_at: 1000,
            updated_at: 1000,
        };
        
        provider.insert_user(user).unwrap();
        provider.delete_user("user1").unwrap();
        
        let retrieved = provider.get_user("user1").unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_scan_all_users() {
        let (provider, _temp_dir) = create_test_provider();
        
        let users = vec![
            UserRecord {
                user_id: "user1".to_string(),
                username: "user1".to_string(),
                email: Some("user1@example.com".to_string()),
                created_at: 1000,
                updated_at: 1000,
            },
            UserRecord {
                user_id: "user2".to_string(),
                username: "user2".to_string(),
                email: None,
                created_at: 2000,
                updated_at: 2000,
            },
        ];
        
        for user in users {
            provider.insert_user(user).unwrap();
        }
        
        let batch = provider.scan_all_users().unwrap();
        assert_eq!(batch.num_rows(), 2);
        assert_eq!(batch.num_columns(), 5);
    }

    #[test]
    fn test_table_provider_schema() {
        let (provider, _temp_dir) = create_test_provider();
        let schema = provider.schema();
        assert_eq!(schema.fields().len(), 5);
        assert_eq!(schema.field(0).name(), "user_id");
        assert_eq!(schema.field(1).name(), "username");
    }
}
