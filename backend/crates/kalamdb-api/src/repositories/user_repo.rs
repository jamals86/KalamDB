use std::sync::Arc;

use kalamdb_auth::{error::AuthResult, AuthError, UserRepository};
use kalamdb_commons::system::User;
use kalamdb_system::UsersTableProvider;

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
}
