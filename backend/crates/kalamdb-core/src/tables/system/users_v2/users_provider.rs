//! System.users table provider
//!
//! This module provides a DataFusion TableProvider implementation for the system.users table.
//! Uses the new EntityStore architecture with type-safe keys (UserId).

use super::super::SystemTableProviderExt;
use super::{new_username_index, new_users_store, UsernameIndex, UsernameIndexExt, UsersStore};
use crate::error::KalamDbError;
use async_trait::async_trait;
use datafusion::arrow::array::{ArrayRef, RecordBatch, StringBuilder, TimestampMillisecondArray};
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_commons::system::User;
use kalamdb_commons::UserId;
use kalamdb_store::EntityStoreV2;
use kalamdb_store::StorageBackend;
use std::any::Any;
use std::sync::Arc;

use super::UsersTableSchema;

/// System.users table provider using EntityStore architecture
pub struct UsersTableProvider {
    store: UsersStore,
    username_index: UsernameIndex,
    schema: SchemaRef,
}

impl std::fmt::Debug for UsersTableProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UsersTableProvider").finish()
    }
}

impl UsersTableProvider {
    /// Create a new users table provider
    ///
    /// # Arguments
    /// * `backend` - Storage backend (RocksDB or mock)
    ///
    /// # Returns
    /// A new UsersTableProvider instance
    pub fn new(backend: Arc<dyn StorageBackend>) -> Self {
        Self {
            store: new_users_store(backend.clone()),
            username_index: new_username_index(backend),
            schema: UsersTableSchema::schema(),
        }
    }

    /// Create a new user
    ///
    /// # Arguments
    /// * `user` - The user to create
    ///
    /// # Returns
    /// Result indicating success or failure
    pub fn create_user(&self, user: User) -> Result<(), KalamDbError> {
        // Check if username already exists (duplicate check via unique index)
        if let Some(_existing_id) = self.username_index.lookup(user.username.as_str())? {
            return Err(KalamDbError::AlreadyExists(format!(
                "User with username '{}' already exists",
                user.username.as_str()
            )));
        }

        // Store user by ID
        self.store.put(&user.id, &user)?;

        // Update username index
        self.username_index.index_user(&user)?;

        Ok(())
    }

    /// Update an existing user
    ///
    /// # Arguments
    /// * `user` - The updated user data
    ///
    /// # Returns
    /// Result indicating success or failure
    pub fn update_user(&self, user: User) -> Result<(), KalamDbError> {
        // Check if user exists
        let existing = self.store.get(&user.id)?;
        if existing.is_none() {
            return Err(KalamDbError::NotFound(format!(
                "User not found: {}",
                user.id
            )));
        }

        let existing_user = existing.unwrap();

        // If username changed, update index
        if existing_user.username != user.username {
            // Remove old username
            self
                .username_index
                .remove_user(existing_user.username.as_str())?;
            // Add new username
            self.username_index.index_user(&user)?;
        }

        // Update user
        self.store.put(&user.id, &user)?;

        Ok(())
    }

    /// Soft delete a user (sets deleted_at timestamp)
    ///
    /// # Arguments
    /// * `user_id` - The ID of the user to delete
    ///
    /// # Returns
    /// Result indicating success or failure
    pub fn delete_user(&self, user_id: &UserId) -> Result<(), KalamDbError> {
        // Get existing user
        let mut user = self
            .store
            .get(user_id)?
            .ok_or_else(|| KalamDbError::NotFound(format!("User not found: {}", user_id)))?;

        // Set deleted_at timestamp (soft delete)
        user.deleted_at = Some(chrono::Utc::now().timestamp_millis());

        // Update user with deleted_at
        self.store.put(user_id, &user)?;

        // Note: Keep username index for audit trail / recovery

        Ok(())
    }

    /// Get a user by ID
    ///
    /// # Arguments
    /// * `user_id` - The user ID to lookup
    ///
    /// # Returns
    /// Option<User> if found, None otherwise
    pub fn get_user_by_id(&self, user_id: &UserId) -> Result<Option<User>, KalamDbError> {
        Ok(self.store.get(user_id)?)
    }

    /// Get a user by username
    ///
    /// # Arguments
    /// * `username` - The username to lookup
    ///
    /// # Returns
    /// Option<User> if found, None otherwise
    pub fn get_user_by_username(&self, username: &str) -> Result<Option<User>, KalamDbError> {
        // Lookup user_id via secondary index
        let user_id = self.username_index.lookup(username)?;

        match user_id {
            Some(id) => Ok(self.store.get(&id)?),
            None => Ok(None),
        }
    }

    /// Scan all users and return as RecordBatch
    pub fn scan_all_users(&self) -> Result<RecordBatch, KalamDbError> {
        let users = self.store.scan_all()?;

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

        for (_key, user) in users {
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
    fn table_name(&self) -> &str {
        UsersTableSchema::table_name()
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
        _state: &dyn datafusion::catalog::Session,
        projection: Option<&Vec<usize>>,
        _filters: &[Expr],
        _limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        use datafusion::datasource::MemTable;
        let schema = self.schema.clone();
        let batch = self.scan_all_users().map_err(|e| {
            DataFusionError::Execution(format!("Failed to build users batch: {}", e))
        })?;
        let partitions = vec![vec![batch]];
        let table = MemTable::try_new(schema, partitions)
            .map_err(|e| DataFusionError::Execution(format!("Failed to create MemTable: {}", e)))?;
        table.scan(_state, projection, &[], _limit).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::{AuthType, Role, StorageId, StorageMode};
    use kalamdb_store::test_utils::InMemoryBackend;

    fn create_test_provider() -> UsersTableProvider {
        let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
        UsersTableProvider::new(backend)
    }

    fn create_test_user(id: &str, username: &str) -> User {
        User {
            id: UserId::new(id),
            username: UserName::new(username),
            password_hash: "hashed_password".to_string(),
            role: Role::User,
            email: Some(format!("{}@example.com", username)),
            auth_type: AuthType::Password,
            auth_data: None,
            storage_mode: StorageMode::Table,
            storage_id: Some(StorageId::new("local")),
            created_at: 1000,
            updated_at: 1000,
            last_seen: None,
            deleted_at: None,
        }
    }

    #[test]
    fn test_create_and_get_user() {
        let provider = create_test_provider();
        let user = create_test_user("user1", "alice");

        // Create user
        provider.create_user(user.clone()).unwrap();

        // Get by ID
        let retrieved = provider.get_user_by_id(&UserId::new("user1")).unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.username.as_str(), "alice");
        assert_eq!(retrieved.email, Some("alice@example.com".to_string()));
    }

    #[test]
    fn test_get_user_by_username() {
        let provider = create_test_provider();
        let user = create_test_user("user1", "alice");

        provider.create_user(user).unwrap();

        // Get by username
        let retrieved = provider.get_user_by_username("alice").unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, UserId::new("user1"));
    }

    #[test]
    fn test_update_user() {
        let provider = create_test_provider();
        let user = create_test_user("user1", "alice");

        provider.create_user(user).unwrap();

        // Update user
        let mut updated = provider
            .get_user_by_id(&UserId::new("user1"))
            .unwrap()
            .unwrap();
        updated.email = Some("newemail@example.com".to_string());
        updated.updated_at = 2000;

        provider.update_user(updated).unwrap();

        // Verify update
        let retrieved = provider
            .get_user_by_id(&UserId::new("user1"))
            .unwrap()
            .unwrap();
        assert_eq!(retrieved.email, Some("newemail@example.com".to_string()));
        assert_eq!(retrieved.updated_at, 2000);
    }

    #[test]
    fn test_update_username() {
        let provider = create_test_provider();
        let user = create_test_user("user1", "alice");

        provider.create_user(user).unwrap();

        // Update username
        let mut updated = provider
            .get_user_by_id(&UserId::new("user1"))
            .unwrap()
            .unwrap();
        updated.username = UserName::new("bob");

        provider.update_user(updated).unwrap();

        // Verify old username doesn't work
        let old_lookup = provider.get_user_by_username("alice").unwrap();
        assert!(old_lookup.is_none());

        // Verify new username works
        let new_lookup = provider.get_user_by_username("bob").unwrap();
        assert!(new_lookup.is_some());
        assert_eq!(new_lookup.unwrap().id, UserId::new("user1"));
    }

    #[test]
    fn test_delete_user() {
        let provider = create_test_provider();
        let user = create_test_user("user1", "alice");

        provider.create_user(user).unwrap();
        provider.delete_user(&UserId::new("user1")).unwrap();

        // Verify deleted_at is set
        let retrieved = provider
            .get_user_by_id(&UserId::new("user1"))
            .unwrap()
            .unwrap();
        assert!(retrieved.deleted_at.is_some());
    }

    #[test]
    fn test_scan_all_users() {
        let provider = create_test_provider();

        // Create multiple users
        for i in 1..=3 {
            let user = create_test_user(&format!("user{}", i), &format!("user{}", i));
            provider.create_user(user).unwrap();
        }

        // Scan all
        let batch = provider.scan_all_users().unwrap();
        assert_eq!(batch.num_rows(), 3);
        assert_eq!(batch.num_columns(), 11);
    }

    #[test]
    fn test_table_provider_schema() {
        let provider = create_test_provider();
        let schema = provider.schema();
        assert_eq!(schema.fields().len(), 11);
        assert_eq!(schema.field(0).name(), "user_id");
        assert_eq!(schema.field(1).name(), "username");
    }
}
