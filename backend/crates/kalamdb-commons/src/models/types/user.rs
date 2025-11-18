//! User entity for system.users table.
//!
//! Represents a database user with authentication and authorization information.

use crate::models::user_name::UserName;
use crate::models::{ids::UserId, AuthType, Role, StorageId, StorageMode};
use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

/// User entity for system.users table.
///
/// Represents a database user with authentication and authorization information.
///
/// ## Fields
/// - `id`: Unique user identifier (e.g., "u_123456")
/// - `username`: Unique username for authentication
/// - `password_hash`: bcrypt hash of password (cost factor 12)
/// - `role`: User role (User, Service, DBA, System)
/// - `email`: Optional email address
/// - `auth_type`: Authentication method (Password, OAuth, Internal)
/// - `auth_data`: JSON blob for auth-specific data (e.g., OAuth provider/subject)
/// - `storage_mode`: Preferred storage partitioning mode (Table, Region)
/// - `storage_id`: Optional preferred storage configuration ID
/// - `created_at`: Unix timestamp in milliseconds when user was created
/// - `updated_at`: Unix timestamp in milliseconds when user was last modified
/// - `last_seen`: Optional Unix timestamp in milliseconds of last activity
/// - `deleted_at`: Optional Unix timestamp in milliseconds for soft delete
///
/// ## Serialization
/// - **RocksDB**: Bincode (compact binary format)
/// - **API**: JSON via Serde
///
/// ## Example
///
/// ```rust
/// use kalamdb_commons::types::User;
/// use kalamdb_commons::{UserId, Role, AuthType, StorageMode, StorageId, UserName};
///
/// let user = User {
///     id: UserId::new("u_123456"),
///     username: UserName::new("alice"),
///     password_hash: "$2b$12$...".to_string(),
///     role: Role::User,
///     email: Some("alice@example.com".to_string()),
///     auth_type: AuthType::Password,
///     auth_data: None,
///     storage_mode: StorageMode::Table,
///     storage_id: Some(StorageId::new("storage_1")),
///     created_at: 1730000000000,
///     updated_at: 1730000000000,
///     last_seen: None,
///     deleted_at: None,
/// };
/// ```
#[derive(Serialize, Deserialize, Encode, Decode, Clone, Debug, PartialEq)]
pub struct User {
    pub id: UserId,
    pub username: UserName,
    pub password_hash: String,
    pub role: Role,
    pub email: Option<String>,
    pub auth_type: AuthType,
    pub auth_data: Option<String>, // JSON blob for OAuth provider/subject
    pub storage_mode: StorageMode, // Preferred storage partitioning mode
    pub storage_id: Option<StorageId>, // Optional preferred storage configuration
    pub created_at: i64,           // Unix timestamp in milliseconds
    pub updated_at: i64,           // Unix timestamp in milliseconds
    pub last_seen: Option<i64>,    // Unix timestamp in milliseconds (daily granularity)
    pub deleted_at: Option<i64>,   // Unix timestamp in milliseconds for soft delete
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_serialization() {
        let user = User {
            id: UserId::new("u_123"),
            username: "alice".into(),
            password_hash: "$2b$12$hash".to_string(),
            role: Role::User,
            email: Some("test@example.com".to_string()),
            auth_type: AuthType::Password,
            auth_data: None,
            storage_mode: StorageMode::Table,
            storage_id: Some(StorageId::new("storage_1")),
            created_at: 1730000000000,
            updated_at: 1730000000000,
            last_seen: None,
            deleted_at: None,
        };

        // Test bincode serialization
        let config = bincode::config::standard();
        let bytes = bincode::encode_to_vec(&user, config).unwrap();
        let (deserialized, _): (User, _) = bincode::decode_from_slice(&bytes, config).unwrap();
        assert_eq!(user, deserialized);
    }
}
