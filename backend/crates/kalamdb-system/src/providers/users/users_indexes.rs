//! Users table index definitions
//!
//! This module defines secondary indexes for the system.users table.

use crate::StoragePartition;
use kalamdb_commons::storage::Partition;
use crate::providers::users::models::User;
use kalamdb_commons::UserId;
use kalamdb_store::IndexDefinition;
use std::sync::Arc;

/// Index for querying users by username (unique).
///
/// Key format: `{username_lowercase}`
///
/// This index allows efficient lookups by username and enforces uniqueness.
/// The username is stored in lowercase for case-insensitive lookups.
pub struct UserUsernameIndex;

impl IndexDefinition<UserId, User> for UserUsernameIndex {
    fn partition(&self) -> Partition {
        Partition::new(StoragePartition::SystemUsersUsernameIdx.name())
    }

    fn indexed_columns(&self) -> Vec<&str> {
        vec!["username"]
    }

    fn extract_key(&self, _primary_key: &UserId, user: &User) -> Option<Vec<u8>> {
        // Store username in lowercase for case-insensitive lookups
        let username_lower = user.username.as_str().to_lowercase();
        Some(username_lower.into_bytes())
    }

    fn filter_to_prefix(&self, filter: &datafusion::logical_expr::Expr) -> Option<Vec<u8>> {
        use datafusion::logical_expr::Expr;
        use datafusion::scalar::ScalarValue;
        use kalamdb_store::extract_string_equality;

        // Handle equality: username = 'value'
        if let Some((col, val)) = extract_string_equality(filter) {
            if col == "username" {
                // Convert to lowercase for case-insensitive matching
                return Some(val.to_lowercase().into_bytes());
            }
        }

        // Handle LIKE operator: username LIKE 'prefix%'
        if let Expr::Like(like_expr) = filter {
            if let Expr::Column(col) = like_expr.expr.as_ref() {
                if col.name == "username" {
                    if let Expr::Literal(ScalarValue::Utf8(Some(pattern)), _) =
                        like_expr.pattern.as_ref()
                    {
                        // Check if pattern is a simple prefix match (ends with %)
                        if pattern.ends_with('%')
                            && !pattern[..pattern.len() - 1].contains('%')
                            && !pattern[..pattern.len() - 1].contains('_')
                        {
                            let prefix = &pattern[..pattern.len() - 1];
                            return Some(prefix.to_lowercase().into_bytes());
                        }
                    }
                }
            }
        }

        None
    }
}

/// Index for querying users by role.
///
/// Key format: `{role}:{user_id}`
///
/// This index allows efficient queries like:
/// - "All users with role 'admin'"
/// - "All service accounts"
pub struct UserRoleIndex;

impl IndexDefinition<UserId, User> for UserRoleIndex {
    fn partition(&self) -> Partition {
        Partition::new(StoragePartition::SystemUsersRoleIdx.name())
    }

    fn indexed_columns(&self) -> Vec<&str> {
        vec!["role"]
    }

    fn extract_key(&self, _primary_key: &UserId, user: &User) -> Option<Vec<u8>> {
        let key = format!("{}:{}", user.role.as_str(), user.user_id.as_str());
        Some(key.into_bytes())
    }

    fn filter_to_prefix(&self, filter: &datafusion::logical_expr::Expr) -> Option<Vec<u8>> {
        use kalamdb_store::extract_string_equality;

        if let Some((col, val)) = extract_string_equality(filter) {
            if col == "role" {
                return Some(format!("{}:", val).into_bytes());
            }
        }
        None
    }
}

/// Create the default set of indexes for the users table.
pub fn create_users_indexes() -> Vec<Arc<dyn IndexDefinition<UserId, User>>> {
    vec![Arc::new(UserUsernameIndex), Arc::new(UserRoleIndex)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::models::UserName;
    use kalamdb_commons::{AuthType, Role, StorageId, StorageMode};

    fn create_test_user(id: &str, username: &str, role: Role) -> User {
        User {
            user_id: UserId::new(id),
            username: UserName::new(username),
            password_hash: "hashed_password".to_string(),
            role,
            email: Some(format!("{}@example.com", username)),
            auth_type: AuthType::Password,
            auth_data: None,
            storage_mode: StorageMode::Table,
            storage_id: Some(StorageId::local()),
            failed_login_attempts: 0,
            locked_until: None,
            last_login_at: None,
            created_at: 1000,
            updated_at: 1000,
            last_seen: None,
            deleted_at: None,
        }
    }

    #[test]
    fn test_username_index_key_format() {
        let user = create_test_user("user1", "Alice", Role::User);
        let user_id = user.user_id.clone();

        let index = UserUsernameIndex;
        let key = index.extract_key(&user_id, &user).unwrap();

        // Should be lowercase
        let key_str = String::from_utf8(key).unwrap();
        assert_eq!(key_str, "alice");
    }

    #[test]
    fn test_role_index_key_format() {
        let user = create_test_user("user1", "alice", Role::Dba);
        let user_id = user.user_id.clone();

        let index = UserRoleIndex;
        let key = index.extract_key(&user_id, &user).unwrap();

        let key_str = String::from_utf8(key).unwrap();
        assert_eq!(key_str, "dba:user1");
    }

    #[test]
    fn test_create_users_indexes() {
        let indexes = create_users_indexes();
        assert_eq!(indexes.len(), 2);
        assert_eq!(
            indexes[0].partition(),
            StoragePartition::SystemUsersUsernameIdx.name().into()
        );
        assert_eq!(
            indexes[1].partition(),
            StoragePartition::SystemUsersRoleIdx.name().into()
        );
    }
}
