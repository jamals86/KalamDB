//! JWT authentication and claims extraction
//!
//! This module provides JWT token validation and user identity extraction
//! using the jsonwebtoken crate.

use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use kalamdb_commons::models::UserId;
use serde::{Deserialize, Serialize};
use std::fmt;

/// JWT authentication errors
#[derive(Debug)]
pub enum JwtError {
    /// Token is missing from request
    MissingToken,

    /// Token format is invalid (not "Bearer <token>")
    InvalidFormat,

    /// Token signature verification failed
    InvalidSignature(String),

    /// Token has expired
    Expired,

    /// Token claims are missing required fields
    MissingClaims(String),

    /// Token algorithm is not supported
    UnsupportedAlgorithm,
}

impl fmt::Display for JwtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JwtError::MissingToken => write!(f, "Missing JWT token"),
            JwtError::InvalidFormat => {
                write!(f, "Invalid token format (expected 'Bearer <token>')")
            }
            JwtError::InvalidSignature(msg) => write!(f, "Invalid token signature: {}", msg),
            JwtError::Expired => write!(f, "Token has expired"),
            JwtError::MissingClaims(field) => write!(f, "Missing required claim: {}", field),
            JwtError::UnsupportedAlgorithm => write!(f, "Unsupported token algorithm"),
        }
    }
}

impl std::error::Error for JwtError {}

/// JWT claims structure
///
/// Standard JWT claims with KalamDB-specific fields
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    /// Subject (user ID)
    pub sub: String,

    /// Issued at (Unix timestamp)
    pub iat: u64,

    /// Expiration time (Unix timestamp)
    pub exp: u64,

    /// User ID (KalamDB-specific)
    pub user_id: String,

    /// Optional: User roles/permissions
    #[serde(default)]
    pub roles: Vec<String>,
}

impl Claims {
    /// Extract UserId from claims
    pub fn user_id(&self) -> UserId {
        UserId::from(self.user_id.as_str())
    }
}

/// JWT authentication service
pub struct JwtAuth {
    /// Secret key for HMAC validation or public key for RSA/ECDSA
    secret: String,

    /// Token validation configuration
    validation: Validation,
}

impl JwtAuth {
    /// Create a new JWT authentication service
    ///
    /// # Arguments
    /// * `secret` - Secret key for HMAC or public key for RSA/ECDSA
    /// * `algorithm` - JWT algorithm (default: HS256)
    ///
    /// # Example
    /// ```rust,ignore
    /// let auth = JwtAuth::new("my-secret-key".to_string(), Algorithm::HS256);
    /// ```
    pub fn new(secret: String, algorithm: Algorithm) -> Self {
        let mut validation = Validation::new(algorithm);
        validation.validate_exp = true;
        validation.leeway = 60; // 60 seconds leeway for clock skew

        Self { secret, validation }
    }

    /// Validate JWT token and extract claims
    ///
    /// # Arguments
    /// * `token` - Raw JWT token string (without "Bearer " prefix)
    ///
    /// # Returns
    /// * `Ok(Claims)` - Validated claims
    /// * `Err(JwtError)` - Validation error
    ///
    /// # Example
    /// ```rust,ignore
    /// let claims = auth.validate_token(token)?;
    /// println!("User ID: {}", claims.user_id);
    /// ```
    pub fn validate_token(&self, token: &str) -> Result<Claims, JwtError> {
        // Check algorithm support
        let header = decode_header(token).map_err(|e| JwtError::InvalidSignature(e.to_string()))?;

        if header.alg != self.validation.algorithms[0] {
            return Err(JwtError::UnsupportedAlgorithm);
        }

        // Decode and validate token
        let token_data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.secret.as_bytes()),
            &self.validation,
        )
        .map_err(|e| {
            if e.to_string().contains("ExpiredSignature") {
                JwtError::Expired
            } else {
                JwtError::InvalidSignature(e.to_string())
            }
        })?;

        // Validate required claims
        let claims = token_data.claims;
        if claims.user_id.is_empty() {
            return Err(JwtError::MissingClaims("user_id".to_string()));
        }

        Ok(claims)
    }

    /// Extract token from Authorization header
    ///
    /// # Arguments
    /// * `auth_header` - Authorization header value (e.g., "Bearer <token>")
    ///
    /// # Returns
    /// * `Ok(&str)` - Extracted token
    /// * `Err(JwtError)` - Extraction error
    pub fn extract_token(auth_header: &str) -> Result<&str, JwtError> {
        if !auth_header.starts_with("Bearer ") {
            return Err(JwtError::InvalidFormat);
        }

        let token = &auth_header[7..]; // Skip "Bearer "
        if token.is_empty() {
            return Err(JwtError::MissingToken);
        }

        Ok(token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{encode, EncodingKey, Header};

    fn create_test_token(secret: &str, user_id: &str, exp_offset: i64) -> String {
        let now = chrono::Utc::now().timestamp() as u64;
        let claims = Claims {
            sub: user_id.to_string(),
            iat: now,
            exp: (now as i64 + exp_offset) as u64,
            user_id: user_id.to_string(),
            roles: vec!["user".to_string()],
        };

        encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap()
    }

    #[test]
    fn test_validate_valid_token() {
        let secret = "test-secret-key";
        let auth = JwtAuth::new(secret.to_string(), Algorithm::HS256);

        let token = create_test_token(secret, "user-123", 3600);
        let claims = auth.validate_token(&token).unwrap();

        assert_eq!(claims.user_id, "user-123");
        assert_eq!(claims.sub, "user-123");
        assert_eq!(claims.roles, vec!["user"]);
    }

    #[test]
    fn test_validate_expired_token() {
        let secret = "test-secret-key";
        let auth = JwtAuth::new(secret.to_string(), Algorithm::HS256);

        // Create token that expired 100 seconds ago
        let token = create_test_token(secret, "user-123", -100);
        let result = auth.validate_token(&token);

        assert!(matches!(result, Err(JwtError::Expired)));
    }

    #[test]
    fn test_validate_invalid_signature() {
        let auth = JwtAuth::new("secret1".to_string(), Algorithm::HS256);

        // Token signed with different secret
        let token = create_test_token("secret2", "user-123", 3600);
        let result = auth.validate_token(&token);

        assert!(matches!(result, Err(JwtError::InvalidSignature(_))));
    }

    #[test]
    fn test_extract_token() {
        let header = "Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...";
        let token = JwtAuth::extract_token(header).unwrap();
        assert_eq!(token, "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...");
    }

    #[test]
    fn test_extract_token_invalid_format() {
        let result = JwtAuth::extract_token("Invalid token format");
        assert!(matches!(result, Err(JwtError::InvalidFormat)));
    }

    #[test]
    fn test_extract_token_missing() {
        let result = JwtAuth::extract_token("Bearer ");
        assert!(matches!(result, Err(JwtError::MissingToken)));
    }

    #[test]
    fn test_claims_user_id() {
        let claims = Claims {
            sub: "user-123".to_string(),
            iat: 1234567890,
            exp: 1234567890 + 3600,
            user_id: "user-123".to_string(),
            roles: vec![],
        };

        let user_id = claims.user_id();
        assert_eq!(user_id.as_ref(), "user-123");
    }
}
