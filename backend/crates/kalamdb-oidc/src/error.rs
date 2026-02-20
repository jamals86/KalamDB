/// Errors produced by the OIDC validator.
#[derive(Debug, thiserror::Error)]
pub enum OidcError {
    /// OIDC Discovery endpoint failed or returned unexpected data.
    #[error("OIDC discovery failed: {0}")]
    DiscoveryFailed(String),

    /// JWKS fetch or parse failed.
    #[error("JWKS fetch failed: {0}")]
    JwksFetchFailed(String),

    /// Token is missing the `kid` header required for key lookup.
    #[error("Token is missing the 'kid' header")]
    MissingKid,

    /// No key with the given `kid` was found in the issuer's JWKS.
    #[error("No key found for kid '{0}'")]
    KeyNotFound(String),

    /// The JWK could not be converted to a decoding key.
    #[error("Invalid JWK format: {0}")]
    InvalidKeyFormat(String),

    /// JWT decode / signature verification / claims validation failed.
    #[error("JWT validation failed: {0}")]
    JwtValidationFailed(String),
}

impl From<jsonwebtoken::errors::Error> for OidcError {
    fn from(e: jsonwebtoken::errors::Error) -> Self {
        use jsonwebtoken::errors::ErrorKind;
        match e.kind() {
            ErrorKind::ExpiredSignature => {
                OidcError::JwtValidationFailed("Token expired".into())
            }
            ErrorKind::InvalidSignature => {
                OidcError::JwtValidationFailed("Invalid signature".into())
            }
            ErrorKind::InvalidToken => {
                OidcError::JwtValidationFailed("Invalid token".into())
            }
            _ => OidcError::JwtValidationFailed(e.to_string()),
        }
    }
}
