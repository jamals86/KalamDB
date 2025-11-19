//! Primary key validation for MVCC INSERT operations
//!
//! This module implements validate_primary_key() and extract_user_pk_value(),
//! used to ensure PK uniqueness and extract PK values from fields JSON.

use crate::error::KalamDbError;

/// Extract primary key value from fields JSON
///
/// # Arguments
/// * `fields` - JSON object with all user-defined columns
/// * `pk_column` - Name of the primary key column
///
/// # Returns
/// * `Ok(String)` - String representation of PK value
/// * `Err(KalamDbError)` - If PK column missing or NULL
pub fn extract_user_pk_value(
    fields: &serde_json::Value,
    pk_column: &str,
) -> Result<String, KalamDbError> {
    let pk_value = fields.get(pk_column).ok_or_else(|| {
        KalamDbError::InvalidSql(format!(
            "Primary key column '{}' not found in fields",
            pk_column
        ))
    })?;

    if pk_value.is_null() {
        return Err(KalamDbError::InvalidSql(format!(
            "Primary key column '{}' cannot be NULL",
            pk_column
        )));
    }

    // Convert to string representation
    match pk_value {
        serde_json::Value::String(s) => Ok(s.clone()),
        serde_json::Value::Number(n) => Ok(n.to_string()),
        serde_json::Value::Bool(b) => Ok(b.to_string()),
        _ => Err(KalamDbError::InvalidSql(format!(
            "Primary key column '{}' has unsupported type (array/object)",
            pk_column
        ))),
    }
}


/// Validate primary key for INSERT operation
///
/// **MVCC Architecture**: This function checks:
/// 1. PK column exists in fields JSON
/// 2. PK value is not NULL
/// 3. PK value is unique (no existing non-deleted version with same PK)
///
/// # Arguments
/// * `fields` - JSON object with all user-defined columns
/// * `pk_column` - Name of the primary key column
/// * `existing_pk_values` - Set of existing PK values (non-deleted versions)
///
/// # Returns
/// * `Ok(())` - Validation passed
/// * `Err(KalamDbError)` - Validation failed
pub fn validate_primary_key(
    fields: &serde_json::Value,
    pk_column: &str,
    existing_pk_values: &std::collections::HashSet<String>,
) -> Result<(), KalamDbError> {
    // Extract PK value
    let pk_value = extract_user_pk_value(fields, pk_column)?;

    // Check uniqueness
    if existing_pk_values.contains(&pk_value) {
        return Err(KalamDbError::AlreadyExists(format!(
            "Primary key violation: value '{}' already exists",
            pk_value
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_extract_user_pk_value_string() {
        let fields = serde_json::json!({"id": "user123", "name": "Alice"});
        let pk = extract_user_pk_value(&fields, "id").unwrap();
        assert_eq!(pk, "user123");
    }

    #[test]
    fn test_extract_user_pk_value_number() {
        let fields = serde_json::json!({"id": 42, "name": "Bob"});
        let pk = extract_user_pk_value(&fields, "id").unwrap();
        assert_eq!(pk, "42");
    }

    #[test]
    fn test_extract_user_pk_value_missing() {
        let fields = serde_json::json!({"name": "Alice"});
        let result = extract_user_pk_value(&fields, "id");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_extract_user_pk_value_null() {
        let fields = serde_json::json!({"id": null, "name": "Alice"});
        let result = extract_user_pk_value(&fields, "id");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be NULL"));
    }

    #[test]
    fn test_validate_primary_key_success() {
        let fields = serde_json::json!({"id": "new_user", "name": "Alice"});
        let mut existing = HashSet::new();
        existing.insert("user1".to_string());
        existing.insert("user2".to_string());

        let result = validate_primary_key(&fields, "id", &existing);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_primary_key_duplicate() {
        let fields = serde_json::json!({"id": "user1", "name": "Alice"});
        let mut existing = HashSet::new();
        existing.insert("user1".to_string());
        existing.insert("user2".to_string());

        let result = validate_primary_key(&fields, "id", &existing);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[test]
    fn test_validate_primary_key_missing_column() {
        let fields = serde_json::json!({"name": "Alice"});
        let existing = HashSet::new();

        let result = validate_primary_key(&fields, "id", &existing);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }
}
