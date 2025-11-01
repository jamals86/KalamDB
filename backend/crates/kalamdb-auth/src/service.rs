// Authentication service orchestrator

use crate::basic_auth;
use crate::connection::ConnectionInfo;
use crate::context::AuthenticatedUser;
use crate::error::{AuthError, AuthResult};
use crate::jwt_auth;
use crate::password;
// use kalamdb_commons::{Role, UserId}; // Unused imports removed
use chrono::{DateTime, Utc};
use kalamdb_sql::RocksDbAdapter;
use log::{info, warn};
use moka::future::Cache;
use std::sync::Arc;
use tokio::task;

/// User cache statistics for monitoring and performance tracking
#[derive(Debug, Clone)]
pub struct UserCacheStats {
    /// Number of cache hits
    pub hits: u64,
    /// Number of cache misses
    pub misses: u64,
    /// Cache hit rate (0.0 to 1.0)
    pub hit_rate: f64,
    /// Current number of entries in cache
    pub entry_count: u64,
}

/// JWT cache statistics for monitoring and performance tracking
#[derive(Debug, Clone)]
pub struct JwtCacheStats {
    /// Number of cache hits
    pub hits: u64,
    /// Number of cache misses
    pub misses: u64,
    /// Cache hit rate (0.0 to 1.0)
    pub hit_rate: f64,
    /// Current number of entries in cache
    pub entry_count: u64,
}

/// Authentication service that orchestrates all auth methods.
///
/// Supports:
/// - HTTP Basic Authentication (username/password)
/// - JWT Bearer Token Authentication
/// - OAuth Token Authentication (Phase 10, User Story 8)
pub struct AuthService {
    /// JWT secret for token validation
    jwt_secret: String,
    /// List of trusted JWT issuers
    trusted_jwt_issuers: Vec<String>,
    /// Whether to allow remote (non-localhost) access
    allow_remote_access: bool,
    /// OAuth auto-provisioning enabled (Phase 10, User Story 8)
    #[allow(dead_code)]
    oauth_auto_provision: bool,
    /// Default role for auto-provisioned OAuth users
    #[allow(dead_code)]
    oauth_default_role: kalamdb_commons::Role,
    /// User record cache for performance optimization
    /// Key: username, Value: User record
    /// TTL: 5 minutes, Max capacity: 1000 users
    user_cache: Cache<String, kalamdb_commons::system::User>,
    /// JWT claims cache for performance optimization
    /// Key: JWT token string, Value: Validated JWT claims
    /// TTL: 10 minutes, Max capacity: 500 tokens
    jwt_cache: Cache<String, crate::jwt_auth::JwtClaims>,
}

impl AuthService {
    /// Create a new authentication service.
    ///
    /// # Arguments
    /// * `jwt_secret` - Secret key for JWT validation
    /// * `trusted_jwt_issuers` - List of trusted JWT issuer domains
    /// * `allow_remote_access` - Whether to allow non-localhost connections
    /// * `oauth_auto_provision` - Enable OAuth auto-provisioning (default: false)
    /// * `oauth_default_role` - Default role for auto-provisioned OAuth users
    pub fn new(
        jwt_secret: String,
        trusted_jwt_issuers: Vec<String>,
        allow_remote_access: bool,
        oauth_auto_provision: bool,
        oauth_default_role: kalamdb_commons::Role,
    ) -> Self {
        // Initialize user cache with 5-minute TTL and 1000 max entries
        let user_cache = Cache::builder()
            .max_capacity(1000)
            .time_to_live(std::time::Duration::from_secs(300)) // 5 minutes
            .build();

        // Initialize JWT cache with 10-minute TTL and 500 max entries
        let jwt_cache = Cache::builder()
            .max_capacity(500)
            .time_to_live(std::time::Duration::from_secs(600)) // 10 minutes
            .build();

        Self {
            jwt_secret,
            trusted_jwt_issuers,
            allow_remote_access,
            oauth_auto_provision,
            oauth_default_role,
            user_cache,
            jwt_cache,
        }
    }

    /// Authenticate a request using Authorization header.
    ///
    /// Supports:
    /// - `Basic <base64-credentials>` - HTTP Basic Auth
    /// - `Bearer <jwt-token>` - JWT Authentication
    /// - `Bearer <oauth-token>` - OAuth Token Authentication (Phase 10, User Story 8)
    ///
    /// # Arguments
    /// * `auth_header` - Value of Authorization header
    /// * `connection_info` - Connection information (IP address)
    /// * `adapter` - RocksDB adapter for user database access
    ///
    /// # Returns
    /// Authenticated user context
    ///
    /// # Errors
    /// - `AuthError::MissingAuthorization` if header is missing
    /// - `AuthError::InvalidCredentials` if credentials are wrong
    /// - Other errors from specific auth methods
    pub async fn authenticate(
        &self,
        auth_header: &str,
        connection_info: &ConnectionInfo,
        adapter: &Arc<RocksDbAdapter>,
    ) -> AuthResult<AuthenticatedUser> {
        // Determine auth method based on prefix
        if auth_header.starts_with("Basic ") {
            self.authenticate_basic(auth_header, connection_info, adapter)
                .await
        } else if auth_header.starts_with("Bearer ") {
            // Try JWT first, then OAuth if JWT fails
            // This allows for fallback when JWT validation fails
            match self
                .authenticate_jwt(auth_header, connection_info, adapter)
                .await
            {
                Ok(user) => Ok(user),
                Err(_) => {
                    // JWT failed, try OAuth
                    self.authenticate_oauth(auth_header, connection_info, adapter)
                        .await
                }
            }
        } else {
            Err(AuthError::MalformedAuthorization(
                "Authorization header must start with 'Basic ' or 'Bearer '".to_string(),
            ))
        }
    }

    /// Authenticate using HTTP Basic Auth.
    ///
    /// # Arguments
    /// * `auth_header` - Authorization header value
    /// * `connection_info` - Connection information
    /// * `adapter` - RocksDB adapter for user database access
    ///
    /// # Returns
    /// Authenticated user context
    ///
    /// # System User Authentication (T103, T104, T106 - Phase 7, User Story 5)
    /// - Internal auth_type users (system users) can only authenticate from localhost by default
    /// - Global allow_remote_access flag allows remote connections for all internal users
    /// - Per-user metadata {"allow_remote": true} overrides localhost restriction
    /// - Remote-enabled internal users MUST have a password set (enforced during user creation)
    async fn authenticate_basic(
        &self,
        auth_header: &str,
        connection_info: &ConnectionInfo,
        adapter: &Arc<RocksDbAdapter>,
    ) -> AuthResult<AuthenticatedUser> {
        // Parse credentials
        let (username, password) = basic_auth::parse_basic_auth_header(auth_header)?;

        // Look up user in database
        let user = self.get_user_by_username(&username, adapter).await?;

        // Check if user is deleted
        if user.deleted_at.is_some() {
            warn!("Attempt to authenticate deleted user: {}", username);
            return Err(AuthError::UserDeleted);
        }

        // T140 (Phase 10, User Story 8): Prevent OAuth users from password authentication
        if user.auth_type == kalamdb_commons::AuthType::OAuth {
            warn!(
                "OAuth user '{}' attempted password authentication",
                username
            );
            return Err(AuthError::AuthenticationFailed(
                "OAuth users cannot authenticate with password. Use OAuth token instead."
                    .to_string(),
            ));
        }

        // T103: Check if user has internal auth_type (system users)
        // T104: Check per-user allow_remote metadata
        // T106: Validate password requirement for remote system users
        if user.auth_type == kalamdb_commons::AuthType::Internal {
            // Parse user metadata to check for per-user allow_remote flag
            let per_user_allow_remote = if let Some(ref metadata) = user.auth_data {
                // Try to parse metadata as JSON to check for allow_remote flag
                match serde_json::from_str::<serde_json::Value>(metadata) {
                    Ok(json) => json
                        .get("allow_remote")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                    Err(_) => false,
                }
            } else {
                false
            };

            // Determine if remote access is allowed for this user
            // Priority: per-user metadata > global config > default (localhost-only)
            let remote_allowed = per_user_allow_remote || self.allow_remote_access;

            // T103: Internal users must connect from localhost unless remote access is explicitly enabled
            if !connection_info.is_localhost() && !remote_allowed {
                warn!(
                    "System user '{}' attempted remote authentication (not allowed)",
                    username
                );
                return Err(AuthError::AuthenticationFailed(
                    "System users can only authenticate from localhost unless remote access is enabled".to_string(),
                ));
            }

            // T106: If remote access is enabled for internal user, password MUST be set
            if remote_allowed && !connection_info.is_localhost() && user.password_hash.is_empty() {
                warn!(
                    "System user '{}' attempted remote authentication without password",
                    username
                );
                return Err(AuthError::AuthenticationFailed(
                    "Remote-enabled system users must have a password set".to_string(),
                ));
            }

            // For localhost connections with internal auth_type, password can be empty
            // For remote connections, password is required (enforced above)
            if !user.password_hash.is_empty() {
                // Verify password if one is set
                let password_match =
                    password::verify_password(&password, &user.password_hash).await?;
                if !password_match {
                    warn!("Invalid password for system user: {}", username);
                    return Err(AuthError::InvalidCredentials(
                        "Invalid username or password".to_string(),
                    ));
                }
            }
        } else {
            // For non-internal users (password, oauth), always verify password
            let password_match = password::verify_password(&password, &user.password_hash).await?;
            if !password_match {
                warn!("Invalid password for user: {}", username);
                return Err(AuthError::InvalidCredentials(
                    "Invalid username or password".to_string(),
                ));
            }

            // Check global remote access permission for regular users
            if !connection_info.is_access_allowed(self.allow_remote_access) {
                return Err(AuthError::AuthenticationFailed(
                    "Remote access is disabled".to_string(),
                ));
            }
        }

        info!("User authenticated via Basic Auth: {}", username);

        Self::spawn_last_seen_update(adapter.clone(), user.clone());

        Ok(AuthenticatedUser::new(
            user.id.clone(),
            user.username.as_str().to_string(),
            user.role,
            user.email.clone(),
            connection_info.clone(),
        ))
    }

    /// Authenticate using JWT Bearer token.
    ///
    /// # Arguments
    /// * `auth_header` - Authorization header value
    /// * `connection_info` - Connection information
    /// * `adapter` - RocksDB adapter for user database access
    ///
    /// # Returns
    /// Authenticated user context
    async fn authenticate_jwt(
        &self,
        auth_header: &str,
        connection_info: &ConnectionInfo,
        adapter: &Arc<RocksDbAdapter>,
    ) -> AuthResult<AuthenticatedUser> {
        // Extract token (remove "Bearer " prefix)
        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or_else(|| AuthError::MalformedAuthorization("Bearer token missing".to_string()))?;

        // Check JWT cache first
        let claims = if let Some(cached_claims) = self.jwt_cache.get(token).await {
            cached_claims
        } else {
            // Cache miss - validate JWT
            let validated_claims =
                jwt_auth::validate_jwt_token(token, &self.jwt_secret, &self.trusted_jwt_issuers)?;

            // Cache the validated claims
            self.jwt_cache
                .insert(token.to_string(), validated_claims.clone())
                .await;
            validated_claims
        };

        // Look up user in database using username from JWT claims
        // The JWT sub field contains the user_id, but we'll use username for lookup
        let username = claims.username.as_ref().ok_or_else(|| {
            AuthError::MissingClaim("username claim missing from JWT".to_string())
        })?;

        let user = self.get_user_by_username(username, adapter).await?;

        // Check if user is deleted
        if user.deleted_at.is_some() {
            warn!("Attempt to authenticate deleted user: {}", user.username);
            return Err(AuthError::UserDeleted);
        }

        // Check remote access permission
        if !connection_info.is_access_allowed(self.allow_remote_access) {
            return Err(AuthError::AuthenticationFailed(
                "Remote access is disabled".to_string(),
            ));
        }

        info!("User authenticated via JWT: {}", user.username);

        Self::spawn_last_seen_update(adapter.clone(), user.clone());

        Ok(AuthenticatedUser::new(
            user.id.clone(),
            user.username.as_str().to_string(),
            user.role,
            user.email.clone(),
            connection_info.clone(),
        ))
    }

    /// Authenticate using OAuth Bearer token.
    ///
    /// Phase 10, User Story 8: OAuth Integration
    ///
    /// # Arguments
    /// * `auth_header` - Authorization header value (Bearer <oauth-token>)
    /// * `connection_info` - Connection information
    /// * `adapter` - RocksDB adapter for user database access
    ///
    /// # Returns
    /// Authenticated user context
    ///
    /// # Note
    /// This method requires OAuth provider configuration to be enabled.
    /// It validates the OAuth token against configured provider issuers,
    /// extracts the provider and subject, and looks up the user by matching
    /// the auth_data JSON {"provider": "...", "subject": "..."}.
    async fn authenticate_oauth(
        &self,
        auth_header: &str,
        connection_info: &ConnectionInfo,
        adapter: &Arc<RocksDbAdapter>,
    ) -> AuthResult<AuthenticatedUser> {
        use crate::oauth;

        // Extract token (remove "Bearer " prefix)
        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or_else(|| AuthError::MalformedAuthorization("Bearer token missing".to_string()))?;

        // Try to extract provider info without full validation first
        // (we'll validate once we know which provider to check against)
        let _header = jsonwebtoken::decode_header(token).map_err(|e| {
            AuthError::MalformedAuthorization(format!("Invalid token header: {}", e))
        })?;

        // For now, we use a simple approach: validate with a generic secret
        // In production, you'd fetch the JWKS (JSON Web Key Set) from the provider
        // For HS256 (testing), we use the JWT secret
        let claims = oauth::validate_oauth_token(token, &self.jwt_secret, "")
            .map_err(|_| AuthError::InvalidSignature)?;

        // Extract provider and subject from claims
        let identity = oauth::extract_provider_and_subject(&claims);

        // Look up user by matching auth_data JSON
        // auth_data format: {"provider": "google", "subject": "oauth_user_id"}
        let auth_data = serde_json::json!({
            "provider": identity.provider,
            "subject": identity.subject
        });

        let users = adapter
            .scan_all_users()
            .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

        let user = users
            .into_iter()
            .find(|u| {
                u.auth_type == kalamdb_commons::AuthType::OAuth
                    && u.auth_data.as_ref().is_some_and(|data| {
                        // Parse stored auth_data and compare
                        if let Ok(stored_json) = serde_json::from_str::<serde_json::Value>(data) {
                            stored_json.get("provider") == auth_data.get("provider")
                                && stored_json.get("subject") == auth_data.get("subject")
                        } else {
                            false
                        }
                    })
            })
            .ok_or(AuthError::UserNotFound("OAuth user not found".to_string()))?;

        // Check if user is deleted
        if user.deleted_at.is_some() {
            warn!("Attempt to authenticate deleted user: {}", user.username);
            return Err(AuthError::UserDeleted);
        }

        // Check remote access permission
        if !connection_info.is_access_allowed(self.allow_remote_access) {
            return Err(AuthError::AuthenticationFailed(
                "Remote access is disabled".to_string(),
            ));
        }

        info!(
            "User authenticated via OAuth ({}): {}",
            identity.provider, user.username
        );

        Self::spawn_last_seen_update(adapter.clone(), user.clone());

        Ok(AuthenticatedUser::new(
            user.id.clone(),
            user.username.as_str().to_string(),
            user.role,
            user.email.clone(),
            connection_info.clone(),
        ))
    }

    /// Get user by username from database.
    ///
    /// # Arguments
    /// * `username` - Username to look up
    /// * `adapter` - RocksDB adapter for database access
    ///
    /// # Returns
    /// User entity
    ///
    /// # Errors
    /// Returns `AuthError::UserNotFound` if user doesn't exist
    fn needs_last_seen_update(last_seen: Option<i64>, now: DateTime<Utc>) -> bool {
        match last_seen.and_then(DateTime::<Utc>::from_timestamp_millis) {
            Some(prev) => prev.date_naive() != now.date_naive(),
            None => true,
        }
    }

    fn spawn_last_seen_update(
        adapter: Arc<RocksDbAdapter>,
        mut user: kalamdb_commons::system::User,
    ) {
        let now = Utc::now();
        if !Self::needs_last_seen_update(user.last_seen, now) {
            return;
        }

        let new_timestamp = now.timestamp_millis();
        user.last_seen = Some(new_timestamp);
        user.updated_at = new_timestamp;
        let user_id = user.id.clone();

        tokio::spawn(async move {
            let uid = user_id.clone();
            let update_result = task::spawn_blocking(move || adapter.update_user(&user)).await;
            match update_result {
                Ok(Ok(())) => {}
                Ok(Err(err)) => warn!(
                    "Failed to persist last_seen for user {}: {}",
                    uid.as_str(),
                    err
                ),
                Err(join_err) => warn!(
                    "Failed to schedule last_seen update for user {}: {}",
                    uid.as_str(),
                    join_err
                ),
            }
        });
    }

    async fn get_user_by_username(
        &self,
        username: &str,
        adapter: &Arc<RocksDbAdapter>,
    ) -> AuthResult<kalamdb_commons::system::User> {
        // Check cache first
        if let Some(user) = self.user_cache.get(username).await {
            return Ok(user);
        }

        // Cache miss - fetch from database
        let user = adapter
            .get_user(username)
            .map_err(|e| AuthError::DatabaseError(e.to_string()))?
            .ok_or_else(|| AuthError::UserNotFound(format!("User '{}' not found", username)))?;

        // Cache the user record for future lookups
        self.user_cache
            .insert(username.to_string(), user.clone())
            .await;

        Ok(user)
    }

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
