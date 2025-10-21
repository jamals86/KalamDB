//! System.users table provider
//!
//! This module provides a DataFusion TableProvider implementation for the system.users table,
//! backed by RocksDB column family system_users.
//!
//! **Architecture Note**: This provider now uses `kalamdb-sql` for all system table operations,
//! eliminating direct RocksDB coupling from kalamdb-core.

use crate::error::KalamDbError;
use crate::tables::system::users::UsersTable;
use async_trait::async_trait;
use datafusion::arrow::array::{ArrayRef, RecordBatch, StringBuilder, TimestampMillisecondArray};
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::execution::context::SessionState;
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::memory::MemoryExec;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_sql::{KalamSql, User};
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
    kalam_sql: Arc<KalamSql>,
    schema: SchemaRef,
}

impl UsersTableProvider {
    /// Create a new users table provider
    pub fn new(kalam_sql: Arc<KalamSql>) -> Self {
        Self {
            kalam_sql,
            schema: UsersTable::schema(),
        }
    }

    /// Insert a new user
    pub fn insert_user(&self, user: UserRecord) -> Result<(), KalamDbError> {
        let kalamdb_user = User {
            user_id: user.user_id.clone(),
            username: user.username.clone(),
            email: user.email.clone().unwrap_or_default(),
            created_at: user.created_at,
        };

        self.kalam_sql
            .insert_user(&kalamdb_user)
            .map_err(|e| KalamDbError::Other(format!("Failed to insert user: {}", e)))
    }

    /// Update an existing user
    pub fn update_user(&self, user: UserRecord) -> Result<(), KalamDbError> {
        // Check if user exists
        let existing = self
            .kalam_sql
            .get_user(&user.username)
            .map_err(|e| KalamDbError::Other(format!("Failed to get user: {}", e)))?;

        if existing.is_none() {
            return Err(KalamDbError::NotFound(format!(
                "User not found: {}",
                user.user_id
            )));
        }

        let kalamdb_user = User {
            user_id: user.user_id.clone(),
            username: user.username.clone(),
            email: user.email.clone().unwrap_or_default(),
            created_at: user.created_at,
        };

        self.kalam_sql
            .insert_user(&kalamdb_user)
            .map_err(|e| KalamDbError::Other(format!("Failed to update user: {}", e)))
    }

    /// Delete a user (not implemented in kalamdb-sql yet)
    pub fn delete_user(&self, _user_id: &str) -> Result<(), KalamDbError> {
        // TODO: Implement delete in kalamdb-sql adapter
        Err(KalamDbError::Other(
            "Delete user not yet implemented in kalamdb-sql".to_string(),
        ))
    }

    /// Get a user by username (kalamdb-sql uses username as key)
    pub fn get_user(&self, username: &str) -> Result<Option<UserRecord>, KalamDbError> {
        let user = self
            .kalam_sql
            .get_user(username)
            .map_err(|e| KalamDbError::Other(format!("Failed to get user: {}", e)))?;

        Ok(user.map(|u| UserRecord {
            user_id: u.user_id,
            username: u.username,
            email: Some(u.email),
            created_at: u.created_at,
            updated_at: u.created_at, // kalamdb-sql doesn't have updated_at yet
        }))
    }

    /// Scan all users and return as RecordBatch
    pub fn scan_all_users(&self) -> Result<RecordBatch, KalamDbError> {
        let users = self
            .kalam_sql
            .scan_all_users()
            .map_err(|e| KalamDbError::Other(format!("Failed to scan users: {}", e)))?;

        let mut user_ids = StringBuilder::new();
        let mut usernames = StringBuilder::new();
        let mut emails = StringBuilder::new();
        let mut created_ats = Vec::new();
        let mut updated_ats = Vec::new();

        for user in users {
            user_ids.append_value(&user.user_id);
            usernames.append_value(&user.username);
            emails.append_value(&user.email);
            created_ats.push(Some(user.created_at));
            updated_ats.push(Some(user.created_at)); // using created_at for both since updated_at not in model
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
        projection: Option<&Vec<usize>>,
        _filters: &[Expr],
        _limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        let batch = self.scan_all_users().map_err(|e| {
            DataFusionError::Execution(format!("Failed to load system.users records: {}", e))
        })?;

        let partitions = vec![vec![batch]];
        let exec = MemoryExec::try_new(&partitions, self.schema.clone(), projection.cloned())
            .map_err(|e| {
                DataFusionError::Execution(format!("Failed to build MemoryExec: {}", e))
            })?;

        Ok(Arc::new(exec))
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
        let kalam_sql = Arc::new(KalamSql::new(db).unwrap());
        let provider = UsersTableProvider::new(kalam_sql);
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

        let retrieved = provider.get_user("testuser").unwrap(); // Use username as key
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
            username: "testuser".to_string(),
            email: Some("updated@example.com".to_string()),
            created_at: 1000,
            updated_at: 2000,
        };

        provider.update_user(updated_user).unwrap();

        let retrieved = provider.get_user("testuser").unwrap().unwrap();
        assert_eq!(retrieved.username, "testuser");
        assert_eq!(retrieved.email, Some("updated@example.com".to_string()));
    }

    #[test]
    fn test_update_nonexistent_user() {
        let (provider, _temp_dir) = create_test_provider();

        let user = UserRecord {
            user_id: "nonexistent".to_string(),
            username: "nonexistentuser".to_string(),
            email: None,
            created_at: 1000,
            updated_at: 1000,
        };

        let result = provider.update_user(user);
        assert!(result.is_err());
    }

    #[test]
    #[ignore] // Delete not yet implemented in kalamdb-sql
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

        let retrieved = provider.get_user("testuser").unwrap();
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
