use crate::error::{AuthError, AuthResult};
use kalamdb_commons::system::User;

/// Abstraction over user persistence for authentication flows.
///
/// This allows kalamdb-auth to work with provider-based implementations
/// from kalamdb-core SystemTablesRegistry without depending on kalamdb-sql.
///
/// Implementations are provided by kalamdb-api to avoid crate cycles.
#[async_trait::async_trait]
pub trait UserRepository: Send + Sync {
    async fn get_user_by_username(&self, username: &str) -> AuthResult<User>;

    /// Update a full user record. Implementations may persist only changed fields.
    async fn update_user(&self, user: &User) -> AuthResult<()>;

    /// Return all users. Used for OAuth identity matching.
    async fn scan_all_users(&self) -> AuthResult<Vec<User>>;
}
