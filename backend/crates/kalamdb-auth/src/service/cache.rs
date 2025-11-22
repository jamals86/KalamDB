use super::types::{AuthService, JwtCacheStats, UserCacheStats};

impl AuthService {
    /// Invalidate user cache entry for a specific username.
    ///
    /// This should be called whenever a user record is updated
    /// (password change, role change, metadata change, etc.)
    ///
    /// # Arguments
    /// * `username` - Username to invalidate from cache
    pub async fn invalidate_user_cache(&self, username: &str) {
        self.user_cache.invalidate(username).await;
    }

    /// Clear all user cache entries.
    ///
    /// This should be called during bulk operations or when
    /// cache consistency needs to be guaranteed.
    pub async fn clear_user_cache(&self) {
        self.user_cache.invalidate_all();
        self.user_cache.run_pending_tasks().await;
    }

    /// Get user cache statistics for monitoring.
    ///
    /// # Returns
    /// Cache stats including entry count
    pub fn get_user_cache_stats(&self) -> UserCacheStats {
        UserCacheStats {
            hits: 0,       // TODO: Implement proper stats tracking
            misses: 0,     // TODO: Implement proper stats tracking
            hit_rate: 0.0, // TODO: Implement proper stats tracking
            entry_count: self.user_cache.entry_count(),
        }
    }

    /// Invalidate JWT cache entry for a specific token.
    ///
    /// This should be called when a JWT token needs to be invalidated
    /// (e.g., user logout, token revocation).
    ///
    /// # Arguments
    /// * `token` - JWT token string to invalidate from cache
    pub async fn invalidate_jwt_cache(&self, token: &str) {
        self.jwt_cache.invalidate(token).await;
    }

    /// Clear all JWT cache entries.
    ///
    /// This should be called during bulk operations or when
    /// JWT cache consistency needs to be guaranteed.
    pub async fn clear_jwt_cache(&self) {
        self.jwt_cache.invalidate_all();
        self.jwt_cache.run_pending_tasks().await;
    }

    /// Get JWT cache statistics for monitoring.
    ///
    /// # Returns
    /// Cache stats including entry count
    pub fn get_jwt_cache_stats(&self) -> JwtCacheStats {
        JwtCacheStats {
            hits: 0,       // TODO: Implement proper stats tracking
            misses: 0,     // TODO: Implement proper stats tracking
            hit_rate: 0.0, // TODO: Implement proper stats tracking
            entry_count: self.jwt_cache.entry_count(),
        }
    }
}
