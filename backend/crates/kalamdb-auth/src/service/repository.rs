use super::types::AuthService;
use crate::basic_auth;
use crate::connection::ConnectionInfo;
use crate::context::AuthenticatedUser;
use crate::error::{AuthError, AuthResult};
use crate::jwt_auth;
use crate::oauth;
use crate::password;
use crate::user_repo::UserRepository;
use chrono::{DateTime, Utc};
use log::warn;
use std::sync::Arc;

impl AuthService {
    /// Authenticate using a repository abstraction (provider-ready).
    ///
    /// This overload allows callers to pass a UserRepository implementation
    /// so that kalamdb-auth can work without RocksDbAdapter.
    pub async fn authenticate_with_repo(
        &self,
        auth_header: &str,
        connection_info: &ConnectionInfo,
        repo: &Arc<dyn UserRepository>,
    ) -> AuthResult<AuthenticatedUser> {
        if auth_header.starts_with("Basic ") {
            self.authenticate_basic_with_repo(auth_header, connection_info, repo)
                .await
        } else if auth_header.starts_with("Bearer ") {
            match self
                .authenticate_jwt_with_repo(auth_header, connection_info, repo)
                .await
            {
                Ok(user) => Ok(user),
                Err(_) => {
                    self.authenticate_oauth_with_repo(auth_header, connection_info, repo)
                        .await
                }
            }
        } else {
            Err(AuthError::MalformedAuthorization(
                "Authorization header must start with 'Basic ' or 'Bearer '".to_string(),
            ))
        }
    }

    async fn authenticate_basic_with_repo(
        &self,
        auth_header: &str,
        connection_info: &ConnectionInfo,
        repo: &Arc<dyn UserRepository>,
    ) -> AuthResult<AuthenticatedUser> {
        // Parse credentials
        let (username, password) = basic_auth::parse_basic_auth_header(auth_header)?;

        // Look up user via repository
        let user = repo.get_user_by_username(&username).await?;

        // The remainder mirrors the adapter-based implementation
        if user.deleted_at.is_some() {
            warn!("Attempt to authenticate deleted user: {}", username);
            return Err(AuthError::UserDeleted);
        }

        if user.auth_type == kalamdb_commons::AuthType::OAuth {
            return Err(AuthError::AuthenticationFailed(
                "OAuth users cannot authenticate with password. Use OAuth token instead."
                    .to_string(),
            ));
        }

        if user.auth_type == kalamdb_commons::AuthType::Internal {
            let per_user_allow_remote = if let Some(ref metadata) = user.auth_data {
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
            let remote_allowed = per_user_allow_remote || self.allow_remote_access;
            if !connection_info.is_localhost() && !remote_allowed {
                return Err(AuthError::AuthenticationFailed(
                    "System users can only authenticate from localhost unless remote access is enabled".to_string(),
                ));
            }
            if remote_allowed && !connection_info.is_localhost() && user.password_hash.is_empty() {
                return Err(AuthError::AuthenticationFailed(
                    "Remote-enabled system users must have a password set".to_string(),
                ));
            }
            if !user.password_hash.is_empty() {
                let password_match =
                    password::verify_password(&password, &user.password_hash).await?;
                if !password_match {
                    return Err(AuthError::InvalidCredentials(
                        "Invalid username or password".to_string(),
                    ));
                }
            }
        } else {
            let password_match = password::verify_password(&password, &user.password_hash).await?;
            if !password_match {
                return Err(AuthError::InvalidCredentials(
                    "Invalid username or password".to_string(),
                ));
            }
            if !connection_info.is_access_allowed(self.allow_remote_access) {
                return Err(AuthError::AuthenticationFailed(
                    "Remote access is disabled".to_string(),
                ));
            }
        }

        // Asynchronously update last_seen via repository
        Self::spawn_last_seen_update_repo(repo.clone(), user.clone());

        Ok(AuthenticatedUser::new(
            user.id.clone(),
            user.username.as_str().to_string(),
            user.role,
            user.email.clone(),
            connection_info.clone(),
        ))
    }

    async fn authenticate_jwt_with_repo(
        &self,
        auth_header: &str,
        connection_info: &ConnectionInfo,
        repo: &Arc<dyn UserRepository>,
    ) -> AuthResult<AuthenticatedUser> {
        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or_else(|| AuthError::MalformedAuthorization("Bearer token missing".to_string()))?;

        let claims = if let Some(cached_claims) = self.jwt_cache.get(token).await {
            cached_claims
        } else {
            let validated_claims =
                jwt_auth::validate_jwt_token(token, &self.jwt_secret, &self.trusted_jwt_issuers)?;
            self.jwt_cache
                .insert(token.to_string(), validated_claims.clone())
                .await;
            validated_claims
        };

        let username = claims.username.as_ref().ok_or_else(|| {
            AuthError::MissingClaim("username claim missing from JWT".to_string())
        })?;

        let user = repo.get_user_by_username(username).await?;
        if user.deleted_at.is_some() {
            return Err(AuthError::UserDeleted);
        }
        if !connection_info.is_access_allowed(self.allow_remote_access) {
            return Err(AuthError::AuthenticationFailed(
                "Remote access is disabled".to_string(),
            ));
        }
        Self::spawn_last_seen_update_repo(repo.clone(), user.clone());
        Ok(AuthenticatedUser::new(
            user.id.clone(),
            user.username.as_str().to_string(),
            user.role,
            user.email.clone(),
            connection_info.clone(),
        ))
    }

    async fn authenticate_oauth_with_repo(
        &self,
        auth_header: &str,
        connection_info: &ConnectionInfo,
        repo: &Arc<dyn UserRepository>,
    ) -> AuthResult<AuthenticatedUser> {
        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or_else(|| AuthError::MalformedAuthorization("Bearer token missing".to_string()))?;

        let _header = jsonwebtoken::decode_header(token).map_err(|e| {
            AuthError::MalformedAuthorization(format!("Invalid token header: {}", e))
        })?;

        let claims = oauth::validate_oauth_token(token, &self.jwt_secret, "")
            .map_err(|_| AuthError::InvalidSignature)?;

        let identity = oauth::extract_provider_and_subject(&claims);
        let auth_data = serde_json::json!({
            "provider": identity.provider,
            "subject": identity.subject
        });

        let users = repo.scan_all_users().await?;
        let user = users
            .into_iter()
            .find(|u| {
                u.auth_type == kalamdb_commons::AuthType::OAuth
                    && u.auth_data.as_ref().is_some_and(|data| {
                        if let Ok(stored_json) = serde_json::from_str::<serde_json::Value>(data) {
                            stored_json.get("provider") == auth_data.get("provider")
                                && stored_json.get("subject") == auth_data.get("subject")
                        } else {
                            false
                        }
                    })
            })
            .ok_or(AuthError::UserNotFound("OAuth user not found".to_string()))?;

        if user.deleted_at.is_some() {
            return Err(AuthError::UserDeleted);
        }
        if !connection_info.is_access_allowed(self.allow_remote_access) {
            return Err(AuthError::AuthenticationFailed(
                "Remote access is disabled".to_string(),
            ));
        }
        Self::spawn_last_seen_update_repo(repo.clone(), user.clone());
        Ok(AuthenticatedUser::new(
            user.id.clone(),
            user.username.as_str().to_string(),
            user.role,
            user.email.clone(),
            connection_info.clone(),
        ))
    }

    fn needs_last_seen_update(last_seen: Option<i64>, now: DateTime<Utc>) -> bool {
        match last_seen.and_then(DateTime::<Utc>::from_timestamp_millis) {
            Some(prev) => prev.date_naive() != now.date_naive(),
            None => true,
        }
    }

    fn spawn_last_seen_update_repo(
        repo: Arc<dyn UserRepository>,
        user: kalamdb_commons::system::User,
    ) {
        let username = user.username.as_str().to_string();
        let username_for_log = username.clone();

        tokio::spawn(async move {
            let now = Utc::now();
            // Fetch latest user record via repo to avoid cache staleness
            match repo.get_user_by_username(&username).await {
                Ok(mut db_user) => {
                    if !Self::needs_last_seen_update(db_user.last_seen, now) {
                        return;
                    }
                    let ts = now.timestamp_millis();
                    db_user.last_seen = Some(ts);
                    db_user.updated_at = ts;
                    if let Err(err) = repo.update_user(&db_user).await {
                        warn!(
                            "Failed to persist last_seen for user {}: {}",
                            username_for_log, err
                        );
                    }
                }
                Err(err) => {
                    // If user not found or repo error, just log and return
                    warn!(
                        "Failed to fetch user {} for last_seen update: {}",
                        username_for_log, err
                    );
                }
            }
        });
    }
}
