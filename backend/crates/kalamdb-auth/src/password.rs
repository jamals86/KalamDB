// Password hashing and validation module

use crate::error::{AuthError, AuthResult};
use bcrypt::{hash, verify, DEFAULT_COST};
use std::collections::HashSet;
use std::sync::OnceLock;

/// Bcrypt cost factor for password hashing.
/// Higher values = more secure but slower.
/// Default: 12 (recommended for 2024)
pub const BCRYPT_COST: u32 = DEFAULT_COST;

/// Minimum password length
pub const MIN_PASSWORD_LENGTH: usize = 8;

/// Maximum password length (bcrypt has a 72-byte limit)
pub const MAX_PASSWORD_LENGTH: usize = 72;

/// Common passwords list (loaded once)
static COMMON_PASSWORDS: OnceLock<HashSet<String>> = OnceLock::new();

/// Hash a password using bcrypt.
///
/// Uses a configurable cost factor (default 12) and runs on a blocking thread pool
/// to avoid blocking the async runtime.
///
/// # Arguments
/// * `password` - Plain text password to hash
/// * `cost` - Optional bcrypt cost (defaults to BCRYPT_COST)
///
/// # Returns
/// Bcrypt hash string (includes salt)
///
/// # Errors
/// Returns `AuthError::HashingError` if bcrypt fails
pub async fn hash_password(password: &str, cost: Option<u32>) -> AuthResult<String> {
    let password = password.to_string();
    let cost = cost.unwrap_or(BCRYPT_COST);

    // Run bcrypt on blocking thread pool (CPU-intensive)
    tokio::task::spawn_blocking(move || {
        hash(password, cost).map_err(|e| AuthError::HashingError(e.to_string()))
    })
    .await
    .map_err(|e| AuthError::HashingError(format!("Task join error: {}", e)))?
}

/// Verify a password against a bcrypt hash.
///
/// Runs on a blocking thread pool to avoid blocking the async runtime.
///
/// # Arguments
/// * `password` - Plain text password to verify
/// * `hash` - Bcrypt hash to check against
///
/// # Returns
/// `Ok(true)` if password matches, `Ok(false)` if not, `Err` on failure
///
/// # Errors
/// Returns `AuthError::HashingError` if bcrypt verification fails
pub async fn verify_password(password: &str, hash: &str) -> AuthResult<bool> {
    let password = password.to_string();
    let hash = hash.to_string();

    // Run bcrypt on blocking thread pool (CPU-intensive)
    tokio::task::spawn_blocking(move || {
        verify(password, &hash).map_err(|e| AuthError::HashingError(e.to_string()))
    })
    .await
    .map_err(|e| AuthError::HashingError(format!("Task join error: {}", e)))?
}

/// Validate password meets security requirements.
///
/// Checks:
/// - Minimum length (8 characters)
/// - Maximum length (72 characters for bcrypt)
/// - Not in common passwords list
///
/// # Arguments
/// * `password` - Password to validate
///
/// # Returns
/// `Ok(())` if valid, `Err` with reason if invalid
///
/// # Errors
/// Returns `AuthError::WeakPassword` with specific reason
pub fn validate_password(password: &str) -> AuthResult<()> {
    // Check minimum length
    if password.len() < MIN_PASSWORD_LENGTH {
        return Err(AuthError::WeakPassword(format!(
            "Password must be at least {} characters",
            MIN_PASSWORD_LENGTH
        )));
    }

    // Check maximum length (bcrypt limit)
    if password.len() > MAX_PASSWORD_LENGTH {
        return Err(AuthError::WeakPassword(format!(
            "Password must be at most {} characters",
            MAX_PASSWORD_LENGTH
        )));
    }

    // Check against common passwords
    if is_common_password(password) {
        return Err(AuthError::WeakPassword(
            "Password is too common".to_string(),
        ));
    }

    Ok(())
}

/// Check if a password is in the common passwords list.
///
/// Loads the common passwords list on first call (lazy initialization).
///
/// # Arguments
/// * `password` - Password to check
///
/// # Returns
/// True if password is common, false otherwise
fn is_common_password(password: &str) -> bool {
    let common_passwords = COMMON_PASSWORDS.get_or_init(|| {
        // TODO: Load from file backend/crates/kalamdb-auth/data/common-passwords.txt
        // For now, use a small hardcoded list
        let passwords = vec![
            "password", "123456", "12345678", "qwerty", "abc123", "monkey", "1234567", "letmein",
            "trustno1", "dragon", "baseball", "iloveyou", "master", "sunshine", "ashley", "bailey",
            "passw0rd", "shadow", "123123", "654321", "superman", "qazwsx", "michael", "football",
        ];
        passwords.iter().map(|s| s.to_string()).collect()
    });

    common_passwords.contains(password)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_hash_and_verify_password() {
        let password = "SecurePassword123!";
        let hash = hash_password(password, Some(4))
            .await
            .expect("Failed to hash");
        assert!(hash.starts_with("$2b$")); // Bcrypt hash format

        let verified = verify_password(password, &hash)
            .await
            .expect("Failed to verify");
        assert!(verified);

        let wrong_verified = verify_password("WrongPassword", &hash)
            .await
            .expect("Failed to verify");
        assert!(!wrong_verified);
    }

    #[test]
    fn test_validate_password_too_short() {
        let result = validate_password("short");
        assert!(matches!(result, Err(AuthError::WeakPassword(_))));
    }

    #[test]
    fn test_validate_password_common() {
        let result = validate_password("password");
        assert!(matches!(result, Err(AuthError::WeakPassword(_))));
    }

    #[test]
    fn test_validate_password_valid() {
        let result = validate_password("MySecurePassword123!");
        assert!(result.is_ok());
    }

    #[test]
    fn test_is_common_password() {
        assert!(is_common_password("password"));
        assert!(is_common_password("123456"));
        assert!(!is_common_password("MyUniquePassword2024!"));
    }
}
