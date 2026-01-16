use std::sync::Arc;
use std::time::Duration;

use kalamdb_auth::{error::AuthResult, AuthError, UserRepository};
use kalamdb_commons::models::UserName;
use kalamdb_commons::system::User;
use kalamdb_system::UsersTableProvider;
use moka::sync::Cache;

/// Time-to-live for cached user records (5 seconds)
/// Short TTL ensures security updates (password changes, role changes) are reflected quickly
const USER_CACHE_TTL_SECS: u64 = 5;

/// Maximum number of cached users
const USER_CACHE_MAX_CAPACITY: u64 = 1000;

/// Cached user repository that wraps CoreUsersRepo with moka caching
///
/// **Performance**: Saves ~1-3ms per request by avoiding RocksDB lookup on cache hit.
/// The cache has a short TTL (5 seconds) to ensure security updates are reflected quickly.
///
/// **Cache Invalidation**:
/// - TTL-based: Entries expire after 5 seconds
/// - Manual: Call `invalidate_user()` after password/role changes
pub struct CachedUsersRepo {
    inner: CoreUsersRepo,
    /// Cache: UserName -> User
    cache: Cache<UserName, User>,
}

impl CachedUsersRepo {
    pub fn new(provider: Arc<UsersTableProvider>) -> Self {
        let cache = Cache::builder()
            .max_capacity(USER_CACHE_MAX_CAPACITY)
            .time_to_live(Duration::from_secs(USER_CACHE_TTL_SECS))
            .build();
        
        Self {
            inner: CoreUsersRepo::new(provider),
            cache,
        }
    }
    
    /// Invalidate a user's cache entry (call after password/role changes)
    pub fn invalidate_user(&self, username: &UserName) {
        self.cache.invalidate(username);
    }
    
    /// Clear entire cache (call on system-wide user changes)
    pub fn clear_cache(&self) {
        self.cache.invalidate_all();
    }
}

#[async_trait::async_trait]
impl UserRepository for CachedUsersRepo {
    async fn get_user_by_username(&self, username: &UserName) -> AuthResult<User> {
        // Fast path: check cache
        if let Some(user) = self.cache.get(username) {
            return Ok(user);
        }
        
        // Cache miss: fetch from database
        let user = self.inner.get_user_by_username(username).await?;
        
        // Cache the result (insert takes ownership, no need to clone user)
        self.cache.insert(username.clone(), user.clone());
        
        Ok(user)
    }

    async fn update_user(&self, user: &User) -> AuthResult<()> {
        // Invalidate cache before update to ensure consistency
        self.invalidate_user(&user.username);
        
        // Perform the update
        self.inner.update_user(user).await
    }
}

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
    async fn get_user_by_username(&self, username: &UserName) -> AuthResult<User> {
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

