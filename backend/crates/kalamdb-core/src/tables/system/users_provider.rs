//! System.users table provider
//!
//! This module provides a DataFusion TableProvider implementation for the system.users table,
//! backed by RocksDB column family system_users.
//!
//! **Architecture Note**: This provider now uses `kalamdb-sql` for all system table operations,
//! eliminating direct RocksDB coupling from kalamdb-core.

use crate::error::KalamDbError;
use crate::tables::system::users::UsersTable;
use crate::tables::system::SystemTableProviderExt;
use async_trait::async_trait;
use datafusion::arrow::array::{ArrayRef, RecordBatch, StringBuilder, TimestampMillisecondArray};
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::Result as DataFusionResult;
use datafusion::execution::context::SessionState;
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_commons::{AuthType, Role, StorageId, StorageMode, UserId};
use kalamdb_sql::{KalamSql, User};
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::sync::Arc;

/// User data structure stored in RocksDB
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRecord {
    pub user_id: String,  //TODO: Use UserId type?
    pub username: String,
    pub email: Option<String>,
    pub created_at: i64, // timestamp in milliseconds
    pub updated_at: i64, // timestamp in milliseconds
}

/// Request structure for creating a new user (legacy, not currently used)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUserRequest {
    pub user_id: String, //TODO: Use UserId type?
    pub username: String,
    pub email: Option<String>,
    pub created_at: i64,
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
    pub fn insert_user(&self, user: CreateUserRequest) -> Result<(), KalamDbError> {
        let kalamdb_user = User {
            id: UserId::new(&user.user_id),
            username: user.username.clone(),
            password_hash: String::new(), // Empty for now, will be set by auth system
            role: Role::User,
            email: Some(user.email.clone().unwrap_or_default()),
            auth_type: AuthType::OAuth,
            auth_data: None,
            storage_mode: StorageMode::Table, // Default to table-based storage
            storage_id: None, // No specific storage preference
            created_at: user.created_at,
            updated_at: user.created_at,
            last_seen: None,
            deleted_at: None,
        };

        self.kalam_sql
            .insert_user(&kalamdb_user)
            .map_err(|e| KalamDbError::Other(format!("Failed to insert user: {}", e)))
    }

    /// Update an existing user (legacy method using UserRecord)
    pub fn update_user_legacy(&self, user: UserRecord) -> Result<(), KalamDbError> {
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

        // Preserve existing fields when updating
        let existing_user = existing.unwrap();

        let kalamdb_user = User {
            id: UserId::new(&user.user_id),
            username: user.username.clone(),
            password_hash: existing_user.password_hash.clone(), // Preserve password
            role: existing_user.role.clone(),                   // Preserve role
            email: Some(user.email.clone().unwrap_or_default()),
            auth_type: existing_user.auth_type.clone(),         // Preserve auth type
            auth_data: existing_user.auth_data.clone(),
            storage_mode: existing_user.storage_mode.clone(),   // Preserve storage mode
            storage_id: existing_user.storage_id.clone(),       // Preserve storage ID
            created_at: user.created_at,
            updated_at: chrono::Utc::now().timestamp_millis(),
            last_seen: existing_user.last_seen,
            deleted_at: None,
        };

        self.kalam_sql
            .insert_user(&kalamdb_user)
            .map_err(|e| KalamDbError::Other(format!("Failed to update user: {}", e)))
    }

    /// Delete a user (not implemented in kalamdb-sql yet)
    pub fn delete_user(&self, user_id: &UserId) -> Result<(), KalamDbError> {
        // TODO: Implement soft delete in kalamdb-sql adapter
        // For now, we can mark as deleted by updating the user
        let username = user_id.to_string();
        let user = self
            .kalam_sql
            .get_user(&username)
            .map_err(|e| KalamDbError::Other(format!("Failed to get user: {}", e)))?;

        if let Some(mut user) = user {
            user.deleted_at = Some(chrono::Utc::now().timestamp_millis());
            self.kalam_sql
                .insert_user(&user)
                .map_err(|e| KalamDbError::Other(format!("Failed to delete user: {}", e)))?;
            Ok(())
        } else {
            Err(KalamDbError::NotFound(format!(
                "User not found: {}",
                username
            )))
        }
    }

    /// Create a new user (full User model)
    pub fn create_user(&self, user: kalamdb_commons::system::User) -> Result<(), KalamDbError> {
        self.kalam_sql
            .insert_user(&user)
            .map_err(|e| KalamDbError::Other(format!("Failed to create user: {}", e)))
    }

    /// Update a user (full User model)
    pub fn update_user(&self, user: kalamdb_commons::system::User) -> Result<(), KalamDbError> {
        // Check if user exists
        let existing = self
            .kalam_sql
            .get_user(&user.username)
            .map_err(|e| KalamDbError::Other(format!("Failed to get user: {}", e)))?;

        if existing.is_none() {
            return Err(KalamDbError::NotFound(format!(
                "User not found: {}",
                user.username
            )));
        }

        // Update user
        self.kalam_sql
            .insert_user(&user)
            .map_err(|e| KalamDbError::Other(format!("Failed to update user: {}", e)))
    }

    /// Get a user by ID
    pub fn get_user_by_id(&self, user_id: &UserId) -> Result<kalamdb_commons::system::User, KalamDbError> {
        let username = user_id.to_string();
        self.kalam_sql
            .get_user(&username)
            .map_err(|e| KalamDbError::Other(format!("Failed to get user: {}", e)))?
            .ok_or_else(|| KalamDbError::NotFound(format!("User not found: {}", username)))
    }

    /// Delete a user (legacy method using string ID)
    pub fn delete_user_legacy(&self, _user_id: &str) -> Result<(), KalamDbError> {
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
            user_id: u.id.to_string(),
            username: u.username,
            email: u.email,
            created_at: u.created_at,
            updated_at: u.updated_at,
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
        let mut password_hashes = StringBuilder::new();
        let mut roles = StringBuilder::new();
        let mut emails = StringBuilder::new();
        let mut auth_types = StringBuilder::new();
        let mut auth_datas = StringBuilder::new();
        let mut created_ats = Vec::new();
        let mut updated_ats = Vec::new();
        let mut last_seens = Vec::new();
        let mut deleted_ats = Vec::new();

        for user in users {
            user_ids.append_value(user.id.as_str());
            usernames.append_value(&user.username);
            password_hashes.append_value(&user.password_hash);
            roles.append_value(user.role.as_str());
            emails.append_option(user.email.as_deref());
            auth_types.append_value(user.auth_type.as_str());
            auth_datas.append_option(user.auth_data.as_deref());
            created_ats.push(Some(user.created_at));
            updated_ats.push(Some(user.updated_at));
            last_seens.push(user.last_seen);
            deleted_ats.push(user.deleted_at);
        }

        let batch = RecordBatch::try_new(
            self.schema.clone(),
            vec![
                Arc::new(user_ids.finish()) as ArrayRef,
                Arc::new(usernames.finish()) as ArrayRef,
                Arc::new(password_hashes.finish()) as ArrayRef,
                Arc::new(roles.finish()) as ArrayRef,
                Arc::new(emails.finish()) as ArrayRef,
                Arc::new(auth_types.finish()) as ArrayRef,
                Arc::new(auth_datas.finish()) as ArrayRef,
                Arc::new(TimestampMillisecondArray::from(created_ats)) as ArrayRef,
                Arc::new(TimestampMillisecondArray::from(updated_ats)) as ArrayRef,
                Arc::new(TimestampMillisecondArray::from(last_seens)) as ArrayRef,
                Arc::new(TimestampMillisecondArray::from(deleted_ats)) as ArrayRef,
            ],
        )
        .map_err(|e| KalamDbError::Other(format!("Failed to create RecordBatch: {}", e)))?;

        Ok(batch)
    }
}

impl SystemTableProviderExt for UsersTableProvider {
    fn table_name(&self) -> &'static str {
        kalamdb_commons::constants::SystemTableNames::USERS
    }

    fn schema_ref(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn load_batch(&self) -> Result<RecordBatch, KalamDbError> {
        self.scan_all_users()
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
        self.into_memory_exec(projection)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_store::RocksDbInit;
    use tempfile::TempDir;

    fn create_test_provider() -> (UsersTableProvider, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let init = RocksDbInit::new(temp_dir.path().to_str().unwrap());
        let db = init.open().unwrap();
        let backend: Arc<dyn kalamdb_store::storage_trait::StorageBackend> =
            Arc::new(kalamdb_store::RocksDBBackend::new(db.clone()));
        let kalam_sql = Arc::new(KalamSql::new(backend).unwrap());
        let provider = UsersTableProvider::new(kalam_sql);
        (provider, temp_dir)
    }

    #[test]
    fn test_insert_and_get_user() {
        let (provider, _temp_dir) = create_test_provider();

        let user = CreateUserRequest {
            user_id: "user1".to_string(),
            username: "testuser".to_string(),
            email: Some("test@example.com".to_string()),
            created_at: 1000,
        };

        provider.insert_user(user).unwrap();

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

        let user = CreateUserRequest {
            user_id: "user1".to_string(),
            username: "testuser".to_string(),
            email: Some("test@example.com".to_string()),
            created_at: 1000,
        };

        provider.insert_user(user).unwrap();

        let mut updated_user = provider
            .get_user_by_id(&UserId::new("user1"))
            .unwrap();
        updated_user.email = Some("updated@example.com".to_string());
        updated_user.updated_at = 2000;

        provider.update_user(updated_user).unwrap();

        let retrieved = provider.get_user("testuser").unwrap().unwrap();
        assert_eq!(retrieved.username, "testuser");
        assert_eq!(retrieved.email, Some("updated@example.com".to_string()));
    }

    #[test]
    fn test_update_nonexistent_user() {
        let (provider, _temp_dir) = create_test_provider();

        let user = kalamdb_commons::system::User {
            id: UserId::new("nonexistent"),
            username: "nonexistentuser".to_string(),
            password_hash: String::new(),
            role: Role::User,
            email: None,
            auth_type: AuthType::OAuth,
            auth_data: None,
            storage_mode: StorageMode::Table,
            storage_id: None,
            created_at: 1000,
            updated_at: 1000,
            last_seen: None,
            deleted_at: None,
        };

        let result = provider.update_user(user);
        assert!(result.is_err());
    }

    #[test]
    #[ignore] // Delete not yet implemented in kalamdb-sql
    fn test_delete_user() {
        let (provider, _temp_dir) = create_test_provider();

        let user = CreateUserRequest {
            user_id: "user1".to_string(),
            username: "testuser".to_string(),
            email: None,
            created_at: 1000,
        };

        provider.insert_user(user).unwrap();
        provider.delete_user(&UserId::new("user1")).unwrap();

        let retrieved = provider.get_user("testuser").unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_scan_all_users() {
        let (provider, _temp_dir) = create_test_provider();

        let users = vec![
            CreateUserRequest {
                user_id: "user1".to_string(),
                username: "user1".to_string(),
                email: Some("user1@example.com".to_string()),
                created_at: 1000,
            },
            CreateUserRequest {
                user_id: "user2".to_string(),
                username: "user2".to_string(),
                email: None,
                created_at: 2000,
            },
        ];

        for user in users {
            provider.insert_user(user).unwrap();
        }

        let batch = provider.scan_all_users().unwrap();
        assert_eq!(batch.num_rows(), 2);
        assert_eq!(batch.num_columns(), 11);
    }

    #[test]
    fn test_table_provider_schema() {
        let (provider, _temp_dir) = create_test_provider();
        let schema = provider.schema();
        assert_eq!(schema.fields().len(), 11);
        assert_eq!(schema.field(0).name(), "user_id");
        assert_eq!(schema.field(1).name(), "username");
    }
}
