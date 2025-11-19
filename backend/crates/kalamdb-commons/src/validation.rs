//! Naming validation for namespaces, tables, and columns
//!
//! This module provides validation functions to ensure that user-provided names
//! comply with KalamDB naming conventions and don't conflict with reserved words.
//!
//! SQL keywords are sourced from sqlparser-rs reserved keyword classifications
//! (table aliases, column aliases, identifier restrictions, table factors), which
//! cover the dialect-specific words that cannot safely be used as identifiers.

use once_cell::sync::Lazy;
use sqlparser::keywords::{
    Keyword, ALL_KEYWORDS, ALL_KEYWORDS_INDEX, RESERVED_FOR_COLUMN_ALIAS,
    RESERVED_FOR_IDENTIFIER, RESERVED_FOR_TABLE_ALIAS, RESERVED_FOR_TABLE_FACTOR,
};
use std::collections::HashSet;

/// Reserved namespace names that cannot be used by users
pub static RESERVED_NAMESPACES: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut set = HashSet::new();
    set.insert("system");
    set.insert("sys");
    set.insert("root");
    set.insert("kalamdb");
    set.insert("kalam");
    set.insert("main");
    set.insert("default");
    set.insert("sql");
    set.insert("admin");
    set.insert("internal");
    set.insert("information_schema");
    set
});

/// Reserved column names that cannot be used by users
/// These are system columns that are automatically managed
pub static RESERVED_COLUMN_NAMES: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut set = HashSet::new();
    // Current system columns
    set.insert("_seq");
    set.insert("_deleted");
    set
});

/// Additional SQL keywords that must always be rejected as identifiers
const CRITICAL_RESERVED_KEYWORDS: &[&str] = &[
    "TABLE",
    "TABLES",
    "TABLESPACE",
    "INDEX",
    "VIEW",
    "SCHEMA",
    "DATABASE",
    "INSERT",
    "UPDATE",
    "DELETE",
    "CREATE",
    "DROP",
    "ALTER",
];

/// SQL keywords that cannot safely be used as identifiers in KalamDB
pub static RESERVED_SQL_KEYWORDS: Lazy<HashSet<String>> = Lazy::new(|| {
    let mut keywords: HashSet<String> = RESERVED_FOR_TABLE_ALIAS
        .iter()
        .chain(RESERVED_FOR_COLUMN_ALIAS.iter())
        .chain(RESERVED_FOR_TABLE_FACTOR.iter())
        .chain(RESERVED_FOR_IDENTIFIER.iter())
        .map(keyword_to_str)
        .map(|keyword| keyword.to_ascii_uppercase())
        .collect();

    for keyword in CRITICAL_RESERVED_KEYWORDS {
        keywords.insert(keyword.to_ascii_uppercase());
    }

    keywords
});

/// Validation error types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// Name is empty
    Empty,
    /// Name is too long (max 64 characters)
    TooLong(usize),
    /// Name starts with an underscore (reserved for system columns)
    StartsWithUnderscore,
    /// Name contains invalid characters (only alphanumeric and underscore allowed)
    InvalidCharacters(String),
    /// Name is a reserved namespace
    ReservedNamespace(String),
    /// Name is a reserved column name
    ReservedColumnName(String),
    /// Name is a reserved SQL keyword
    ReservedSqlKeyword(String),
    /// Name starts with a number
    StartsWithNumber,
    /// Name contains spaces
    ContainsSpaces,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::Empty => write!(f, "Name cannot be empty"),
            ValidationError::TooLong(len) => {
                write!(f, "Name is too long ({} characters, max 64)", len)
            }
            ValidationError::StartsWithUnderscore => write!(
                f,
                "Name cannot start with underscore (reserved for system columns)"
            ),
            ValidationError::InvalidCharacters(name) => write!(
                f,
                "Name '{}' contains invalid characters (only alphanumeric and underscore allowed)",
                name
            ),
            ValidationError::ReservedNamespace(name) => {
                write!(f, "Namespace '{}' is reserved and cannot be used", name)
            }
            ValidationError::ReservedColumnName(name) => {
                write!(f, "Column name '{}' is reserved and cannot be used", name)
            }
            ValidationError::ReservedSqlKeyword(name) => write!(
                f,
                "Name '{}' is a reserved SQL keyword and cannot be used",
                name
            ),
            ValidationError::StartsWithNumber => write!(f, "Name cannot start with a number"),
            ValidationError::ContainsSpaces => write!(f, "Name cannot contain spaces"),
        }
    }
}

impl std::error::Error for ValidationError {}

/// Maximum length for namespace, table, and column names
pub const MAX_NAME_LENGTH: usize = 64;

/// Validate a namespace name
///
/// Rules:
/// - Not empty
/// - Max 64 characters
/// - Only alphanumeric and underscore
/// - Cannot start with underscore or number
/// - Cannot contain spaces
/// - Cannot be a reserved namespace
/// - Cannot be a reserved SQL keyword
pub fn validate_namespace_name(name: &str) -> Result<(), ValidationError> {
    // Check if it's a reserved namespace first (more specific error message)
    let lowercase = name.to_lowercase();
    if RESERVED_NAMESPACES.contains(lowercase.as_str()) {
        return Err(ValidationError::ReservedNamespace(name.to_string()));
    }

    validate_identifier_base(name)?;
    ensure_not_reserved_sql_keyword(name)?;

    Ok(())
}

/// Validate a table name
///
/// Rules:
/// - Not empty
/// - Max 64 characters
/// - Only alphanumeric and underscore
/// - Cannot start with underscore or number
/// - Cannot contain spaces
/// - Cannot be a reserved SQL keyword
pub fn validate_table_name(name: &str) -> Result<(), ValidationError> {
    validate_identifier_base(name)?;
    ensure_not_reserved_sql_keyword(name)?;
    Ok(())
}

/// Validate a column name
///
/// Rules:
/// - Not empty
/// - Max 64 characters
/// - Only alphanumeric and underscore
/// - Cannot start with underscore (reserved for system columns)
/// - Cannot start with number
/// - Cannot contain spaces
/// - Cannot be a reserved column name
/// - Cannot be a reserved SQL keyword
pub fn validate_column_name(name: &str) -> Result<(), ValidationError> {
    validate_identifier_base(name)?;
    ensure_not_reserved_sql_keyword(name)?;
    Ok(())
}

/// Base validation rules for all identifiers
fn validate_identifier_base(name: &str) -> Result<(), ValidationError> {
    // Check if empty
    if name.is_empty() {
        return Err(ValidationError::Empty);
    }

    // Check length
    if name.len() > MAX_NAME_LENGTH {
        return Err(ValidationError::TooLong(name.len()));
    }

    // Check for spaces
    if name.contains(' ') {
        return Err(ValidationError::ContainsSpaces);
    }

    // Check if starts with underscore
    if name.starts_with('_') {
        return Err(ValidationError::StartsWithUnderscore);
    }

    // Check if starts with number
    if name.chars().next().unwrap().is_ascii_digit() {
        return Err(ValidationError::StartsWithNumber);
    }

    // Check for invalid characters (only alphanumeric and underscore allowed)
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(ValidationError::InvalidCharacters(name.to_string()));
    }

    Ok(())
}

fn ensure_not_reserved_sql_keyword(name: &str) -> Result<(), ValidationError> {
    let uppercase = name.to_ascii_uppercase();
    if RESERVED_SQL_KEYWORDS.contains(uppercase.as_str()) {
        return Err(ValidationError::ReservedSqlKeyword(name.to_string()));
    }
    Ok(())
}

fn keyword_to_str(keyword: &Keyword) -> &'static str {
    let index = ALL_KEYWORDS_INDEX
        .iter()
        .position(|entry| entry == keyword)
        .expect("keyword missing from sqlparser ALL_KEYWORDS_INDEX");
    ALL_KEYWORDS[index]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_namespace_names() {
        assert!(validate_namespace_name("app").is_ok());
        assert!(validate_namespace_name("my_namespace").is_ok());
        assert!(validate_namespace_name("namespace123").is_ok());
        assert!(validate_namespace_name("MyNamespace").is_ok());
    }

    #[test]
    fn test_reserved_namespace_names() {
        assert_eq!(
            validate_namespace_name("system"),
            Err(ValidationError::ReservedNamespace("system".to_string()))
        );
        assert_eq!(
            validate_namespace_name("sys"),
            Err(ValidationError::ReservedNamespace("sys".to_string()))
        );
        assert_eq!(
            validate_namespace_name("root"),
            Err(ValidationError::ReservedNamespace("root".to_string()))
        );
        assert_eq!(
            validate_namespace_name("kalamdb"),
            Err(ValidationError::ReservedNamespace("kalamdb".to_string()))
        );
        assert_eq!(
            validate_namespace_name("SYSTEM"),
            Err(ValidationError::ReservedNamespace("SYSTEM".to_string()))
        );
    }

    #[test]
    fn test_valid_table_names() {
        assert!(validate_table_name("users").is_ok());
        assert!(validate_table_name("user_messages").is_ok());
        assert!(validate_table_name("table123").is_ok());
    }

    #[test]
    fn test_valid_column_names() {
        assert!(validate_column_name("user_id").is_ok());
        assert!(validate_column_name("firstName").is_ok());
        assert!(validate_column_name("age").is_ok());
        assert!(validate_column_name("field123").is_ok());
    }

    #[test]
    fn test_reserved_column_names() {
        // All reserved column names start with underscore, so they get caught by StartsWithUnderscore check
        assert_eq!(
            validate_column_name("_seq"),
            Err(ValidationError::StartsWithUnderscore)
        );
        assert_eq!(
            validate_column_name("_deleted"),
            Err(ValidationError::StartsWithUnderscore)
        );
        assert_eq!(
            validate_column_name("_row_id"),
            Err(ValidationError::StartsWithUnderscore)
        );
        assert_eq!(
            validate_column_name("_id"),
            Err(ValidationError::StartsWithUnderscore)
        );
        assert_eq!(
            validate_column_name("_updated"),
            Err(ValidationError::StartsWithUnderscore)
        );
    }

    #[test]
    fn test_starts_with_underscore() {
        assert_eq!(
            validate_column_name("_custom"),
            Err(ValidationError::StartsWithUnderscore)
        );
        assert_eq!(
            validate_table_name("_table"),
            Err(ValidationError::StartsWithUnderscore)
        );
        assert_eq!(
            validate_namespace_name("_namespace"),
            Err(ValidationError::StartsWithUnderscore)
        );
    }

    #[test]
    fn test_starts_with_number() {
        assert_eq!(
            validate_column_name("123field"),
            Err(ValidationError::StartsWithNumber)
        );
        assert_eq!(
            validate_table_name("1table"),
            Err(ValidationError::StartsWithNumber)
        );
    }

    #[test]
    fn test_contains_spaces() {
        assert_eq!(
            validate_column_name("user name"),
            Err(ValidationError::ContainsSpaces)
        );
        assert_eq!(
            validate_table_name("my table"),
            Err(ValidationError::ContainsSpaces)
        );
    }

    #[test]
    fn test_invalid_characters() {
        assert_eq!(
            validate_column_name("user-name"),
            Err(ValidationError::InvalidCharacters("user-name".to_string()))
        );
        assert_eq!(
            validate_table_name("table@name"),
            Err(ValidationError::InvalidCharacters("table@name".to_string()))
        );
        assert_eq!(
            validate_namespace_name("name.space"),
            Err(ValidationError::InvalidCharacters("name.space".to_string()))
        );
    }

    #[test]
    fn test_too_long() {
        let long_name = "a".repeat(65);
        assert_eq!(
            validate_column_name(&long_name),
            Err(ValidationError::TooLong(65))
        );
    }

    #[test]
    fn test_empty_name() {
        assert_eq!(validate_column_name(""), Err(ValidationError::Empty));
    }

    #[test]
    fn test_reserved_sql_keywords() {
        assert_eq!(
            validate_table_name("select"),
            Err(ValidationError::ReservedSqlKeyword("select".to_string()))
        );
        assert_eq!(
            validate_column_name("from"),
            Err(ValidationError::ReservedSqlKeyword("from".to_string()))
        );
        assert_eq!(
            validate_namespace_name("WHERE"),
            Err(ValidationError::ReservedSqlKeyword("WHERE".to_string()))
        );
    }
}
