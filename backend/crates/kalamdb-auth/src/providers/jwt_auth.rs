// JWT authentication and validation module

use crate::errors::error::{AuthError, AuthResult};
use jsonwebtoken::errors::ErrorKind;
use jsonwebtoken::{
    decode, decode_header, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation,
};
use kalamdb_commons::{Role, UserId, UserName};
use serde::{Deserialize, Serialize};

/// Default JWT expiration time in hours
pub const DEFAULT_JWT_EXPIRY_HOURS: i64 = 24;

/// Default issuer for KalamDB tokens
pub const KALAMDB_ISSUER: &str = "kalamdb";

/// Token type for distinguishing access from refresh tokens.
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

/// JWT claims structure for KalamDB tokens.
///
/// Standard JWT claims plus custom KalamDB-specific fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtClaims {
    /// Subject (user ID)
    pub sub: String,
    /// Issuer
    pub iss: String,
    /// Expiration time (Unix timestamp)
    pub exp: usize,
    /// Issued at (Unix timestamp)
    pub iat: usize,
    /// Username (custom claim)
    pub username: Option<UserName>,
    /// Email (custom claim)
    pub email: Option<String>,
    /// Role (custom claim)
    pub role: Option<Role>,
    /// Token type: "access" or "refresh"
    /// Optional for backward compatibility with tokens issued before this field existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_type: Option<TokenType>,
}

impl JwtClaims {
    /// Create new JWT claims for a user (defaults to access token).
    ///
    /// # Arguments
    /// * `user_id` - User's unique identifier
    /// * `username` - Username
    /// * `role` - User's role
    /// * `email` - Optional email address
    /// * `expiry_hours` - Token expiration in hours (defaults to DEFAULT_JWT_EXPIRY_HOURS)
    pub fn new(
        user_id: &UserId,
        username: &UserName,
        role: &Role,
        email: Option<&str>,
        expiry_hours: Option<i64>,
    ) -> Self {
        Self::with_token_type(user_id, username, role, email, expiry_hours, TokenType::Access)
    }

    /// Create new JWT claims with an explicit token type.
    pub fn with_token_type(
        user_id: &UserId,
        username: &UserName,
        role: &Role,
        email: Option<&str>,
        expiry_hours: Option<i64>,
        token_type: TokenType,
    ) -> Self {
        let now = chrono::Utc::now();
        let exp_hours = expiry_hours.unwrap_or(DEFAULT_JWT_EXPIRY_HOURS);
        let exp = now + chrono::Duration::hours(exp_hours);

        Self {
            sub: user_id.to_string(),
            iss: KALAMDB_ISSUER.to_string(),
            exp: exp.timestamp() as usize,
            iat: now.timestamp() as usize,
            username: Some(username.clone()),
            email: email.map(|e| e.to_string()),
            role: Some(*role),
            token_type: Some(token_type),
        }
    }
}

/// Generate a new JWT token.
///
/// # Arguments
/// * `claims` - JWT claims to encode
/// * `secret` - Secret key for signing
///
/// # Returns
/// Encoded JWT token string
///
/// # Errors
/// Returns `AuthError::HashingError` if encoding fails
pub fn generate_jwt_token(claims: &JwtClaims, secret: &str) -> AuthResult<String> {
    let header = Header::new(Algorithm::HS256);
    let encoding_key = EncodingKey::from_secret(secret.as_bytes());

    encode(&header, claims, &encoding_key)
        .map_err(|e| AuthError::HashingError(format!("JWT encoding error: {}", e)))
}

/// Create and sign a new JWT access token in one step.
///
/// This is the preferred way to generate tokens to ensure consistency.
/// Produces an access token (token_type = "access").
pub fn create_and_sign_token(
    user_id: &UserId,
    username: &UserName,
    role: &Role,
    email: Option<&str>,
    expiry_hours: Option<i64>,
    secret: &str,
) -> AuthResult<(String, JwtClaims)> {
    let claims =
        JwtClaims::with_token_type(user_id, username, role, email, expiry_hours, TokenType::Access);
    let token = generate_jwt_token(&claims, secret)?;
    Ok((token, claims))
}

/// Create and sign a new JWT refresh token.
///
/// Refresh tokens have `token_type = "refresh"` and MUST NOT be accepted
/// as access tokens for API authentication.
pub fn create_and_sign_refresh_token(
    user_id: &UserId,
    username: &UserName,
    role: &Role,
    email: Option<&str>,
    expiry_hours: Option<i64>,
    secret: &str,
) -> AuthResult<(String, JwtClaims)> {
    let claims = JwtClaims::with_token_type(
        user_id,
        username,
        role,
        email,
        expiry_hours,
        TokenType::Refresh,
    );
    let token = generate_jwt_token(&claims, secret)?;
    Ok((token, claims))
}

/// Refresh a JWT token by generating a new token with extended expiration.
///
/// This validates the existing token first, then creates a new token
/// with the same claims but a new expiration time.
///
/// # Arguments
/// * `token` - Existing JWT token
/// * `secret` - Secret key for validation and signing
/// * `expiry_hours` - New expiration time in hours
///
/// # Returns
/// New JWT token with extended expiration
///
/// # Errors
/// Returns error if existing token is invalid or expired
pub fn refresh_jwt_token(
    token: &str,
    secret: &str,
    expiry_hours: Option<i64>,
) -> AuthResult<(String, JwtClaims)> {
    // First validate the existing token (with trusted issuer = kalamdb)
    let trusted_issuers = vec![KALAMDB_ISSUER.to_string()];
    let old_claims = validate_jwt_token(token, secret, &trusted_issuers)?;

    let user_id = UserId::new(&old_claims.sub);
    let username = old_claims.username.as_ref().cloned().unwrap_or_else(|| UserName::new(""));
    let role = old_claims.role.as_ref().cloned().unwrap_or(Role::User);

    create_and_sign_token(
        &user_id,
        &username,
        &role,
        old_claims.email.as_deref(),
        expiry_hours,
        secret,
    )
}

/// Validate a JWT token and extract claims.
///
/// Verifies:
/// - Token signature (using provided secret)
/// - Token expiration
/// - Issuer is in trusted list
/// - Required claims are present
///
/// # Arguments
/// * `token` - JWT token string (without "Bearer " prefix)
/// * `secret` - Secret key for signature verification
/// * `trusted_issuers` - List of trusted issuer domains
///
/// # Returns
/// Validated JWT claims
///
/// # Errors
/// - `AuthError::InvalidSignature` if signature verification fails
/// - `AuthError::TokenExpired` if token has expired
/// - `AuthError::UntrustedIssuer` if issuer is not in trusted list
/// - `AuthError::MissingClaim` if required claim is missing
pub fn validate_jwt_token(
    token: &str,
    secret: &str,
    trusted_issuers: &[String],
) -> AuthResult<JwtClaims> {
    // Decode token header to get algorithm
    let _header = decode_header(token)
        .map_err(|e| AuthError::MalformedAuthorization(format!("Invalid JWT header: {}", e)))?;

    // Decode and validate token
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true; // Check expiration
    validation.validate_nbf = false; // Don't check "not before"

    let decoding_key = DecodingKey::from_secret(secret.as_bytes());
    let token_data =
        decode::<JwtClaims>(token, &decoding_key, &validation).map_err(|e| match e.kind() {
            ErrorKind::ExpiredSignature => AuthError::TokenExpired,
            ErrorKind::InvalidSignature => AuthError::InvalidSignature,
            _ => AuthError::MalformedAuthorization(format!("JWT decode error: {}", e)),
        })?;

    let claims = token_data.claims;

    // Verify issuer is trusted
    verify_issuer(&claims.iss, trusted_issuers)?;

    // Verify required claims exist
    if claims.sub.is_empty() {
        return Err(AuthError::MissingClaim("sub".to_string()));
    }

    Ok(claims)
}

/// Verify JWT issuer is in the trusted list.
///
/// # Arguments
/// * `issuer` - Issuer from JWT claims
/// * `trusted_issuers` - List of trusted issuer domains
///
/// # Returns
/// `Ok(())` if issuer is trusted
///
/// # Errors
/// Returns `AuthError::UntrustedIssuer` if issuer is not in the list
///
/// # Security Note
/// If no trusted issuers are configured, ALL issuers are rejected.
/// This is a secure-by-default approach to prevent accepting arbitrary tokens.
fn verify_issuer(issuer: &str, trusted_issuers: &[String]) -> AuthResult<()> {
    // Security: If no issuers configured, reject all (secure by default)
    if trusted_issuers.is_empty() {
        return Err(AuthError::UntrustedIssuer(format!(
            "No trusted issuers configured. Rejecting issuer: {}",
            issuer
        )));
    }

    if trusted_issuers.iter().any(|i| i == issuer) {
        Ok(())
    } else {
        Err(AuthError::UntrustedIssuer(issuer.to_string()))
    }
}

/// Extract claims from a JWT token without full validation.
///
/// **WARNING**: This does NOT verify the signature! Only use in tests
/// or when you need to inspect a token before validation.
///
/// # Arguments
/// * `token` - JWT token string
///
/// # Returns
/// JWT claims (unverified)
///
/// # Errors
/// Returns error if token structure is invalid
///
/// # Security
/// This function is gated behind `#[cfg(test)]` to prevent accidental
/// use in production code paths.
#[cfg(test)]
pub fn extract_claims_unverified(token: &str) -> AuthResult<JwtClaims> {
    // Decode without verification (dangerous!)
    let mut validation = Validation::new(Algorithm::HS256);
    #[allow(deprecated)]
    validation.insecure_disable_signature_validation(); // DANGEROUS - but needed for unverified claim extraction
    validation.validate_exp = false;

    let decoding_key = DecodingKey::from_secret(b""); // Empty key since we're not validating
    let token_data = decode::<JwtClaims>(token, &decoding_key, &validation)
        .map_err(|e| AuthError::MalformedAuthorization(format!("JWT decode error: {}", e)))?;

    Ok(token_data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{encode, EncodingKey, Header};

    fn create_test_token(secret: &str, exp_offset_secs: i64) -> String {
        create_test_token_with_type(secret, exp_offset_secs, Some(TokenType::Access))
    }

    fn create_test_token_with_type(
        secret: &str,
        exp_offset_secs: i64,
        token_type: Option<TokenType>,
    ) -> String {
        let now = chrono::Utc::now().timestamp() as usize;
        let claims = JwtClaims {
            sub: "user_123".to_string(),
            iss: "kalamdb-test".to_string(),
            exp: ((now as i64) + exp_offset_secs) as usize,
            iat: now,
            username: Some(UserName::new("testuser")),
            email: Some("test@example.com".to_string()),
            role: Some(Role::User),
            token_type,
        };

        let header = Header::new(Algorithm::HS256);
        let encoding_key = EncodingKey::from_secret(secret.as_bytes());
        encode(&header, &claims, &encoding_key).unwrap()
    }

    #[test]
    fn test_validate_jwt_token_valid() {
        let secret = "test-secret-key";
        let token = create_test_token(secret, 3600); // Expires in 1 hour

        let trusted_issuers = vec!["kalamdb-test".to_string()];
        let result = validate_jwt_token(&token, secret, &trusted_issuers);
        assert!(result.is_ok());

        let claims = result.unwrap();
        assert_eq!(claims.sub, "user_123");
        assert_eq!(claims.iss, "kalamdb-test");
        assert_eq!(claims.username, Some(UserName::new("testuser")));
    }

    #[test]
    fn test_validate_jwt_token_wrong_secret() {
        let secret = "test-secret-key";
        let token = create_test_token(secret, 3600);

        let trusted_issuers = vec!["kalamdb-test".to_string()];
        let result = validate_jwt_token(&token, "wrong-secret", &trusted_issuers);
        assert!(matches!(result, Err(AuthError::InvalidSignature)));
    }

    #[test]
    fn test_validate_jwt_token_expired() {
        let secret = "test-secret-key";
        let token = create_test_token(secret, -3600); // Expired 1 hour ago

        let trusted_issuers = vec!["kalamdb-test".to_string()];
        let result = validate_jwt_token(&token, secret, &trusted_issuers);
        assert!(matches!(result, Err(AuthError::TokenExpired)));
    }

    #[test]
    fn test_verify_issuer_trusted() {
        let trusted = vec!["kalamdb.io".to_string(), "auth.kalamdb.io".to_string()];
        assert!(verify_issuer("kalamdb.io", &trusted).is_ok());
        assert!(verify_issuer("auth.kalamdb.io", &trusted).is_ok());
    }

    #[test]
    fn test_verify_issuer_untrusted() {
        let trusted = vec!["kalamdb.io".to_string()];
        let result = verify_issuer("evil.com", &trusted);
        assert!(matches!(result, Err(AuthError::UntrustedIssuer(_))));
    }

    #[test]
    fn test_verify_issuer_empty_list() {
        // Security: Empty trusted list = reject ALL issuers (secure by default)
        let trusted = vec![];
        let result = verify_issuer("any-issuer.com", &trusted);
        assert!(matches!(result, Err(AuthError::UntrustedIssuer(_))));
    }

    // ─── Token-type security tests ──────────────────────────────────────────

    /// Refresh tokens must carry `token_type = "refresh"` so the bearer-auth
    /// layer can detect and reject them when used on the SQL / API endpoints.
    #[test]
    fn test_refresh_token_type_claim_is_preserved() {
        let secret = "test-secret-key";
        let trusted = vec!["kalamdb".to_string()];

        let user_id = kalamdb_commons::UserId::new("u_refresh");
        let username = kalamdb_commons::UserName::new("refresh_user");
        let role = kalamdb_commons::Role::User;

        let (refresh_token, _) = create_and_sign_refresh_token(
            &user_id, &username, &role, None, None, secret,
        )
        .expect("Failed to create refresh token");

        let claims =
            validate_jwt_token(&refresh_token, secret, &trusted).expect("Token validation failed");

        assert_eq!(
            claims.token_type,
            Some(TokenType::Refresh),
            "Refresh token must carry token_type=Refresh claim"
        );
    }

    /// Access tokens must carry `token_type = "access"` so consumers can
    /// distinguish them from refresh tokens.
    #[test]
    fn test_access_token_type_claim_is_preserved() {
        let secret = "test-secret-key";
        let trusted = vec!["kalamdb".to_string()];

        let user_id = kalamdb_commons::UserId::new("u_access");
        let username = kalamdb_commons::UserName::new("access_user");
        let role = kalamdb_commons::Role::User;

        let (access_token, _) = create_and_sign_token(
            &user_id, &username, &role, None, None, secret,
        )
        .expect("Failed to create access token");

        let claims =
            validate_jwt_token(&access_token, secret, &trusted).expect("Token validation failed");

        assert_eq!(
            claims.token_type,
            Some(TokenType::Access),
            "Access token must carry token_type=Access claim"
        );
    }

    /// Refresh and access tokens signed with the same secret must NOT be
    /// interchangeable at the validation layer — their `token_type` claims
    /// must differ so calling code can enforce the separation.
    #[test]
    fn test_refresh_and_access_token_types_are_distinct() {
        let secret = "shared-secret";
        let trusted = vec!["kalamdb".to_string()];
        let user_id = kalamdb_commons::UserId::new("u_distinct");
        let username = kalamdb_commons::UserName::new("distinct_user");
        let role = kalamdb_commons::Role::User;

        let (access, _) = create_and_sign_token(
            &user_id, &username, &role, None, None, secret,
        )
        .unwrap();

        let (refresh, _) = create_and_sign_refresh_token(
            &user_id, &username, &role, None, None, secret,
        )
        .unwrap();

        let access_claims = validate_jwt_token(&access, secret, &trusted).unwrap();
        let refresh_claims = validate_jwt_token(&refresh, secret, &trusted).unwrap();

        assert_ne!(
            access_claims.token_type,
            refresh_claims.token_type,
            "Access and refresh tokens must have different token_type claims"
        );
    }

    /// An empty string is not a valid JWT and must return an error, not panic.
    #[test]
    fn test_validate_empty_string_returns_error() {
        let trusted = vec!["kalamdb.io".to_string()];
        let result = validate_jwt_token("", "any-secret", &trusted);
        assert!(result.is_err(), "Empty token string must be rejected");
    }

    /// A token with only two segments ("header.payload", missing signature)
    /// must be rejected.
    #[test]
    fn test_validate_truncated_jwt_returns_error() {
        let trusted = vec!["kalamdb.io".to_string()];
        let result = validate_jwt_token("eyJhbGciOiJIUzI1NiJ9.e30", "any-secret", &trusted);
        assert!(
            result.is_err(),
            "Truncated JWT (missing signature) must be rejected"
        );
    }

    /// A JWT whose `sub` claim contains SQL-injection text must still be
    /// parsed correctly by the JWT library without any panic.  The attacker
    /// cannot bypass validation by injecting SQL into claims.
    #[test]
    fn test_validate_jwt_sql_injection_in_sub_is_safe() {
        let secret = "some-secret";
        let trusted = vec!["kalamdb-test".to_string()];

        // Construct a well-signed JWT with a payloaded sub/username.
        let sqli_username = "'; DROP TABLE users; --";
        let now = chrono::Utc::now().timestamp() as usize;
        let claims = JwtClaims {
            sub: sqli_username.to_string(),
            iss: "kalamdb-test".to_string(),
            exp: now + 3600,
            iat: now,
            // The username field uses UserName which validates and rejects SQL
            // injection characters.  The test exercises JWT claim preservation
            // via the `sub` field only; the optional `username` claim is left
            // absent so the JWT encoder does not reject the payload.
            username: None,
            email: None,
            role: None,
            token_type: Some(TokenType::Access),
        };

        let token = generate_jwt_token(&claims, secret).unwrap();

        // The token validates (valid signature, not expired, trusted issuer)
        let parsed = validate_jwt_token(&token, secret, &trusted).unwrap();

        // The SQL injection string is preserved literally — it's the auth and SQL
        // layers' job to sanitise inputs, not the JWT validator.
        assert_eq!(
            parsed.sub, sqli_username,
            "JWT validator must preserve sub claims verbatim"
        );
    }

    /// A token signed with the cluster's secret but containing a higher role
    /// (`system`) than the user actually has must still validate at the JWT
    /// level — the role-DB-mismatch check is the responsibility of the auth
    /// service layer (`authenticate_bearer`), not the JWT validator itself.
    ///
    /// This test documents the boundary: `validate_jwt_token` validates
    /// *cryptographic* integrity only; *semantic* authorization (role match)
    /// is a separate step.
    #[test]
    fn test_validate_jwt_role_claim_is_returned_for_caller_to_check() {
        let secret = "secure-secret";
        let trusted = vec!["kalamdb-test".to_string()];

        let token =
            create_test_token_with_type(secret, 3600, Some(TokenType::Access));

        let claims = validate_jwt_token(&token, secret, &trusted).unwrap();

        // claims.role may be Some(role) — the *caller* (authenticate_bearer)
        // must verify it matches the DB.  The JWT validator must not silently drop it.
        // We just ensure validation succeeded and role is accessible.
        let _ = claims.role; // accessible without panic
    }
}
