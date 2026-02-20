use crate::errors::error::{AuthError, AuthResult};
use crate::models::context::AuthenticatedUser;
use crate::providers::jwt_auth;
use crate::providers::jwt_config;
use crate::repository::user_repo::UserRepository;
use kalamdb_commons::models::{ConnectionInfo, UserId, UserName};
use kalamdb_commons::{AuthType, Role};
use kalamdb_system::providers::storages::models::StorageMode;
use kalamdb_system::User;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tracing::Instrument;

/// Known OIDC provider codes (3 characters) derived from issuer URLs.
/// The code is used to compose a unique username: `oidc:{code}:{subject}`.
/// This ensures users from different providers with the same subject ID
/// never collide, while staying short and readable.
/// TODO: We can have a faster lookup for well-known providers and only hash unknown ones.
fn provider_code_from_issuer(issuer: &str) -> String {
    // Well-known providers get deterministic codes
    let lower = issuer.to_lowercase();
    if lower.contains("keycloak") || lower.contains("/realms/") {
        return "kcl".to_string();
    }
    if lower.contains("accounts.google") {
        return "ggl".to_string();
    }
    if lower.contains("github") {
        return "ghb".to_string();
    }
    if lower.contains("login.microsoftonline") || lower.contains("sts.windows.net") {
        return "msf".to_string();
    }
    if lower.contains("auth0") {
        return "a0x".to_string();
    }
    if lower.contains("okta") {
        return "okt".to_string();
    }

    // Fallback: hash the issuer and take 3 hex chars
    let mut hasher = Sha256::new();
    hasher.update(issuer.as_bytes());
    let hash = hex::encode(hasher.finalize());
    hash[..3].to_string()
}

/// Compose a deterministic, index-friendly username for an OIDC provider user.
///
/// Format: `oidc:{3-char-provider-code}:{subject}`
///
/// This is stored as the user's `username` in the system, so lookups use the
/// existing username secondary index (O(1)) instead of scanning all users.
/// TODO: This will hurt the sharding distribution of provider users, i prefer adding the provider code as a prefix to the username instead of suffixing it, but we can revisit this if it becomes an issue.
pub(crate) fn compose_provider_username(issuer: &str, subject: &str) -> UserName {
    let code = provider_code_from_issuer(issuer);
    UserName::new(format!("oidc:{}:{}", code, subject))
}

pub(super) async fn authenticate_bearer(
    token: &str,
    connection_info: &ConnectionInfo,
    repo: &Arc<dyn UserRepository>,
) -> AuthResult<AuthenticatedUser> {
    let span = tracing::info_span!(
        "auth.bearer",
        token_len = token.len(),
        is_localhost = connection_info.is_localhost()
    );

    async move {
        let config = jwt_config::get_jwt_config();
        let claims = jwt_auth::validate_jwt_token(token, &config.secret, &config.trusted_issuers)?;

        if let Some(ref tt) = claims.token_type {
            if *tt == jwt_auth::TokenType::Refresh {
                log::warn!("Refresh token used as access token for user={}", claims.sub);
                return Err(AuthError::InvalidCredentials(
                    "Refresh tokens cannot be used for API authentication".to_string(),
                ));
            }
        }

        let issuer = claims.iss.clone();
        let subject = claims.sub.clone();

        // Compose the deterministic provider username (oidc:{code}:{subject})
        // This is the canonical username stored for OIDC users and uses the
        // existing username secondary index for O(1) lookup.
        let provider_username = compose_provider_username(&issuer, &subject);

        let user = resolve_or_provision_provider_user(
            &provider_username,
            &issuer,
            &subject,
            &claims,
            config.auto_create_users_from_provider,
            repo,
        )
        .await?;

        if user.deleted_at.is_some() {
            return Err(AuthError::InvalidCredentials("Invalid username or password".to_string()));
        }

        let role = if let Some(claimed_role) = &claims.role {
            if *claimed_role != user.role {
                log::warn!(
                    "JWT role mismatch: claimed={:?}, actual={:?} for user={}",
                    claimed_role,
                    user.role,
                    user.username
                );
                return Err(AuthError::InvalidCredentials(
                    "Token role does not match user role".to_string(),
                ));
            }
            *claimed_role
        } else {
            user.role
        };

        tracing::debug!(username = %user.username, role = ?role, "Bearer authentication succeeded");

        Ok(AuthenticatedUser::new(
            user.user_id.clone(),
            user.username.clone(),
            role,
            user.email.clone(),
            connection_info.clone(),
        ))
    }
    .instrument(span)
    .await
}

/// Look up a provider user by their composed username (index-backed, O(1)).
/// If not found and auto-provisioning is enabled, create the user.
async fn resolve_or_provision_provider_user(
    provider_username: &UserName,
    issuer: &str,
    subject: &str,
    claims: &jwt_auth::JwtClaims,
    auto_create_users_from_provider: bool,
    repo: &Arc<dyn UserRepository>,
) -> AuthResult<User> {
    match repo.get_user_by_username(provider_username).await {
        Ok(user) => Ok(user),
        Err(AuthError::UserNotFound(_)) => {
            maybe_auto_provision_provider_user(
                claims,
                issuer,
                subject,
                provider_username,
                auto_create_users_from_provider,
                repo,
            )
            .await
        },
        Err(e) => Err(e),
    }
}
/// TODO: Remove this and use the same logic we use for other userid generation to generate the provider user id. We can keep the deterministic username composition for lookups, but the user id can be generated in a more standard way instead of hashing the username.
fn build_provider_user_id(issuer: &str, subject: &str) -> UserId {
    let mut hasher = Sha256::new();
    hasher.update(issuer.as_bytes());
    hasher.update(b":");
    hasher.update(subject.as_bytes());
    let hash = hex::encode(hasher.finalize());
    UserId::new(format!("u_oidc_{}", &hash[..16]))
}

/// Auto-provision a new OIDC user when `auto_create_users_from_provider` is enabled.
///
/// The username is the deterministic `oidc:{code}:{subject}` composed earlier,
/// so subsequent logins resolve via the username index with zero scanning.
async fn maybe_auto_provision_provider_user(
    claims: &jwt_auth::JwtClaims,
    issuer: &str,
    subject: &str,
    provider_username: &UserName,
    auto_create_users_from_provider: bool,
    repo: &Arc<dyn UserRepository>,
) -> AuthResult<User> {
    if !auto_create_users_from_provider {
        return Err(AuthError::UserNotFound(format!(
            "User not found for provider and subject (username='{}')",
            provider_username
        )));
    }

    let user_id = build_provider_user_id(issuer, subject);

    let auth_data = serde_json::json!({
        "provider": issuer,
        "subject": subject,
    })
    .to_string();

    let now = chrono::Utc::now().timestamp_millis();
    let user = User {
        created_at: now,
        updated_at: now,
        locked_until: None,
        last_login_at: Some(now),
        last_seen: Some(now),
        deleted_at: None,
        user_id,
        username: provider_username.clone(),
        password_hash: String::new(),
        email: claims.email.clone(),
        auth_data: Some(auth_data),
        storage_id: None,
        failed_login_attempts: 0,
        role: Role::User,
        auth_type: AuthType::OAuth,
        storage_mode: StorageMode::Table,
    };

    repo.create_user(user.clone()).await?;
    Ok(user)
}
