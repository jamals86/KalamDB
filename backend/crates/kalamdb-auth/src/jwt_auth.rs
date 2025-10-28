// JWT authentication and validation module

use crate::error::{AuthError, AuthResult};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};

/// JWT claims structure for KalamDB tokens.
///
/// Standard JWT claims plus custom KalamDB-specific fields.
#[derive(Debug, Serialize, Deserialize)]
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
    pub username: Option<String>,
    /// Email (custom claim)
    pub email: Option<String>,
    /// Role (custom claim)
    pub role: Option<String>,
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
    let token_data = decode::<JwtClaims>(token, &decoding_key, &validation).map_err(|e| {
        if e.to_string().contains("ExpiredSignature") {
            AuthError::TokenExpired
        } else if e.to_string().contains("InvalidSignature") {
            AuthError::InvalidSignature
        } else {
            AuthError::MalformedAuthorization(format!("JWT decode error: {}", e))
        }
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
fn verify_issuer(issuer: &str, trusted_issuers: &[String]) -> AuthResult<()> {
    if trusted_issuers.is_empty() {
        // If no issuers configured, accept any
        return Ok(());
    }

    if trusted_issuers.iter().any(|i| i == issuer) {
        Ok(())
    } else {
        Err(AuthError::UntrustedIssuer(issuer.to_string()))
    }
}

/// Extract claims from a JWT token without full validation.
///
/// **WARNING**: This does NOT verify the signature! Only use for debugging
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
pub fn extract_claims_unverified(token: &str) -> AuthResult<JwtClaims> {
    // Decode without verification (dangerous!)
    let mut validation = Validation::new(Algorithm::HS256);
    validation.insecure_disable_signature_validation(); // DANGEROUS
    validation.validate_exp = false;

    let decoding_key = DecodingKey::from_secret(b""); // Empty key since we're not validating
    let token_data = decode::<JwtClaims>(token, &decoding_key, &validation).map_err(|e| {
        AuthError::MalformedAuthorization(format!("JWT decode error: {}", e))
    })?;

    Ok(token_data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{encode, EncodingKey, Header};

    fn create_test_token(secret: &str, exp_offset_secs: i64) -> String {
        let now = chrono::Utc::now().timestamp() as usize;
        let claims = JwtClaims {
            sub: "user_123".to_string(),
            iss: "kalamdb-test".to_string(),
            exp: ((now as i64) + exp_offset_secs) as usize,
            iat: now,
            username: Some("testuser".to_string()),
            email: Some("test@example.com".to_string()),
            role: Some("user".to_string()),
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
        assert_eq!(claims.username, Some("testuser".to_string()));
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
        // Empty trusted list = accept any issuer
        let trusted = vec![];
        assert!(verify_issuer("any-issuer.com", &trusted).is_ok());
    }
}
