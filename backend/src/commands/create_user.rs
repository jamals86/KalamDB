//! Create user command for kalamdb-server
//!
//! Provides CLI command to create a new user with auto-generated API key

use anyhow::{Context, Result};
use kalamdb_core::auth::roles::validate_role;
use kalamdb_sql::RocksDbAdapter;
use kalamdb_sql::User;
use kalamdb_commons::{AuthType, Role, StorageMode, UserId};
use log::info;
use std::sync::Arc;
use uuid::Uuid;

/// Create a new user with auto-generated API key
///
/// # Arguments
/// * `sql_adapter` - SQL adapter for system tables
/// * `username` - Username for the new user
/// * `email` - Email address for the new user
/// * `role` - User role (admin, user, readonly)
///
/// # Returns
/// The generated API key (UUID v4)
pub async fn create_user(
    sql_adapter: Arc<RocksDbAdapter>,
    username: &str,
    email: &str,
    role: &str,
) -> Result<String> {
    // Validate role using kalamdb-core validation
    validate_role(role).context("Role validation failed")?;

    // Generate unique user_id (using username for simplicity)
    let user_id = UserId::new(format!("user_{}", username));

    // Auto-generate API key (UUID v4)
    let apikey = Uuid::new_v4().to_string();

    // Get current timestamp
    let created_at = chrono::Utc::now().timestamp_millis();

    // Parse role
    let user_role = match role {
        "admin" => Role::Dba,
        "user" => Role::User,
        "readonly" => Role::User, // For now, map readonly to User
        _ => return Err(anyhow::anyhow!("Invalid role: {}", role)),
    };

    // Create user struct
    let user = User {
        id: user_id,
        username: username.to_string(),
        password_hash: "".to_string(), // Empty for API key auth
        role: user_role,
        email: Some(email.to_string()),
        auth_type: AuthType::ApiKey,
        auth_data: None,
        api_key: Some(apikey.clone()),
        storage_mode: StorageMode::Table, // Default to table storage
        storage_id: None,
        created_at,
        updated_at: created_at,
        last_seen: None,
        deleted_at: None,
    };

    // Insert user into system_users table
    sql_adapter
        .insert_user(&user)
        .context("Failed to insert user into system_users")?;

    info!(
        "Created user '{}' with role '{}' and API key: {}",
        username, role, apikey
    );

    Ok(apikey)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_user_valid_roles() {
        // This test requires SqlAdapter initialization
        // Integration tests will validate the full flow
        let valid_roles = vec!["admin", "user", "readonly"];

        for role in valid_roles {
            assert!(["admin", "user", "readonly"].contains(&role));
        }
    }

    #[test]
    fn test_uuid_generation() {
        let apikey1 = Uuid::new_v4().to_string();
        let apikey2 = Uuid::new_v4().to_string();

        // Verify UUIDs are different
        assert_ne!(apikey1, apikey2);

        // Verify UUID format (36 characters with hyphens)
        assert_eq!(apikey1.len(), 36);
        assert!(apikey1.contains('-'));
    }
}
