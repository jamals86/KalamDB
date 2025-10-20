//! Storage location path template engine
//!
//! This module handles template substitution for storage paths (e.g., ${user_id}).

use crate::catalog::UserId;
use crate::error::KalamDbError;
use std::collections::HashMap;

/// Path template engine for storage locations
pub struct PathTemplate;

impl PathTemplate {
    /// Substitute variables in a path template
    ///
    /// Supported variables:
    /// - `${user_id}`: User ID
    /// - `${namespace}`: Namespace name
    /// - `${table_name}`: Table name
    ///
    /// # Examples
    ///
    /// ```
    /// use kalamdb_core::storage::PathTemplate;
    /// use kalamdb_core::catalog::UserId;
    /// use std::collections::HashMap;
    ///
    /// let mut vars = HashMap::new();
    /// vars.insert("user_id".to_string(), "user1".to_string());
    /// vars.insert("table_name".to_string(), "messages".to_string());
    ///
    /// let result = PathTemplate::substitute("/data/${user_id}/${table_name}", &vars).unwrap();
    /// assert_eq!(result, "/data/user1/messages");
    /// ```
    pub fn substitute(
        template: &str,
        variables: &HashMap<String, String>,
    ) -> Result<String, KalamDbError> {
        let mut result = template.to_string();

        for (key, value) in variables {
            let placeholder = format!("${{{}}}", key);
            result = result.replace(&placeholder, value);
        }

        // Check for unresolved variables
        if result.contains("${") {
            return Err(KalamDbError::Other(format!(
                "Unresolved template variables in path: {}",
                result
            )));
        }

        Ok(result)
    }

    /// Substitute user_id in a path template
    pub fn substitute_user_id(template: &str, user_id: &UserId) -> Result<String, KalamDbError> {
        let mut vars = HashMap::new();
        vars.insert("user_id".to_string(), user_id.as_ref().to_string());
        Self::substitute(template, &vars)
    }

    /// Substitute multiple variables for table paths
    pub fn substitute_table_path(
        template: &str,
        user_id: Option<&UserId>,
        namespace: &str,
        table_name: &str,
    ) -> Result<String, KalamDbError> {
        let mut vars = HashMap::new();

        if let Some(uid) = user_id {
            vars.insert("user_id".to_string(), uid.as_ref().to_string());
        }

        vars.insert("namespace".to_string(), namespace.to_string());
        vars.insert("table_name".to_string(), table_name.to_string());

        Self::substitute(template, &vars)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_substitution() {
        let mut vars = HashMap::new();
        vars.insert("user_id".to_string(), "user123".to_string());
        vars.insert("table_name".to_string(), "messages".to_string());

        let result = PathTemplate::substitute("/data/${user_id}/${table_name}", &vars).unwrap();
        assert_eq!(result, "/data/user123/messages");
    }

    #[test]
    fn test_user_id_substitution() {
        let user_id = UserId::new("user456");
        let result =
            PathTemplate::substitute_user_id("/data/${user_id}/storage", &user_id).unwrap();
        assert_eq!(result, "/data/user456/storage");
    }

    #[test]
    fn test_table_path_substitution() {
        let user_id = UserId::new("user789");
        let result = PathTemplate::substitute_table_path(
            "/data/${user_id}/${namespace}/${table_name}",
            Some(&user_id),
            "app",
            "messages",
        )
        .unwrap();
        assert_eq!(result, "/data/user789/app/messages");
    }

    #[test]
    fn test_table_path_without_user_id() {
        let result = PathTemplate::substitute_table_path(
            "/data/${namespace}/${table_name}",
            None,
            "system",
            "users",
        )
        .unwrap();
        assert_eq!(result, "/data/system/users");
    }

    #[test]
    fn test_unresolved_variable_error() {
        let vars = HashMap::new();
        let result = PathTemplate::substitute("/data/${user_id}/storage", &vars);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unresolved"));
    }

    #[test]
    fn test_no_variables() {
        let vars = HashMap::new();
        let result = PathTemplate::substitute("/data/static/path", &vars).unwrap();
        assert_eq!(result, "/data/static/path");
    }
}
