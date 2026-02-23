// Pure JWT utilities for header/payload inspection without signature verification.
//
// These helpers are used as a routing step before full validation:
// read the algorithm and issuer claim first, then select the appropriate
// validator (internal HS256 vs external OIDC JWKS).

use crate::error::OidcError;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use jsonwebtoken::{decode_header, Algorithm};

/// Extract the `alg` field from the JWT header without verifying the signature.
///
/// Safe to use as a routing step because the algorithm merely selects which
/// validator runs; the chosen validator then verifies the signature itself.
pub fn extract_algorithm_unverified(token: &str) -> Result<Algorithm, OidcError> {
    let header = decode_header(token)
        .map_err(|e| OidcError::JwtValidationFailed(format!("Invalid JWT header: {}", e)))?;

    // Reject 'none' algorithm explicitly (if it exists in the enum, otherwise we just rely on the fact that it's not in the allowed list later)
    // jsonwebtoken crate doesn't have a `none` variant in `Algorithm` enum, so it will fail to decode if `alg=none`

    Ok(header.alg)
}

/// Extract the `iss` claim from the JWT payload **without verifying the signature**.
///
/// Safe to use as a routing step: the issuer determines which public key (or
/// shared secret) is used for verification.  The authenticity of `iss` is
/// confirmed once the signature check passes.
pub fn extract_issuer_unverified(token: &str) -> Result<String, OidcError> {
    let parts: Vec<&str> = token.splitn(3, '.').collect();
    if parts.len() < 2 {
        return Err(OidcError::JwtValidationFailed(
            "Invalid JWT format: less than 2 segments".into(),
        ));
    }

    let payload_bytes = URL_SAFE_NO_PAD.decode(parts[1]).map_err(|e| {
        OidcError::JwtValidationFailed(format!("Invalid JWT payload base64: {}", e))
    })?;

    let payload: serde_json::Value = serde_json::from_slice(&payload_bytes)
        .map_err(|e| OidcError::JwtValidationFailed(format!("Invalid JWT payload JSON: {}", e)))?;

    payload
        .get("iss")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| OidcError::JwtValidationFailed("Missing 'iss' claim".into()))
}

/// Extract all claims from a JWT without verifying the signature.
///
/// **WARNING**: This does NOT verify the signature or expiry!
/// Only use in tests or diagnostic tooling — never in a production auth path.
#[cfg(test)]
pub fn extract_claims_unverified<T>(token: &str) -> Result<T, OidcError>
where
    T: for<'de> serde::Deserialize<'de>,
{
    use jsonwebtoken::{decode, DecodingKey, Validation};

    let mut validation = Validation::new(Algorithm::HS256);
    #[allow(deprecated)]
    validation.insecure_disable_signature_validation();
    validation.validate_exp = false;

    let decoding_key = DecodingKey::from_secret(b"");
    let token_data = decode::<T>(token, &decoding_key, &validation)
        .map_err(|e| OidcError::JwtValidationFailed(format!("JWT decode error: {}", e)))?;

    Ok(token_data.claims)
}
