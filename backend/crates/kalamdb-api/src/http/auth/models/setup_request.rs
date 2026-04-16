//! Server setup request model

use super::login_request::{validate_password_length, validate_user_length};
use serde::Deserialize;

/// Server setup request body
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerSetupRequest {
    /// Canonical user identifier for the new DBA account
    #[serde(deserialize_with = "validate_user_length")]
    pub user: String,
    /// Password for the new DBA user
    #[serde(deserialize_with = "validate_password_length")]
    pub password: String,
    /// Password for the root user
    #[serde(deserialize_with = "validate_password_length")]
    pub root_password: String,
    /// Email for the new DBA user (optional)
    pub email: Option<String>,
}
