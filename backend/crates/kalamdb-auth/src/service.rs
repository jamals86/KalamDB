// Authentication service orchestrator

use crate::basic_auth;
use crate::connection::ConnectionInfo;
use crate::context::AuthenticatedUser;
use crate::error::{AuthError, AuthResult};
use crate::jwt_auth;
use crate::password;
// use kalamdb_commons::{Role, UserId}; // Unused imports removed
use kalamdb_sql::RocksDbAdapter;
use log::{info, warn};
use std::sync::Arc;

/// Authentication service that orchestrates all auth methods.
///
/// Supports:
/// - HTTP Basic Authentication (username/password)
/// - JWT Bearer Token Authentication
/// - API Key Authentication
pub struct AuthService {
    /// JWT secret for token validation
    jwt_secret: String,
    /// List of trusted JWT issuers
    trusted_jwt_issuers: Vec<String>,
    /// Whether to allow remote (non-localhost) access
    allow_remote_access: bool,
}

impl AuthService {
    /// Create a new authentication service.
    ///
    /// # Arguments
    /// * `jwt_secret` - Secret key for JWT validation
    /// * `trusted_jwt_issuers` - List of trusted JWT issuer domains
    /// * `allow_remote_access` - Whether to allow non-localhost connections
    pub fn new(
        jwt_secret: String,
        trusted_jwt_issuers: Vec<String>,
        allow_remote_access: bool,
    ) -> Self {
        Self {
            jwt_secret,
            trusted_jwt_issuers,
            allow_remote_access,
        }
    }

    /// Authenticate a request using Authorization header.
    ///
    /// Supports:
    /// - `Basic <base64-credentials>` - HTTP Basic Auth
    /// - `Bearer <jwt-token>` - JWT Authentication
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
            self.authenticate_jwt(auth_header, connection_info, adapter)
                .await
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
            if remote_allowed && !connection_info.is_localhost() {
                if user.password_hash.is_empty() {
                    warn!(
                        "System user '{}' attempted remote authentication without password",
                        username
                    );
                    return Err(AuthError::AuthenticationFailed(
                        "Remote-enabled system users must have a password set".to_string(),
                    ));
                }
            }

            // For localhost connections with internal auth_type, password can be empty
            // For remote connections, password is required (enforced above)
            if !user.password_hash.is_empty() {
                // Verify password if one is set
                let password_match =
                    password::verify_password(&password, &user.password_hash).await?;
                if !password_match {
                    warn!("Invalid password for system user: {}", username);
                    return Err(AuthError::InvalidCredentials);
                }
            }
        } else {
            // For non-internal users (password, oauth), always verify password
            let password_match = password::verify_password(&password, &user.password_hash).await?;
            if !password_match {
                warn!("Invalid password for user: {}", username);
                return Err(AuthError::InvalidCredentials);
            }

            // Check global remote access permission for regular users
            if !connection_info.is_access_allowed(self.allow_remote_access) {
                return Err(AuthError::AuthenticationFailed(
                    "Remote access is disabled".to_string(),
                ));
            }
        }

        info!("User authenticated via Basic Auth: {}", username);

        Ok(AuthenticatedUser::new(
            user.id,
            user.username,
            user.role,
            user.email,
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

        // Validate token
        let claims =
            jwt_auth::validate_jwt_token(token, &self.jwt_secret, &self.trusted_jwt_issuers)?;

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

        Ok(AuthenticatedUser::new(
            user.id,
            user.username,
            user.role,
            user.email,
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
    async fn get_user_by_username(
        &self,
        username: &str,
        adapter: &Arc<RocksDbAdapter>,
    ) -> AuthResult<kalamdb_commons::system::User> {
        adapter
            .get_user(username)
            .map_err(AuthError::DatabaseError)?
            .ok_or(AuthError::UserNotFound)
    }
}
