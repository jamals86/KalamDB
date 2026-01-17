// OAuth authentication and token validation module
// Phase 10, User Story 8: OAuth Integration

use crate::error::{AuthError, AuthResult};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};

/// OAuth token claims structure.
///
/// Supports standard OAuth/OIDC claims from providers like Google, GitHub, Azure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthClaims {
    /// Subject (unique user identifier from OAuth provider)
    pub sub: String,
    /// Issuer (OAuth provider's issuer URL)
    pub iss: String,
    /// Expiration time (Unix timestamp)
    pub exp: usize,
    /// Issued at (Unix timestamp)
    #[serde(default)]
    pub iat: Option<usize>,
    /// Email (optional, from OAuth provider)
    #[serde(default)]
    pub email: Option<String>,
    /// Email verified flag (optional, from OAuth provider)
    #[serde(default)]
    pub email_verified: Option<bool>,
    /// Name (optional, from OAuth provider)
    #[serde(default)]
    pub name: Option<String>,
    /// Audience (optional, OAuth client ID)
    #[serde(default)]
    pub aud: Option<String>,
}

/// OAuth provider and subject extracted from token.
#[derive(Debug, Clone)]
pub struct OAuthIdentity {
    /// OAuth provider name (e.g., "google", "github", "azure")
    pub provider: String,
    /// Subject (unique user identifier from OAuth provider)
    pub subject: String,
    /// Email (if available from provider)
    pub email: Option<String>,
    /// Name (if available from provider)
    pub name: Option<String>,
}

/// Validate an OAuth token and extract claims.
///
/// Verifies:
/// - Token signature (using provided secret or JWKS)
/// - Token expiration
/// - Issuer matches expected provider issuer
/// - Required claims are present
///
/// # Arguments
/// * `token` - OAuth token string (without "Bearer " prefix)
/// * `secret` - Secret key for signature verification (HS256) or empty for RS256/JWKS
/// * `expected_issuer` - Expected issuer URL from OAuth provider
///
/// # Returns
/// Validated OAuth claims
///
/// # Errors
/// - `AuthError::InvalidSignature` if signature verification fails
/// - `AuthError::TokenExpired` if token has expired
/// - `AuthError::UntrustedIssuer` if issuer doesn't match expected
/// - `AuthError::MissingClaim` if required claim is missing
pub fn validate_oauth_token(
    token: &str,
    secret: &str,
    expected_issuer: &str,
) -> AuthResult<OAuthClaims> {
    // Decode token header to get algorithm
    let header = decode_header(token).map_err(|e| {
        AuthError::MalformedAuthorization(format!("Invalid OAuth token header: {}", e))
    })?;

    // Determine algorithm (OAuth providers typically use RS256, but we support HS256 for testing)
    let algorithm = header.alg;

    // Decode and validate token
    let mut validation = Validation::new(algorithm);
    validation.validate_exp = true; // Check expiration
    validation.validate_nbf = false; // Don't check "not before"
    validation.validate_aud = false; // Don't validate audience automatically
                                     // Note: We manually check issuer after decoding instead of using set_issuer
                                     // because set_issuer requires exact match which may fail with trailing slashes

    let decoding_key = match algorithm {
        Algorithm::HS256 => DecodingKey::from_secret(secret.as_bytes()),
        Algorithm::RS256 | Algorithm::RS384 | Algorithm::RS512 => {
            // For RS256, we would need JWKS support
            // For now, return error - JWKS support can be added later
            return Err(AuthError::MalformedAuthorization(
                "RS256 tokens require JWKS support (not yet implemented)".to_string(),
            ));
        },
        _ => {
            return Err(AuthError::MalformedAuthorization(format!(
                "Unsupported algorithm: {:?}",
                algorithm
            )));
        },
    };

    let token_data = decode::<OAuthClaims>(token, &decoding_key, &validation).map_err(|e| {
        if e.to_string().contains("ExpiredSignature") {
            AuthError::TokenExpired
        } else if e.to_string().contains("InvalidSignature") {
            AuthError::InvalidSignature
        } else if e.to_string().contains("InvalidIssuer") {
            AuthError::UntrustedIssuer(expected_issuer.to_string())
        } else {
            AuthError::MalformedAuthorization(format!("OAuth token decode error: {}", e))
        }
    })?;

    let claims = token_data.claims;

    // Manually verify issuer matches expected
    if claims.iss != expected_issuer {
        return Err(AuthError::UntrustedIssuer(claims.iss.clone()));
    }

    // Verify required claims exist
    if claims.sub.is_empty() {
        return Err(AuthError::MissingClaim("sub".to_string()));
    }

    if claims.iss.is_empty() {
        return Err(AuthError::MissingClaim("iss".to_string()));
    }

    Ok(claims)
}

/// Extract provider name and subject from OAuth token claims.
///
/// Maps issuer URLs to provider names:
/// - `https://accounts.google.com` -> "google"
/// - `https://github.com` -> "github"
/// - `https://login.microsoftonline.com/...` -> "azure"
///
/// # Arguments
/// * `claims` - OAuth token claims
///
/// # Returns
/// OAuth identity with provider name and subject
pub fn extract_provider_and_subject(claims: &OAuthClaims) -> OAuthIdentity {
    let provider = match claims.iss.as_str() {
        iss if iss.starts_with("https://accounts.google.com") => "google",
        iss if iss.starts_with("https://github.com") => "github",
        iss if iss.starts_with("https://login.microsoftonline.com") => "azure",
        _ => "unknown",
    };

    OAuthIdentity {
        provider: provider.to_string(),
        subject: claims.sub.clone(),
        email: claims.email.clone(),
        name: claims.name.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{encode, EncodingKey, Header};

    fn create_test_oauth_token(secret: &str, issuer: &str, exp_offset_secs: i64) -> String {
        let now = chrono::Utc::now().timestamp() as usize;
        let claims = OAuthClaims {
            sub: "oauth_user_12345".to_string(),
            iss: issuer.to_string(),
            exp: ((now as i64) + exp_offset_secs) as usize,
            iat: Some(now),
            email: Some("user@example.com".to_string()),
            email_verified: Some(true),
            name: Some("Test User".to_string()),
            aud: Some("client-id-123".to_string()),
        };

        let header = Header::new(Algorithm::HS256);
        let encoding_key = EncodingKey::from_secret(secret.as_bytes());
        encode(&header, &claims, &encoding_key).unwrap()
    }

    #[test]
    fn test_validate_oauth_token_valid() {
        let secret = "oauth-test-secret";
        let issuer = "https://accounts.google.com";
        let token = create_test_oauth_token(secret, issuer, 3600); // Expires in 1 hour

        let result = validate_oauth_token(&token, secret, issuer);
        if let Err(ref e) = result {
            eprintln!("Validation error: {:?}", e);
        }
        assert!(result.is_ok());

        let claims = result.unwrap();
        assert_eq!(claims.sub, "oauth_user_12345");
        assert_eq!(claims.iss, issuer);
        assert_eq!(claims.email, Some("user@example.com".to_string()));
    }

    #[test]
    fn test_validate_oauth_token_wrong_secret() {
        let secret = "oauth-test-secret";
        let issuer = "https://accounts.google.com";
        let token = create_test_oauth_token(secret, issuer, 3600);

        let result = validate_oauth_token(&token, "wrong-secret", issuer);
        assert!(matches!(result, Err(AuthError::InvalidSignature)));
    }

    #[test]
    fn test_validate_oauth_token_expired() {
        let secret = "oauth-test-secret";
        let issuer = "https://accounts.google.com";
        let token = create_test_oauth_token(secret, issuer, -3600); // Expired 1 hour ago

        let result = validate_oauth_token(&token, secret, issuer);
        assert!(matches!(result, Err(AuthError::TokenExpired)));
    }

    #[test]
    fn test_validate_oauth_token_wrong_issuer() {
        let secret = "oauth-test-secret";
        let issuer = "https://accounts.google.com";
        let token = create_test_oauth_token(secret, issuer, 3600);

        // Try to validate with different expected issuer
        let result = validate_oauth_token(&token, secret, "https://github.com");
        if let Err(ref e) = result {
            eprintln!("Expected error: {:?}", e);
        }
        assert!(matches!(result, Err(AuthError::UntrustedIssuer(_))));
    }

    #[test]
    fn test_extract_provider_and_subject_google() {
        let claims = OAuthClaims {
            sub: "google_user_123".to_string(),
            iss: "https://accounts.google.com".to_string(),
            exp: 0,
            iat: None,
            email: Some("test@gmail.com".to_string()),
            email_verified: Some(true),
            name: Some("Google User".to_string()),
            aud: None,
        };

        let identity = extract_provider_and_subject(&claims);
        assert_eq!(identity.provider, "google");
        assert_eq!(identity.subject, "google_user_123");
        assert_eq!(identity.email, Some("test@gmail.com".to_string()));
    }

    #[test]
    fn test_extract_provider_and_subject_github() {
        let claims = OAuthClaims {
            sub: "github_user_456".to_string(),
            iss: "https://github.com".to_string(),
            exp: 0,
            iat: None,
            email: Some("test@github.com".to_string()),
            email_verified: None,
            name: Some("GitHub User".to_string()),
            aud: None,
        };

        let identity = extract_provider_and_subject(&claims);
        assert_eq!(identity.provider, "github");
        assert_eq!(identity.subject, "github_user_456");
    }

    #[test]
    fn test_extract_provider_and_subject_azure() {
        let claims = OAuthClaims {
            sub: "azure_user_789".to_string(),
            iss: "https://login.microsoftonline.com/tenant-id/v2.0".to_string(),
            exp: 0,
            iat: None,
            email: Some("test@microsoft.com".to_string()),
            email_verified: Some(true),
            name: Some("Azure User".to_string()),
            aud: None,
        };

        let identity = extract_provider_and_subject(&claims);
        assert_eq!(identity.provider, "azure");
        assert_eq!(identity.subject, "azure_user_789");
    }

    #[test]
    fn test_extract_provider_and_subject_unknown() {
        let claims = OAuthClaims {
            sub: "unknown_user_999".to_string(),
            iss: "https://unknown-provider.com".to_string(),
            exp: 0,
            iat: None,
            email: None,
            email_verified: None,
            name: None,
            aud: None,
        };

        let identity = extract_provider_and_subject(&claims);
        assert_eq!(identity.provider, "unknown");
        assert_eq!(identity.subject, "unknown_user_999");
    }
}
