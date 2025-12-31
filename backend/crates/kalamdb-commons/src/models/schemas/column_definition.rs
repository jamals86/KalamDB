//! Column definition for table schemas

use crate::models::datatypes::KalamDataType;
use crate::models::schemas::column_default::ColumnDefault;
use serde::{Deserialize, Serialize};

/// Complete definition of a table column.
/// Fields ordered for optimal memory alignment (8-byte types first).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColumnDefinition {
    /// Column name (case-insensitive, stored as lowercase)
    pub column_name: String,

    /// Optional column comment/description
    pub column_comment: Option<String>,

    /// Data type
    pub data_type: KalamDataType,

    /// Default value specification
    pub default_value: ColumnDefault,

    /// Ordinal position in table (1-indexed, sequential)
    /// Determines SELECT * column ordering
    pub ordinal_position: u32,

    /// Whether column can contain NULL values
    pub is_nullable: bool,

    /// Whether this column is part of the primary key
    pub is_primary_key: bool,

    /// Whether this column is part of the partition key (for distributed tables)
    pub is_partition_key: bool,
}

impl ColumnDefinition {
    /// Create a new column definition
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use kalamdb_commons::models::schemas::{ColumnDefinition, ColumnDefault};
    /// use kalamdb_commons::models::datatypes::KalamDataType;
    ///
    /// let column = ColumnDefinition::new(
    ///     "user_id",
    ///     1,
    ///     KalamDataType::Uuid,
    ///     false,  // not nullable
    ///     true,   // is primary key
    ///     false,  // not partition key
    ///     ColumnDefault::None,
    ///     Some("Primary key for users table".into()),
    /// );
    ///
    /// assert_eq!(column.column_name, "user_id");
    /// assert_eq!(column.data_type, KalamDataType::Uuid);
    /// assert!(!column.is_nullable);
    /// assert!(column.is_primary_key);
    /// ```
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        column_name: impl Into<String>,
        ordinal_position: u32,
        data_type: KalamDataType,
        is_nullable: bool,
        is_primary_key: bool,
        is_partition_key: bool,
        default_value: ColumnDefault,
        column_comment: Option<String>,
    ) -> Self {
        Self {
            column_name: column_name.into().to_lowercase(),
            ordinal_position,
            data_type,
            is_nullable,
            is_primary_key,
            is_partition_key,
            default_value,
            column_comment,
        }
    }

    /// Create a simple column with minimal configuration
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use kalamdb_commons::models::schemas::ColumnDefinition;
    /// use kalamdb_commons::models::datatypes::KalamDataType;
    ///
    /// let column = ColumnDefinition::simple("email", 2, KalamDataType::Utf8);
    ///
    /// assert_eq!(column.column_name, "email");
    /// assert_eq!(column.ordinal_position, 2);
    /// assert!(column.is_nullable);
    /// assert!(!column.is_primary_key);
    /// ```
    pub fn simple(
        column_name: impl Into<String>,
        ordinal_position: u32,
        data_type: KalamDataType,
    ) -> Self {
        Self {
            column_name: column_name.into().to_lowercase(),
            ordinal_position,
            data_type,
            is_nullable: true,
            is_primary_key: false,
            is_partition_key: false,
            default_value: ColumnDefault::None,
            column_comment: None,
        }
    }

    /// Create a primary key column
    pub fn primary_key(
        column_name: impl Into<String>,
        ordinal_position: u32,
        data_type: KalamDataType,
    ) -> Self {
        Self {
            column_name: column_name.into().to_lowercase(),
            ordinal_position,
            data_type,
            is_nullable: false, // Primary keys cannot be NULL
            is_primary_key: true,
            is_partition_key: false,
            default_value: ColumnDefault::None,
            column_comment: None,
        }
    }

    /// Get SQL DDL fragment for this column
    pub fn to_sql(&self) -> String {
        let mut parts = vec![self.column_name.clone(), self.data_type.sql_name()];

        if self.is_primary_key {
            parts.push("PRIMARY KEY".to_string());
        }

        if !self.is_nullable && !self.is_primary_key {
            parts.push("NOT NULL".to_string());
        }

        if !self.default_value.is_none() {
            parts.push(format!("DEFAULT {}", self.default_value.to_sql()));
        }

        if let Some(ref comment) = self.column_comment {
            parts.push(format!("COMMENT '{}'", comment.replace('\'', "''")));
        }

        parts.join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bincode::config::standard;
    use bincode::serde::{decode_from_slice, encode_to_vec};
    use serde_json::json;

    #[test]
    fn test_simple_column() {
        let col = ColumnDefinition::simple("name", 1, KalamDataType::Text);
        assert_eq!(col.column_name, "name");
        assert_eq!(col.ordinal_position, 1);
        assert!(col.is_nullable);
        assert!(!col.is_primary_key);
    }

    #[test]
    fn test_primary_key_column() {
        let col = ColumnDefinition::primary_key("id", 1, KalamDataType::BigInt);
        assert_eq!(col.column_name, "id");
        assert!(!col.is_nullable);
        assert!(col.is_primary_key);
    }

    #[test]
    fn test_column_with_default() {
        let col = ColumnDefinition::new(
            "created_at",
            3,
            KalamDataType::Timestamp,
            false,
            false,
            false,
            ColumnDefault::function("NOW", vec![]),
            Some("Creation timestamp".to_string()),
        );

        assert_eq!(col.ordinal_position, 3);
        assert!(!col.is_nullable);
        assert_eq!(col.column_comment, Some("Creation timestamp".to_string()));
    }

    #[test]
    fn test_to_sql() {
        let col = ColumnDefinition::primary_key("id", 1, KalamDataType::BigInt);
        let sql = col.to_sql();
        assert!(sql.contains("id"));
        assert!(sql.contains("BIGINT"));
        assert!(sql.contains("PRIMARY KEY"));

        let col = ColumnDefinition::new(
            "status",
            2,
            KalamDataType::Text,
            false,
            false,
            false,
            ColumnDefault::literal(json!("pending")),
            None,
        );
        let sql = col.to_sql();
        assert!(sql.contains("status"));
        assert!(sql.contains("TEXT"));
        assert!(sql.contains("NOT NULL"));
        assert!(sql.contains("DEFAULT 'pending'"));
    }

    #[test]
    fn test_serialization() {
        let col = ColumnDefinition::simple("test", 1, KalamDataType::Int);
        let json = serde_json::to_string(&col).unwrap();
        let decoded: ColumnDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(col, decoded);
    }

    #[test]
    fn test_partition_key() {
        let col = ColumnDefinition::new(
            "partition_key",
            1,
            KalamDataType::Text,
            false,
            false,
            true,
            ColumnDefault::None,
            None,
        );
        assert!(col.is_partition_key);
        assert!(!col.is_primary_key);
    }

    #[test]
    fn test_bincode_roundtrip() {
        let column = ColumnDefinition::new(
            "status",
            5,
            KalamDataType::Text,
            false,
            false,
            false,
            ColumnDefault::literal(json!({"state": "active"})),
            Some("Status column".to_string()),
        );

        let config = standard();
        let bytes = encode_to_vec(&column, config).expect("encode column definition");
        let (decoded, _): (ColumnDefinition, usize) =
            decode_from_slice(&bytes, config).expect("decode column definition");

        assert_eq!(decoded, column);
    }

    #[test]
    fn test_column_name_case_insensitive() {
        // Column names should be normalized to lowercase
        let col1 = ColumnDefinition::simple("FirstName", 1, KalamDataType::Text);
        let col2 = ColumnDefinition::simple("firstname", 1, KalamDataType::Text);
        let col3 = ColumnDefinition::simple("FIRSTNAME", 1, KalamDataType::Text);

        assert_eq!(col1.column_name, "firstname");
        assert_eq!(col2.column_name, "firstname");
        assert_eq!(col3.column_name, "firstname");
        assert_eq!(col1, col2);
        assert_eq!(col2, col3);

        // primary_key constructor also normalizes
        let pk = ColumnDefinition::primary_key("UserId", 1, KalamDataType::BigInt);
        assert_eq!(pk.column_name, "userid");

        // new constructor also normalizes
        let full = ColumnDefinition::new(
            "CreatedAt",
            2,
            KalamDataType::Timestamp,
            false,
            false,
            false,
            ColumnDefault::None,
            None,
        );
        assert_eq!(full.column_name, "createdat");
    }
}
