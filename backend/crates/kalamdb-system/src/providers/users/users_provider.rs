//! System.users table provider
//!
//! This module provides a DataFusion TableProvider implementation for the system.users table.
//! Uses `IndexedEntityStore` for automatic secondary index management.
//!
//! ## Indexes
//!
//! The users table has two secondary indexes (managed automatically):
//!
//! 1. **UserUsernameIndex** - Unique username lookup (case-insensitive)
//!    - Key: `{username_lowercase}`
//!    - Enables: "Get user by username"
//!
//! 2. **UserRoleIndex** - Query users by role
//!    - Key: `{role}:{user_id}`
//!    - Enables: "All users with role 'admin'"

use super::users_indexes::create_users_indexes;
use super::UsersTableSchema;
use crate::error::{SystemError, SystemResultExt};
use crate::system_table_trait::SystemTableProviderExt;
use async_trait::async_trait;
use datafusion::arrow::array::RecordBatch;
use datafusion::arrow::datatypes::SchemaRef;
use kalamdb_commons::RecordBatchBuilder;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_commons::system::User;
use kalamdb_commons::{StorageKey, UserId};
use kalamdb_store::entity_store::EntityStore;
use kalamdb_store::{IndexedEntityStore, StorageBackend};
use std::any::Any;
use std::sync::Arc;

/// Type alias for the indexed users store
pub type UsersStore = IndexedEntityStore<UserId, User>;

/// System.users table provider using IndexedEntityStore for automatic index management.
///
/// All insert/update/delete operations automatically maintain secondary indexes
/// using RocksDB's atomic WriteBatch - no manual index management needed.
pub struct UsersTableProvider {
    store: UsersStore,
    schema: SchemaRef,
}

impl std::fmt::Debug for UsersTableProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UsersTableProvider").finish()
    }
}

impl UsersTableProvider {
    /// Create a new users table provider with automatic index management.
    ///
    /// # Arguments
    /// * `backend` - Storage backend (RocksDB or mock)
    ///
    /// # Returns
    /// A new UsersTableProvider instance with indexes configured
    pub fn new(backend: Arc<dyn StorageBackend>) -> Self {
        let store = IndexedEntityStore::new(backend, "system_users", create_users_indexes());
        Self {
            store,
            schema: UsersTableSchema::schema(),
        }
    }

    /// Create a new user.
    ///
    /// Indexes are automatically maintained via `IndexedEntityStore`.
    ///
    /// # Arguments
    /// * `user` - The user to create
    ///
    /// # Returns
    /// Result indicating success or failure
    pub fn create_user(&self, user: User) -> Result<(), SystemError> {
        // Check if username already exists (lookup by index)
        // Username index is index 0, key is lowercase username
        let username_key = user.username.as_str().to_lowercase();
        let existing = self
            .store
            .scan_by_index(0, Some(username_key.as_bytes()), Some(1))
            .into_system_error("scan_by_index error")?;

        if !existing.is_empty() {
            return Err(SystemError::AlreadyExists(format!(
                "User with username '{}' already exists",
                user.username.as_str()
            )));
        }

        // Insert user - indexes are managed automatically
        self.store
            .insert(&user.id, &user)
            .into_system_error("insert user error")
    }

    /// Update an existing user.
    ///
    /// Indexes are automatically maintained via `IndexedEntityStore`.
    /// Stale index entries are removed and new ones added atomically.
    ///
    /// # Arguments
    /// * `user` - The updated user data
    ///
    /// # Returns
    /// Result indicating success or failure
    pub fn update_user(&self, user: User) -> Result<(), SystemError> {
        // Check if user exists
        let existing = self.store.get(&user.id)?;
        if existing.is_none() {
            return Err(SystemError::NotFound(format!(
                "User not found: {}",
                user.id
            )));
        }

        let existing_user = existing.unwrap();

        // If username changed, check for conflicts
        if existing_user.username != user.username {
            let username_key = user.username.as_str().to_lowercase();
            let conflicts = self
                .store
                .scan_by_index(0, Some(username_key.as_bytes()), Some(1))
                .into_system_error("scan_by_index error")?;

            if !conflicts.is_empty() && conflicts[0].0 != user.id {
                return Err(SystemError::AlreadyExists(format!(
                    "User with username '{}' already exists",
                    user.username.as_str()
                )));
            }
        }

        // Use update_with_old for efficiency (we already have old entity)
        self.store
            .update_with_old(&user.id, Some(&existing_user), &user)
            .into_system_error("update user error")
    }

    /// Soft delete a user (sets deleted_at timestamp).
    ///
    /// # Arguments
    /// * `user_id` - The ID of the user to delete
    ///
    /// # Returns
    /// Result indicating success or failure
    pub fn delete_user(&self, user_id: &UserId) -> Result<(), SystemError> {
        // Get existing user
        let mut user = self
            .store
            .get(user_id)?
            .ok_or_else(|| SystemError::NotFound(format!("User not found: {}", user_id)))?;

        // Set deleted_at timestamp (soft delete)
        user.deleted_at = Some(chrono::Utc::now().timestamp_millis());

        // Update user with deleted_at
        self.store
            .update(user_id, &user)
            .into_system_error("update user error")
    }

    /// Get a user by ID.
    ///
    /// # Arguments
    /// * `user_id` - The user ID to lookup
    ///
    /// # Returns
    /// Option<User> if found, None otherwise
    pub fn get_user_by_id(&self, user_id: &UserId) -> Result<Option<User>, SystemError> {
        Ok(self.store.get(user_id)?)
    }

    /// Get a user by username.
    ///
    /// Uses the username index for efficient lookup.
    ///
    /// # Arguments
    /// * `username` - The username to lookup
    ///
    /// # Returns
    /// Option<User> if found, None otherwise
    pub fn get_user_by_username(&self, username: &str) -> Result<Option<User>, SystemError> {
        // Username index is index 0, key is lowercase username
        let username_key = username.to_lowercase();
        let results = self
            .store
            .scan_by_index(0, Some(username_key.as_bytes()), Some(1))
            .into_system_error("scan_by_index error")?;

        Ok(results.into_iter().next().map(|(_, user)| user))
    }

    /// Helper to create RecordBatch from users
    fn create_batch(&self, users: Vec<(Vec<u8>, User)>) -> Result<RecordBatch, SystemError> {
        // Extract data into vectors
        let mut user_ids = Vec::with_capacity(users.len());
        let mut usernames = Vec::with_capacity(users.len());
        let mut password_hashes = Vec::with_capacity(users.len());
        let mut roles = Vec::with_capacity(users.len());
        let mut emails = Vec::with_capacity(users.len());
        let mut auth_types = Vec::with_capacity(users.len());
        let mut auth_datas = Vec::with_capacity(users.len());
        let mut storage_modes = Vec::with_capacity(users.len());
        let mut storage_ids = Vec::with_capacity(users.len());
        let mut created_ats = Vec::with_capacity(users.len());
        let mut updated_ats = Vec::with_capacity(users.len());
        let mut last_seens = Vec::with_capacity(users.len());
        let mut deleted_ats = Vec::with_capacity(users.len());

        for (_key, user) in users {
            user_ids.push(Some(user.id.as_str().to_string()));
            usernames.push(Some(user.username.as_str().to_string()));
            password_hashes.push(Some(user.password_hash));
            roles.push(Some(user.role.as_str().to_string()));
            emails.push(user.email);
            auth_types.push(Some(user.auth_type.as_str().to_string()));
            auth_datas.push(user.auth_data);
            storage_modes.push(Some(user.storage_mode.as_str().to_string()));
            storage_ids.push(user.storage_id.map(|s| s.as_str().to_string()));
            created_ats.push(Some(user.created_at));
            updated_ats.push(Some(user.updated_at));
            last_seens.push(user.last_seen);
            deleted_ats.push(user.deleted_at);
        }

        // Build batch using RecordBatchBuilder
        let mut builder = RecordBatchBuilder::new(self.schema.clone());
        builder
            .add_string_column_owned(user_ids)
            .add_string_column_owned(usernames)
            .add_string_column_owned(password_hashes)
            .add_string_column_owned(roles)
            .add_string_column_owned(emails)
            .add_string_column_owned(auth_types)
            .add_string_column_owned(auth_datas)
            .add_string_column_owned(storage_modes)
            .add_string_column_owned(storage_ids)
            .add_timestamp_micros_column(created_ats)
            .add_timestamp_micros_column(updated_ats)
            .add_timestamp_micros_column(last_seens)
            .add_timestamp_micros_column(deleted_ats);

        let batch = builder.build().into_arrow_error("Failed to create RecordBatch")?;

        Ok(batch)
    }

    /// Scan all users and return as RecordBatch
    pub fn scan_all_users(&self) -> Result<RecordBatch, SystemError> {
        let iter = self.store.scan_iterator(None, None)?;
        let mut users: Vec<(Vec<u8>, User)> = Vec::new();
        for item in iter {
            let (id, user) = item?;
            users.push((id.storage_key(), user));
        }
        self.create_batch(users)
    }
}

impl SystemTableProviderExt for UsersTableProvider {
    fn table_name(&self) -> &str {
        UsersTableSchema::table_name()
    }

    fn schema_ref(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn load_batch(&self) -> Result<RecordBatch, SystemError> {
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
        filters: &[Expr],
        limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        use datafusion::datasource::MemTable;
        use datafusion::logical_expr::Operator;
        use datafusion::scalar::ScalarValue;

        let mut start_key = None;
        let mut prefix = None;

        // Extract start_key/prefix from filters
        for expr in filters {
            if let Expr::BinaryExpr(binary) = expr {
                if let Expr::Column(col) = binary.left.as_ref() {
                    if let Expr::Literal(val, _) = binary.right.as_ref() {
                        if col.name == "user_id" {
                            if let ScalarValue::Utf8(Some(s)) = val {
                                match binary.op {
                                    Operator::Eq => {
                                        prefix = Some(UserId::new(s));
                                    }
                                    Operator::Gt | Operator::GtEq => {
                                        start_key = Some(UserId::new(s));
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
        }

        let schema = self.schema.clone();
        let users = self
            .store
            .scan_all(limit, prefix.as_ref(), start_key.as_ref())
            .map_err(|e| DataFusionError::Execution(format!("Failed to scan users: {}", e)))?;

        let batch = self.create_batch(users).map_err(|e| {
            DataFusionError::Execution(format!("Failed to build users batch: {}", e))
        })?;

        let partitions = vec![vec![batch]];
        let table = MemTable::try_new(schema, partitions)
            .map_err(|e| DataFusionError::Execution(format!("Failed to create MemTable: {}", e)))?;
        table.scan(_state, projection, &[], limit).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::{AuthType, Role, StorageId, StorageMode, UserName};
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
            storage_id: Some(StorageId::local()),
            failed_login_attempts: 0,
            locked_until: None,
            last_login_at: None,
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
        assert_eq!(batch.num_columns(), 13);
    }

    #[test]
    fn test_table_provider_schema() {
        let provider = create_test_provider();
        let schema = provider.schema();
        assert_eq!(schema.fields().len(), 13);
        assert_eq!(schema.field(0).name(), "user_id");
        assert_eq!(schema.field(1).name(), "username");
    }
}
