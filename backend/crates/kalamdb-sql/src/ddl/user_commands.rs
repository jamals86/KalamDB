//! User management SQL commands
//!
//! This module provides SQL command parsing for user management:
//! - CREATE USER: Create a new user with authentication
//! - ALTER USER: Modify user properties (password, role, email)
//! - DROP USER: Soft delete a user account

use kalamdb_commons::Role;
use kalamdb_commons::AuthType;
use serde::{Deserialize, Serialize};

/// CREATE USER command
///
/// Syntax:
/// ```sql
/// CREATE USER 'username'
///   WITH PASSWORD 'password'
///   [ROLE role_name]
///   [EMAIL 'email@example.com'];
///
/// CREATE USER 'username'
///   WITH OAUTH
///   ROLE role_name
///   [EMAIL 'email@example.com'];
///
/// CREATE USER 'username'
///   WITH INTERNAL
///   ROLE role_name;
/// ```
///
/// Examples:
/// ```sql
/// CREATE USER 'alice' WITH PASSWORD 'secure123' ROLE developer EMAIL 'alice@example.com';
/// CREATE USER 'service_account' WITH INTERNAL ROLE system;
/// CREATE USER 'oauth_user' WITH OAUTH ROLE viewer EMAIL 'user@example.com';
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreateUserStatement {
    /// Username (unique identifier)
    pub username: String,

    /// Authentication type
    pub auth_type: AuthType,

    /// User role (dba, admin, developer, analyst, viewer)
    pub role: Role,

    /// Optional email address
    pub email: Option<String>,

    /// Password (only for Password auth type)
    pub password: Option<String>,
}

impl CreateUserStatement {
    /// Parse CREATE USER from SQL
    pub fn parse(sql: &str) -> Result<Self, String> {
        use crate::parser::utils::{
            extract_keyword_value, extract_quoted_keyword_value, normalize_sql,
        };

        let normalized = normalize_sql(sql);
        let sql_upper = normalized.to_uppercase();

        if !sql_upper.starts_with("CREATE USER") {
            return Err("SQL must start with CREATE USER".to_string());
        }

        // Extract username (quoted string after CREATE USER)
        let username = extract_quoted_keyword_value(&normalized, "USER")?;

        // Determine auth type
        let auth_type = if normalized.to_uppercase().contains("WITH PASSWORD") {
            AuthType::Password
        } else if normalized.to_uppercase().contains("WITH OAUTH") {
            AuthType::OAuth
        } else if normalized.to_uppercase().contains("WITH INTERNAL") {
            AuthType::Internal
        } else {
            return Err(
                "Must specify authentication type: WITH PASSWORD, WITH OAUTH, or WITH INTERNAL"
                    .to_string(),
            );
        };

        // Extract password if auth type is Password
        let password = if auth_type == AuthType::Password {
            Some(extract_quoted_keyword_value(&normalized, "PASSWORD")?)
        } else {
            None
        };

        // Extract role (required)
        let role_str = extract_keyword_value(&normalized, "ROLE")
            .or_else(|_| extract_quoted_keyword_value(&normalized, "ROLE"))?;

        // Map SQL role names to Role enum

        let role =  Role::from_str(&role_str)
            .map_err(|_| {
                format!(
                    "Invalid role '{}'. Must be one of: dba, user, service, system",
                    role_str
                )
            })?;

        // Extract email (optional)
        let email = extract_quoted_keyword_value(&normalized, "EMAIL").ok();

        Ok(CreateUserStatement {
            username,
            auth_type,
            role,
            email,
            password,
        })
    }
}

/// ALTER USER command
///
/// Syntax:
/// ```sql
/// ALTER USER 'username' SET PASSWORD 'new_password';
/// ALTER USER 'username' SET ROLE new_role;
/// ALTER USER 'username' SET EMAIL 'new_email@example.com';
/// ```
///
/// Examples:
/// ```sql
/// ALTER USER 'alice' SET PASSWORD 'newsecure456';
/// ALTER USER 'alice' SET ROLE admin;
/// ALTER USER 'alice' SET EMAIL 'alice.new@example.com';
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AlterUserStatement {
    /// Username to modify
    pub username: String,

    /// What to modify
    pub modification: UserModification,
}

/// Type of user modification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum UserModification {
    /// Set new password
    SetPassword(String),
    /// Set new role
    SetRole(Role),
    /// Set new email
    SetEmail(String),
}

impl AlterUserStatement {
    /// Parse ALTER USER from SQL
    pub fn parse(sql: &str) -> Result<Self, String> {
        use crate::parser::utils::{extract_quoted_keyword_value, normalize_sql};

        let normalized = normalize_sql(sql);
        let sql_upper = normalized.to_uppercase();

        if !sql_upper.starts_with("ALTER USER") {
            return Err("SQL must start with ALTER USER".to_string());
        }

        // Extract username
        let username = extract_quoted_keyword_value(&normalized, "USER")?;

        // Determine modification type
        let modification = if sql_upper.contains("SET PASSWORD") {
            let password = extract_quoted_keyword_value(&normalized, "PASSWORD")?;
            UserModification::SetPassword(password)
        } else if sql_upper.contains("SET ROLE") {
            // Try unquoted first (ROLE admin), then quoted (ROLE 'admin')
            let role_str = extract_quoted_keyword_value(&normalized, "ROLE")
                .or_else(|_| -> Result<String, String> {
                    // Extract unquoted role value manually
                    let set_role_idx = sql_upper.find("SET ROLE").ok_or("ROLE not found")?;
                    let after_role = &normalized[set_role_idx + 8..].trim();
                    let role_value = after_role
                        .split_whitespace()
                        .next()
                        .ok_or("Role value not found")?
                        .trim_end_matches(';');
                    Ok(role_value.to_string())
                })?;

            // Map SQL role names to Role enum
            let role = match role_str.to_lowercase().as_str() {
                "dba" | "admin" => Role::Dba,
                "developer" | "analyst" | "service" => Role::Service,
                "viewer" | "readonly" | "user" => Role::User,
                "system" => Role::System,
                _ => {
                    return Err(format!(
                        "Invalid role '{}'. Must be one of: dba, admin, developer, analyst, viewer, user, service, system",
                        role_str
                    ))
                }
            };
            UserModification::SetRole(role)
        } else if sql_upper.contains("SET EMAIL") {
            let email = extract_quoted_keyword_value(&normalized, "EMAIL")?;
            UserModification::SetEmail(email)
        } else {
            return Err(
                "Must specify SET PASSWORD, SET ROLE, or SET EMAIL modification".to_string()
            );
        };

        Ok(AlterUserStatement {
            username,
            modification,
        })
    }
}

/// DROP USER command
///
/// Syntax:
/// ```sql
/// DROP USER 'username';
/// ```
///
/// Example:
/// ```sql
/// DROP USER 'alice';
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DropUserStatement {
    /// Username to delete
    pub username: String,
}

impl DropUserStatement {
    /// Parse DROP USER from SQL
    pub fn parse(sql: &str) -> Result<Self, String> {
        use crate::parser::utils::{extract_quoted_keyword_value, normalize_sql};

        let normalized = normalize_sql(sql);
        let sql_upper = normalized.to_uppercase();

        if !sql_upper.starts_with("DROP USER") {
            return Err("SQL must start with DROP USER".to_string());
        }

        // Extract username
        let username = extract_quoted_keyword_value(&normalized, "USER")?;

        Ok(DropUserStatement { username })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_create_user_with_password() {
        let sql = "CREATE USER 'alice' WITH PASSWORD 'secure123' ROLE developer EMAIL 'alice@example.com'";
        let result = CreateUserStatement::parse(sql);
        assert!(result.is_ok());
        let stmt = result.unwrap();
        assert_eq!(stmt.username, "alice");
        assert_eq!(stmt.auth_type, AuthType::Password);
        assert_eq!(stmt.password, Some("secure123".to_string()));
        assert_eq!(stmt.role, Role::Service); // developer maps to Service
        assert_eq!(stmt.email, Some("alice@example.com".to_string()));
    }

    #[test]
    fn test_parse_create_user_with_oauth() {
        let sql = "CREATE USER 'oauth_user' WITH OAUTH ROLE viewer EMAIL 'user@example.com'";
        let result = CreateUserStatement::parse(sql);
        assert!(result.is_ok());
        let stmt = result.unwrap();
        assert_eq!(stmt.username, "oauth_user");
        assert_eq!(stmt.auth_type, AuthType::OAuth);
        assert_eq!(stmt.password, None);
        assert_eq!(stmt.role, Role::User); // viewer maps to User
    }

    #[test]
    fn test_parse_create_user_with_internal() {
        let sql = "CREATE USER 'service_account' WITH INTERNAL ROLE system";
        let result = CreateUserStatement::parse(sql);
        assert!(result.is_ok());
        let stmt = result.unwrap();
        assert_eq!(stmt.username, "service_account");
        assert_eq!(stmt.auth_type, AuthType::Internal);
        assert_eq!(stmt.role, Role::System);
    }

    #[test]
    fn test_parse_alter_user_set_password() {
        let sql = "ALTER USER 'alice' SET PASSWORD 'newsecure456'";
        let result = AlterUserStatement::parse(sql);
        assert!(result.is_ok());
        let stmt = result.unwrap();
        assert_eq!(stmt.username, "alice");
        assert!(matches!(
            stmt.modification,
            UserModification::SetPassword(_)
        ));
    }

    #[test]
    fn test_parse_alter_user_set_role() {
        let sql = "ALTER USER 'alice' SET ROLE admin";
        let result = AlterUserStatement::parse(sql);
        assert!(result.is_ok());
        let stmt = result.unwrap();
        assert_eq!(stmt.username, "alice");
        if let UserModification::SetRole(role) = stmt.modification {
            assert_eq!(role, "admin");
        } else {
            panic!("Expected SetRole modification");
        }
    }

    #[test]
    fn test_parse_drop_user() {
        let sql = "DROP USER 'alice'";
        let result = DropUserStatement::parse(sql);
        assert!(result.is_ok());
        let stmt = result.unwrap();
        assert_eq!(stmt.username, "alice");
    }

    #[test]
    fn test_invalid_role() {
        let sql = "CREATE USER 'alice' WITH PASSWORD 'pass123' ROLE invalid_role";
        let result = CreateUserStatement::parse(sql);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid role"));
    }

    #[test]
    fn test_missing_auth_type() {
        let sql = "CREATE USER 'alice' ROLE developer";
        let result = CreateUserStatement::parse(sql);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Must specify authentication type"));
    }
}
