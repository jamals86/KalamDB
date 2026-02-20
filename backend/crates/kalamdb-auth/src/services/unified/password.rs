use crate::errors::error::{AuthError, AuthResult};
use crate::helpers::basic_auth;
use crate::models::context::AuthenticatedUser;
use crate::repository::user_repo::UserRepository;
use crate::security::password;
use kalamdb_commons::constants::AuthConstants;
use kalamdb_commons::models::{ConnectionInfo, UserName};
use kalamdb_commons::{AuthType, Role};
use log::debug;
use std::sync::Arc;
use tracing::Instrument;

use super::LOGIN_TRACKER;

/// Authenticate using Basic Auth header.
#[allow(dead_code)]
pub(super) async fn authenticate_basic(
    auth_header: &str,
    connection_info: &ConnectionInfo,
    repo: &Arc<dyn UserRepository>,
) -> AuthResult<AuthenticatedUser> {
    let (username, password) = basic_auth::parse_basic_auth_header(auth_header)?;
    authenticate_username_password(&username, &password, connection_info, repo).await
}

/// Core authentication logic for username/password.
pub(super) async fn authenticate_username_password(
    username: &str,
    password: &str,
    connection_info: &ConnectionInfo,
    repo: &Arc<dyn UserRepository>,
) -> AuthResult<AuthenticatedUser> {
    let span = tracing::info_span!(
        "auth.username_password",
        username = username,
        is_localhost = connection_info.is_localhost()
    );
    async move {
        if username.trim().is_empty() {
            return Err(AuthError::InvalidCredentials("Invalid username or password".to_string()));
        }

        let username_typed = UserName::from(username);
        let mut user = repo.get_user_by_username(&username_typed).await?;

        if user.deleted_at.is_some() {
            debug!("Authentication failed for user attempt");
            return Err(AuthError::InvalidCredentials("Invalid username or password".to_string()));
        }

        LOGIN_TRACKER.check_lockout(&user)?;

        if user.auth_type == AuthType::OAuth {
            return Err(AuthError::AuthenticationFailed(
                "OAuth users cannot authenticate with password. Use OAuth token instead."
                    .to_string(),
            ));
        }

        let is_localhost = connection_info.is_localhost();
        let is_system_internal = user.role == Role::System && user.auth_type == AuthType::Internal;

        if username == AuthConstants::DEFAULT_SYSTEM_USERNAME
            && user.password_hash.is_empty()
            && password.is_empty()
        {
            return Err(AuthError::SetupRequired(
                "Server requires initial setup. Root password is not configured.".to_string(),
            ));
        }

        let mut auth_success = false;

        if is_system_internal {
            if is_localhost {
                let password_ok = !password.is_empty()
                    && !user.password_hash.is_empty()
                    && password::verify_password(password, &user.password_hash)
                        .await
                        .unwrap_or(false);

                if password_ok {
                    auth_success = true;
                } else {
                    debug!("Authentication failed for system user attempt");
                }
            } else {
                if user.password_hash.is_empty() {
                    return Err(AuthError::RemoteAccessDenied(
                        "System users with empty passwords cannot authenticate remotely. Set a password with: ALTER USER root SET PASSWORD '...'".to_string(),
                    ));
                }
                if !password.is_empty()
                    && password::verify_password(password, &user.password_hash)
                        .await
                        .unwrap_or(false)
                {
                    auth_success = true;
                } else {
                    debug!("Authentication failed for remote user attempt");
                }
            }
        } else if user.password_hash.is_empty() {
            return Err(AuthError::InvalidCredentials(
                "Invalid username or password".to_string(),
            ));
        } else if !password.is_empty()
            && password::verify_password(password, &user.password_hash)
                .await
                .unwrap_or(false)
        {
            auth_success = true;
        } else {
            debug!("Authentication failed for user attempt");
        }

        if !auth_success {
            tracing::warn!(username = username, "Password authentication failed");
            if let Err(e) = LOGIN_TRACKER.record_failed_login(&mut user, repo).await {
                log::error!("Failed to record failed login: {}", e);
            }
            return Err(AuthError::InvalidCredentials(
                "Invalid username or password".to_string(),
            ));
        }

        if let Err(e) = LOGIN_TRACKER.record_successful_login(&mut user, repo).await {
            log::error!("Failed to record successful login: {}", e);
        }
        tracing::debug!(username = %user.username, role = ?user.role, "Password authentication succeeded");

        Ok(AuthenticatedUser::new(
            user.user_id,
            user.username.clone(),
            user.role,
            user.email,
            connection_info.clone(),
        ))
    }
    .instrument(span)
    .await
}
