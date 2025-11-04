use crate::error::{AuthError, AuthResult};
use kalamdb_commons::{
    system::User,
    AuthType, Role, StorageId, StorageMode, UserId, UserName,
};
use kalamdb_sql::RocksDbAdapter;
use std::sync::Arc;

/// Abstraction over user persistence for authentication flows.
///
/// This allows kalamdb-auth to work with either:
/// - Legacy RocksDbAdapter (kalamdb-sql)
/// - New provider-based implementations (kalamdb-core SystemTablesRegistry)
#[async_trait::async_trait]
pub trait UserRepository: Send + Sync {
    async fn get_user_by_username(&self, username: &str) -> AuthResult<User>;

    /// Update a full user record. Implementations may persist only changed fields.
    async fn update_user(&self, user: &User) -> AuthResult<()>;

    /// Return all users. Used for OAuth identity matching.
    async fn scan_all_users(&self) -> AuthResult<Vec<User>>;
}

/// Adapter-backed repository (legacy path)
pub struct RocksAdapterUserRepo {
    adapter: Arc<RocksDbAdapter>,
}

impl RocksAdapterUserRepo {
    pub fn new(adapter: Arc<RocksDbAdapter>) -> Self {
        Self { adapter }
    }

    pub fn adapter(&self) -> &Arc<RocksDbAdapter> {
        &self.adapter
    }
}

#[async_trait::async_trait]
impl UserRepository for RocksAdapterUserRepo {
    async fn get_user_by_username(&self, username: &str) -> AuthResult<User> {
        // Use blocking call on a separate thread since adapter is sync
        let adapter = self.adapter.clone();
        let username = username.to_string();
    tokio::task::spawn_blocking(move || {
            adapter
                .get_user(&username)
                .map_err(|e| AuthError::DatabaseError(e.to_string()))?
                .ok_or_else(|| AuthError::UserNotFound(format!("User '{}' not found", username)))
        })
        .await
        .map_err(|e| AuthError::DatabaseError(e.to_string()))?
    }

    async fn update_user(&self, user: &User) -> AuthResult<()> {
        let adapter = self.adapter.clone();
        let user = user.clone();
        tokio::task::spawn_blocking(move || adapter.update_user(&user))
            .await
            .map_err(|e| AuthError::DatabaseError(e.to_string()))?
            .map_err(|e| AuthError::DatabaseError(e.to_string()))
    }

    async fn scan_all_users(&self) -> AuthResult<Vec<User>> {
        let adapter = self.adapter.clone();
        tokio::task::spawn_blocking(move || adapter.scan_all_users())
            .await
            .map_err(|e| AuthError::DatabaseError(e.to_string()))?
            .map_err(|e| AuthError::DatabaseError(e.to_string()))
    }
}

// Provider-backed repository is implemented in kalamdb-api to avoid crate cycles.
