use std::sync::Arc;

use arrow::array::{StringArray, TimestampMillisecondArray};
use kalamdb_auth::{AuthError, AuthResult, UserRepository};
use kalamdb_commons::{system::User, AuthType, Role, StorageId, StorageMode, UserId, UserName};
use kalamdb_core::tables::system::UsersTableProvider;

/// Repository adapter backed by kalamdb-core's UsersTableProvider
pub struct CoreUsersRepo {
    provider: Arc<UsersTableProvider>,
}

impl CoreUsersRepo {
    pub fn new(provider: Arc<UsersTableProvider>) -> Self {
        Self { provider }
    }
}

#[async_trait::async_trait]
impl UserRepository for CoreUsersRepo {
    async fn get_user_by_username(&self, username: &str) -> AuthResult<User> {
        let username = username.to_string();
        let provider = self.provider.clone();
        tokio::task::spawn_blocking(move || {
            provider
                .get_user_by_username(&username)
                .map_err(|e| AuthError::DatabaseError(e.to_string()))?
                .ok_or_else(|| AuthError::UserNotFound(format!("User '{}' not found", username)))
        })
        .await
        .map_err(|e| AuthError::DatabaseError(e.to_string()))?
    }

    async fn update_user(&self, user: &User) -> AuthResult<()> {
        let provider = self.provider.clone();
        let user = user.clone();
        tokio::task::spawn_blocking(move || provider.update_user(user))
            .await
            .map_err(|e| AuthError::DatabaseError(e.to_string()))?
            .map_err(|e| AuthError::DatabaseError(e.to_string()))
    }

    async fn scan_all_users(&self) -> AuthResult<Vec<User>> {
        let provider = self.provider.clone();
        tokio::task::spawn_blocking(move || {
            let batch = provider
                .scan_all_users()
                .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

            let user_id_col = batch
                .column_by_name("user_id")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                .ok_or_else(|| AuthError::DatabaseError("users.user_id column missing".into()))?;
            let username_col = batch
                .column_by_name("username")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                .ok_or_else(|| AuthError::DatabaseError("users.username column missing".into()))?;
            let password_hash_col = batch
                .column_by_name("password_hash")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                .ok_or_else(|| AuthError::DatabaseError("users.password_hash column missing".into()))?;
            let role_col = batch
                .column_by_name("role")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                .ok_or_else(|| AuthError::DatabaseError("users.role column missing".into()))?;
            let email_col = batch
                .column_by_name("email")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                .ok_or_else(|| AuthError::DatabaseError("users.email column missing".into()))?;
            let auth_type_col = batch
                .column_by_name("auth_type")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                .ok_or_else(|| AuthError::DatabaseError("users.auth_type column missing".into()))?;
            let auth_data_col = batch
                .column_by_name("auth_data")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                .ok_or_else(|| AuthError::DatabaseError("users.auth_data column missing".into()))?;
            let storage_mode_col = batch
                .column_by_name("storage_mode")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                .ok_or_else(|| AuthError::DatabaseError("users.storage_mode column missing".into()))?;
            let storage_id_col = batch
                .column_by_name("storage_id")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                .ok_or_else(|| AuthError::DatabaseError("users.storage_id column missing".into()))?;
            let created_at_col = batch
                .column_by_name("created_at")
                .and_then(|c| c.as_any().downcast_ref::<TimestampMillisecondArray>())
                .ok_or_else(|| AuthError::DatabaseError("users.created_at column missing".into()))?;
            let updated_at_col = batch
                .column_by_name("updated_at")
                .and_then(|c| c.as_any().downcast_ref::<TimestampMillisecondArray>())
                .ok_or_else(|| AuthError::DatabaseError("users.updated_at column missing".into()))?;
            let last_seen_col = batch
                .column_by_name("last_seen")
                .and_then(|c| c.as_any().downcast_ref::<TimestampMillisecondArray>())
                .ok_or_else(|| AuthError::DatabaseError("users.last_seen column missing".into()))?;
            let deleted_at_col = batch
                .column_by_name("deleted_at")
                .and_then(|c| c.as_any().downcast_ref::<TimestampMillisecondArray>())
                .ok_or_else(|| AuthError::DatabaseError("users.deleted_at column missing".into()))?;

            let mut users = Vec::with_capacity(batch.num_rows());
            for i in 0..batch.num_rows() {
                let id = UserId::new(user_id_col.value(i));
                let username = UserName::new(username_col.value(i));
                let password_hash = password_hash_col.value(i).to_string();
                let role = Role::from(role_col.value(i));
                let email = if email_col.is_null(i) {
                    None
                } else {
                    Some(email_col.value(i).to_string())
                };
                let auth_type = AuthType::from(auth_type_col.value(i));
                let auth_data = if auth_data_col.is_null(i) {
                    None
                } else {
                    Some(auth_data_col.value(i).to_string())
                };
                let storage_mode = StorageMode::from(storage_mode_col.value(i));
                let storage_id = if storage_id_col.is_null(i) {
                    None
                } else {
                    Some(StorageId::new(storage_id_col.value(i)))
                };
                let created_at = created_at_col
                    .value_as_datetime(i)
                    .map(|dt| dt.timestamp_millis())
                    .unwrap_or(0);
                let updated_at = updated_at_col
                    .value_as_datetime(i)
                    .map(|dt| dt.timestamp_millis())
                    .unwrap_or(0);
                let last_seen = if last_seen_col.is_null(i) {
                    None
                } else {
                    Some(
                        last_seen_col
                            .value_as_datetime(i)
                            .map(|dt| dt.timestamp_millis())
                            .unwrap_or(0),
                    )
                };
                let deleted_at = if deleted_at_col.is_null(i) {
                    None
                } else {
                    Some(
                        deleted_at_col
                            .value_as_datetime(i)
                            .map(|dt| dt.timestamp_millis())
                            .unwrap_or(0),
                    )
                };

                users.push(User {
                    id,
                    username,
                    password_hash,
                    role,
                    email,
                    auth_type,
                    auth_data,
                    storage_mode,
                    storage_id,
                    created_at,
                    updated_at,
                    last_seen,
                    deleted_at,
                });
            }

            Ok(users)
        })
        .await
        .map_err(|e| AuthError::DatabaseError(e.to_string()))?
    }
}
