// HTTP Basic Authentication parser

use crate::error::{AuthError, AuthResult};
use base64::prelude::*;

/// Parse HTTP Basic Auth header and extract credentials.
///
/// Expected format: `Authorization: Basic <base64-encoded-username:password>`
///
/// # Arguments
/// * `auth_header` - Value of the Authorization header
///
/// # Returns
/// Tuple of (username, password)
///
/// # Errors
/// - `AuthError::MalformedAuthorization` if header format is invalid
/// - `AuthError::MalformedAuthorization` if base64 decoding fails
/// - `AuthError::MalformedAuthorization` if credentials format is invalid
///
/// # Example
/// ```rust
/// use kalamdb_auth::basic_auth::parse_basic_auth_header;
///
/// let header = "Basic dXNlcjpwYXNz"; // base64("user:pass")
/// let (username, password) = parse_basic_auth_header(header).unwrap();
/// assert_eq!(username, "user");
/// assert_eq!(password, "pass");
/// ```
pub fn parse_basic_auth_header(auth_header: &str) -> AuthResult<(String, String)> {
    // Check if header starts with "Basic "
    let encoded = auth_header.strip_prefix("Basic ").ok_or_else(|| {
        AuthError::MalformedAuthorization(
            "Authorization header must start with 'Basic '".to_string(),
        )
    })?;

    // Decode base64
    let decoded_bytes = BASE64_STANDARD.decode(encoded.as_bytes()).map_err(|e| {
        AuthError::MalformedAuthorization(format!("Invalid base64 encoding: {}", e))
    })?;

    let decoded_str = String::from_utf8(decoded_bytes).map_err(|e| {
        AuthError::MalformedAuthorization(format!("Invalid UTF-8 in credentials: {}", e))
    })?;

    // Split into username:password
    extract_credentials(&decoded_str)
}

/// Extract username and password from decoded credentials string.
///
/// Expected format: `username:password`
///
/// # Arguments
/// * `credentials` - Decoded credentials string
///
/// # Returns
/// Tuple of (username, password)
///
/// # Errors
/// - `AuthError::MalformedAuthorization` if format is invalid (no colon found)
fn extract_credentials(credentials: &str) -> AuthResult<(String, String)> {
    let mut parts = credentials.splitn(2, ':');

    let username = parts.next().ok_or_else(|| {
        AuthError::MalformedAuthorization("Missing username in credentials".to_string())
    })?;

    let password = parts.next().ok_or_else(|| {
        AuthError::MalformedAuthorization(
            "Credentials must be in format 'username:password'".to_string(),
        )
    })?;

    Ok((username.to_string(), password.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_auth_valid() {
        // "user:pass" in base64 = "dXNlcjpwYXNz"
        let header = "Basic dXNlcjpwYXNz";
        let (username, password) = parse_basic_auth_header(header).unwrap();
        assert_eq!(username, "user");
        assert_eq!(password, "pass");
    }

    #[test]
    fn test_parse_basic_auth_with_colon_in_password() {
        // "admin:p@ss:word" in base64 = "YWRtaW46cEBzczp3b3Jk"
        let header = "Basic YWRtaW46cEBzczp3b3Jk";
        let (username, password) = parse_basic_auth_header(header).unwrap();
        assert_eq!(username, "admin");
        assert_eq!(password, "p@ss:word");
    }

    #[test]
    fn test_parse_basic_auth_missing_prefix() {
        let header = "dXNlcjpwYXNz"; // Missing "Basic "
        let result = parse_basic_auth_header(header);
        assert!(matches!(result, Err(AuthError::MalformedAuthorization(_))));
    }

    #[test]
    fn test_parse_basic_auth_invalid_base64() {
        let header = "Basic !!invalid!!";
        let result = parse_basic_auth_header(header);
        assert!(matches!(result, Err(AuthError::MalformedAuthorization(_))));
    }

    #[test]
    fn test_parse_basic_auth_no_colon() {
        // "userpass" (no colon) in base64 = "dXNlcnBhc3M="
        let header = "Basic dXNlcnBhc3M=";
        let result = parse_basic_auth_header(header);
        assert!(matches!(result, Err(AuthError::MalformedAuthorization(_))));
    }

    #[test]
    fn test_extract_credentials_valid() {
        let creds = "alice:SecurePassword123!";
        let (username, password) = extract_credentials(creds).unwrap();
        assert_eq!(username, "alice");
        assert_eq!(password, "SecurePassword123!");
    }

    #[test]
    fn test_extract_credentials_no_colon() {
        let creds = "alice";
        let result = extract_credentials(creds);
        assert!(matches!(result, Err(AuthError::MalformedAuthorization(_))));
    }
}
