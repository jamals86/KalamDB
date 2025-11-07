//! Handler Helper Functions
//!
//! Common utilities shared across SQL execution handlers.
//! **Phase 2 Task T017**: Extract repeated code patterns into reusable functions.

use crate::error::KalamDbError;
use crate::sql::executor::handlers::types::ExecutionContext;
use kalamdb_commons::{NamespaceId, TableName};

/// Extract namespace from statement or use fallback
///
/// Priority:
/// 1. Namespace specified in SQL (e.g., `CREATE TABLE myns.tablename`)
/// 2. Namespace from ExecutionContext
/// 3. Default namespace ("default")
///
/// # Arguments
/// * `statement_namespace` - Optional namespace from SQL statement
/// * `context` - Execution context
///
/// # Returns
/// * `NamespaceId` - Resolved namespace
///
/// # Example
/// ```ignore
/// use kalamdb_core::sql::executor::handlers::helpers::resolve_namespace;
///
/// let ns = resolve_namespace(None, &exec_ctx);
/// ```
pub fn resolve_namespace(
    statement_namespace: Option<&NamespaceId>,
    context: &ExecutionContext,
) -> NamespaceId {
    statement_namespace
        .cloned()
        .or_else(|| context.namespace_id().cloned())
        .unwrap_or_else(|| NamespaceId::from("default"))
}

/// Extract namespace with error if missing
///
/// Similar to `resolve_namespace` but returns an error instead of defaulting.
///
/// # Arguments
/// * `statement_namespace` - Optional namespace from SQL statement
/// * `context` - Execution context
///
/// # Returns
/// * `Ok(NamespaceId)` - Resolved namespace
/// * `Err(KalamDbError::MissingNamespace)` - No namespace found
pub fn resolve_namespace_required(
    statement_namespace: Option<&NamespaceId>,
    context: &ExecutionContext,
) -> Result<NamespaceId, KalamDbError> {
    statement_namespace
        .cloned()
        .or_else(|| context.namespace_id().cloned())
        .ok_or_else(|| {
            KalamDbError::InvalidOperation(
                "Namespace is required but not specified in statement or context".to_string(),
            )
        })
}

/// Format table identifier for error messages
///
/// # Arguments
/// * `namespace` - Table namespace
/// * `table_name` - Table name
///
/// # Returns
/// * Formatted string: "namespace.table_name"
pub fn format_table_identifier(namespace: &NamespaceId, table_name: &TableName) -> String {
    format!("{}.{}", namespace.as_str(), table_name.as_str())
}

/// Format table identifier with optional namespace
pub fn format_table_identifier_opt(
    namespace: Option<&NamespaceId>,
    table_name: &TableName,
) -> String {
    match namespace {
        Some(ns) => format!("{}.{}", ns.as_str(), table_name.as_str()),
        None => table_name.as_str().to_string(),
    }
}

/// Validate table name format
///
/// Table names must:
/// - Not be empty
/// - Start with a letter or underscore
/// - Contain only letters, numbers, underscores
/// - Be <= 64 characters
///
/// # Arguments
/// * `table_name` - Table name to validate
///
/// # Returns
/// * `Ok(())` - Name is valid
/// * `Err(KalamDbError::InvalidOperation)` - Name is invalid
pub fn validate_table_name(table_name: &TableName) -> Result<(), KalamDbError> {
    let name = table_name.as_str();

    if name.is_empty() {
        return Err(KalamDbError::InvalidOperation(
            "Table name cannot be empty".to_string(),
        ));
    }

    if name.len() > 64 {
        return Err(KalamDbError::InvalidOperation(format!(
            "Table name '{}' exceeds maximum length of 64 characters",
            name
        )));
    }

    // First character must be letter or underscore
    let first_char = name.chars().next().unwrap();
    if !first_char.is_ascii_alphabetic() && first_char != '_' {
        return Err(KalamDbError::InvalidOperation(format!(
            "Table name '{}' must start with a letter or underscore",
            name
        )));
    }

    // All characters must be alphanumeric or underscore
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(KalamDbError::InvalidOperation(format!(
            "Table name '{}' contains invalid characters (only letters, numbers, and underscores allowed)",
            name
        )));
    }

    Ok(())
}

/// Validate namespace name format
///
/// Namespace names have the same rules as table names.
pub fn validate_namespace_name(namespace: &NamespaceId) -> Result<(), KalamDbError> {
    let name = namespace.as_str();

    if name.is_empty() {
        return Err(KalamDbError::InvalidOperation(
            "Namespace name cannot be empty".to_string(),
        ));
    }

    if name.len() > 64 {
        return Err(KalamDbError::InvalidOperation(format!(
            "Namespace name '{}' exceeds maximum length of 64 characters",
            name
        )));
    }

    let first_char = name.chars().next().unwrap();
    if !first_char.is_ascii_alphabetic() && first_char != '_' {
        return Err(KalamDbError::InvalidOperation(format!(
            "Namespace name '{}' must start with a letter or underscore",
            name
        )));
    }

    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(KalamDbError::InvalidOperation(format!(
            "Namespace name '{}' contains invalid characters (only letters, numbers, and underscores allowed)",
            name
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::Role;

    #[test]
    fn test_resolve_namespace_from_statement() {
        let ns = NamespaceId::from("myns");
        let ctx = ExecutionContext::new(
            kalamdb_commons::UserId::from("user1"),
            Role::User,
        );

        let resolved = resolve_namespace(Some(&ns), &ctx);
        assert_eq!(resolved.as_str(), "myns");
    }

    #[test]
    fn test_resolve_namespace_from_context() {
        let ns = NamespaceId::from("ctxns");
        let ctx = ExecutionContext::with_namespace(
            kalamdb_commons::UserId::from("user1"),
            Role::User,
            ns.clone(),
        );

        let resolved = resolve_namespace(None, &ctx);
        assert_eq!(resolved.as_str(), "ctxns");
    }

    #[test]
    fn test_resolve_namespace_default() {
        let ctx = ExecutionContext::new(
            kalamdb_commons::UserId::from("user1"),
            Role::User,
        );

        let resolved = resolve_namespace(None, &ctx);
        assert_eq!(resolved.as_str(), "default");
    }

    #[test]
    fn test_resolve_namespace_required_error() {
        let ctx = ExecutionContext::new(
            kalamdb_commons::UserId::from("user1"),
            Role::User,
        );

        let result = resolve_namespace_required(None, &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_format_table_identifier() {
        let ns = NamespaceId::from("myns");
        let table = TableName::new("mytable");

        let formatted = format_table_identifier(&ns, &table);
        assert_eq!(formatted, "myns.mytable");
    }

    #[test]
    fn test_validate_table_name_valid() {
        assert!(validate_table_name(&TableName::new("valid_table")).is_ok());
        assert!(validate_table_name(&TableName::new("Table123")).is_ok());
        assert!(validate_table_name(&TableName::new("_private")).is_ok());
    }

    #[test]
    fn test_validate_table_name_invalid() {
        // Empty
        assert!(validate_table_name(&TableName::new("")).is_err());

        // Too long
        let long_name = "a".repeat(65);
        assert!(validate_table_name(&TableName::new(&long_name)).is_err());

        // Invalid first character
        assert!(validate_table_name(&TableName::new("123table")).is_err());
        assert!(validate_table_name(&TableName::new("-table")).is_err());

        // Invalid characters
        assert!(validate_table_name(&TableName::new("table-name")).is_err());
        assert!(validate_table_name(&TableName::new("table.name")).is_err());
        assert!(validate_table_name(&TableName::new("table name")).is_err());
    }

    #[test]
    fn test_validate_namespace_name_valid() {
        assert!(validate_namespace_name(&NamespaceId::from("valid_ns")).is_ok());
        assert!(validate_namespace_name(&NamespaceId::from("Namespace123")).is_ok());
        assert!(validate_namespace_name(&NamespaceId::from("_system")).is_ok());
    }

    #[test]
    fn test_validate_namespace_name_invalid() {
        assert!(validate_namespace_name(&NamespaceId::from("")).is_err());
        assert!(validate_namespace_name(&NamespaceId::from("a".repeat(65))).is_err());
        assert!(validate_namespace_name(&NamespaceId::from("123ns")).is_err());
        assert!(validate_namespace_name(&NamespaceId::from("ns-name")).is_err());
    }
}
