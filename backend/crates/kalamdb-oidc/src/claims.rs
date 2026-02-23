// JWT claims and token type definitions shared across internal and external
// (OIDC) authentication paths.

use kalamdb_commons::{Role, UserId, UserName};
use serde::{Deserialize, Serialize};

/// Default JWT expiration time in hours.
pub const DEFAULT_JWT_EXPIRY_HOURS: i64 = 24;

/// Token type for distinguishing access from refresh tokens.
///
/// Stored in the `token_type` JWT claim to prevent refresh tokens from
/// being used for API authentication.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TokenType {
    Access,
    Refresh,
}

impl std::fmt::Display for TokenType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenType::Access => write!(f, "access"),
            TokenType::Refresh => write!(f, "refresh"),
        }
    }
}

/// JWT claims structure used by both internally-issued (HS256) and
/// externally-issued OIDC (RS256/ES256) tokens.
///
/// Standard JWT claims (`sub`, `iss`, `exp`, `iat`) plus KalamDB-specific
/// custom fields (`username`, `email`, `role`, `token_type`).
///
/// The `username` field accepts the OIDC-standard `preferred_username` alias
/// for seamless interop with Keycloak, Auth0, and similar providers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtClaims {
    /// Subject (user ID for internal tokens; provider subject for OIDC)
    pub sub: String,
    /// Issuer (`"kalamdb"` for internal; provider URL for OIDC)
    pub iss: String,
    /// Expiration time (Unix timestamp seconds)
    pub exp: usize,
    /// Issued at (Unix timestamp seconds)
    pub iat: usize,
    /// Username.  Also accepts the OIDC-standard `preferred_username` claim
    /// from external providers (Keycloak, Auth0, …).
    #[serde(alias = "preferred_username")]
    pub username: Option<UserName>,
    /// Email address
    pub email: Option<String>,
    /// User role (custom KalamDB claim)
    pub role: Option<Role>,
    /// Token type: `"access"` or `"refresh"`.
    /// Optional for backward compatibility with tokens issued before this
    /// field was introduced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_type: Option<TokenType>,
}

impl JwtClaims {
    /// Create new JWT claims for a user (defaults to `TokenType::Access`).
    pub fn new(
        user_id: &UserId,
        username: &UserName,
        role: &Role,
        email: Option<&str>,
        expiry_hours: Option<i64>,
        issuer: &str,
    ) -> Self {
        Self::with_token_type(
            user_id,
            username,
            role,
            email,
            expiry_hours,
            TokenType::Access,
            issuer,
        )
    }

    /// Create new JWT claims with an explicit token type.
    pub fn with_token_type(
        user_id: &UserId,
        username: &UserName,
        role: &Role,
        email: Option<&str>,
        expiry_hours: Option<i64>,
        token_type: TokenType,
        issuer: &str,
    ) -> Self {
        let now = chrono::Utc::now();
        let exp_hours = expiry_hours.unwrap_or(DEFAULT_JWT_EXPIRY_HOURS);
        let exp = now + chrono::Duration::hours(exp_hours);

        Self {
            sub: user_id.to_string(),
            iss: issuer.to_string(),
            exp: exp.timestamp() as usize,
            iat: now.timestamp() as usize,
            username: Some(username.clone()),
            email: email.map(|e| e.to_string()),
            role: Some(*role),
            token_type: Some(token_type),
        }
    }
}
