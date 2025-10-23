//! ALTER TABLE statement parser
//!
//! Parses SQL statements like:
//! - ALTER TABLE messages ADD COLUMN age INT
//! - ALTER TABLE messages DROP COLUMN age
//! - ALTER TABLE messages MODIFY COLUMN age BIGINT
//! - ALTER USER TABLE messages ADD COLUMN age INT
//! - ALTER SHARED TABLE messages ADD COLUMN age INT

use crate::ddl::DdlResult;

use kalamdb_commons::models::{NamespaceId, TableName};

/// Column alteration operation
#[derive(Debug, Clone, PartialEq)]
pub enum ColumnOperation {
    /// Add a new column
    Add {
        column_name: String,
        data_type: String,
        nullable: bool,
        default_value: Option<String>,
    },
    /// Drop an existing column
    Drop { column_name: String },
    /// Modify an existing column's data type
    Modify {
        column_name: String,
        new_data_type: String,
        nullable: Option<bool>,
    },
}

/// ALTER TABLE statement
#[derive(Debug, Clone, PartialEq)]
pub struct AlterTableStatement {
    /// Table name to alter
    pub table_name: TableName,

    /// Namespace ID (defaults to current namespace)
    pub namespace_id: NamespaceId,

    /// Column operation to perform
    pub operation: ColumnOperation,
}

impl AlterTableStatement {
    /// Parse an ALTER TABLE statement from SQL (optimized)
    ///
    /// Supports syntax:
    /// - ALTER TABLE name ADD COLUMN col_name data_type [NULL | NOT NULL] [DEFAULT value]
    /// - ALTER TABLE name DROP COLUMN col_name
    /// - ALTER TABLE name MODIFY COLUMN col_name new_data_type [NULL | NOT NULL]
    /// - ALTER USER TABLE ... (explicitly specify table type)
    /// - ALTER SHARED TABLE ... (explicitly specify table type)
    pub fn parse(sql: &str, current_namespace: &NamespaceId) -> DdlResult<Self> {
        let tokens: Vec<&str> = sql.split_whitespace().collect();
        
        if tokens.is_empty() || !tokens[0].eq_ignore_ascii_case("ALTER") {
            return Err("Expected ALTER TABLE statement".to_string());
        }

        // Extract table name (handles ALTER TABLE, ALTER USER TABLE, ALTER SHARED TABLE)
        let table_name = Self::extract_table_name_from_tokens(&tokens)?;

        // Find operation keyword position
        let op_pos = tokens.iter()
            .position(|&t| matches!(
                t.to_uppercase().as_str(),
                "ADD" | "DROP" | "MODIFY"
            ))
            .ok_or_else(|| "Expected ADD COLUMN, DROP COLUMN, or MODIFY COLUMN operation".to_string())?;

        let operation_upper = tokens[op_pos].to_uppercase();
        let operation = match operation_upper.as_str() {
            "ADD" => Self::parse_add_column_from_tokens(&tokens[op_pos..])?,
            "DROP" => Self::parse_drop_column_from_tokens(&tokens[op_pos..])?,
            "MODIFY" => Self::parse_modify_column_from_tokens(&tokens[op_pos..])?,
            _ => return Err("Expected ADD COLUMN, DROP COLUMN, or MODIFY COLUMN operation".to_string()),
        };

        Ok(Self {
            table_name: TableName::new(&table_name),
            namespace_id: current_namespace.clone(),
            operation,
        })
    }

    /// Extract table name from ALTER TABLE statement tokens (optimized)
    fn extract_table_name_from_tokens(tokens: &[&str]) -> DdlResult<String> {
        // Skip ALTER, then check for USER/SHARED TABLE or just TABLE
        let skip = if tokens.len() >= 3 {
            let second = tokens[1].to_uppercase();
            if second == "USER" || second == "SHARED" {
                3 // Skip "ALTER USER/SHARED TABLE"
            } else if second == "TABLE" {
                2 // Skip "ALTER TABLE"
            } else {
                return Err("Expected TABLE keyword".to_string());
            }
        } else {
            return Err("Table name is required".to_string());
        };

        tokens.get(skip)
            .map(|s| s.to_string())
            .ok_or_else(|| "Table name is required".to_string())
    }

    /// Parse ADD COLUMN operation from tokens (optimized)
    fn parse_add_column_from_tokens(tokens: &[&str]) -> DdlResult<ColumnOperation> {
        // Expect: ADD [COLUMN] col_name data_type [NOT NULL] [DEFAULT value]
        let start_idx = if tokens.len() > 1 && tokens[1].eq_ignore_ascii_case("COLUMN") {
            2
        } else {
            1
        };

        if tokens.len() < start_idx + 2 {
            return Err("Column definition requires name and data type".to_string());
        }

        let column_name = tokens[start_idx].to_string();
        let data_type = tokens[start_idx + 1].to_string();

        // Check for NOT NULL
        let nullable = !tokens[start_idx + 2..]
            .windows(2)
            .any(|w| w[0].eq_ignore_ascii_case("NOT") && w[1].eq_ignore_ascii_case("NULL"));

        // Extract default value if present
        let default_value = tokens[start_idx + 2..]
            .windows(2)
            .find(|w| w[0].eq_ignore_ascii_case("DEFAULT")).map(|w| w[1].trim_matches('\'').trim_matches('"').to_string());

        Ok(ColumnOperation::Add {
            column_name,
            data_type,
            nullable,
            default_value,
        })
    }

    /// Parse DROP COLUMN operation from tokens (optimized)
    fn parse_drop_column_from_tokens(tokens: &[&str]) -> DdlResult<ColumnOperation> {
        // Expect: DROP [COLUMN] col_name
        let col_idx = if tokens.len() > 1 && tokens[1].eq_ignore_ascii_case("COLUMN") {
            2
        } else {
            1
        };

        let column_name = tokens
            .get(col_idx)
            .ok_or_else(|| "Column name is required".to_string())?
            .to_string();

        Ok(ColumnOperation::Drop { column_name })
    }

    /// Parse MODIFY COLUMN operation from tokens (optimized)
    fn parse_modify_column_from_tokens(tokens: &[&str]) -> DdlResult<ColumnOperation> {
        // Expect: MODIFY [COLUMN] col_name new_data_type [NOT NULL]
        let start_idx = if tokens.len() > 1 && tokens[1].eq_ignore_ascii_case("COLUMN") {
            2
        } else {
            1
        };

        if tokens.len() < start_idx + 2 {
            return Err("Column modification requires name and new data type".to_string());
        }

        let column_name = tokens[start_idx].to_string();
        let new_data_type = tokens[start_idx + 1].to_string();

        // Check for NULL/NOT NULL
        let nullable = if tokens.len() > start_idx + 2 {
            let has_null = tokens[start_idx + 2..]
                .iter()
                .any(|&t| t.eq_ignore_ascii_case("NULL"));
            let has_not_null = tokens[start_idx + 2..]
                .windows(2)
                .any(|w| w[0].eq_ignore_ascii_case("NOT") && w[1].eq_ignore_ascii_case("NULL"));

            if has_not_null {
                Some(false)
            } else if has_null {
                Some(true)
            } else {
                None
            }
        } else {
            None
        };

        Ok(ColumnOperation::Modify {
            column_name,
            new_data_type,
            nullable,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_namespace() -> NamespaceId {
        NamespaceId::new("test_app")
    }

    #[test]
    fn test_parse_add_column() {
        let stmt = AlterTableStatement::parse(
            "ALTER TABLE messages ADD COLUMN age INT",
            &test_namespace(),
        )
        .unwrap();

        assert_eq!(stmt.table_name.as_str(), "messages");

        match stmt.operation {
            ColumnOperation::Add {
                column_name,
                data_type,
                nullable,
                default_value,
            } => {
                assert_eq!(column_name, "age");
                assert_eq!(data_type, "INT");
                assert!(nullable);
                assert_eq!(default_value, None);
            }
            _ => panic!("Expected Add operation"),
        }
    }

    #[test]
    fn test_parse_add_column_not_null() {
        let stmt = AlterTableStatement::parse(
            "ALTER TABLE messages ADD COLUMN age INT NOT NULL",
            &test_namespace(),
        )
        .unwrap();

        match stmt.operation {
            ColumnOperation::Add { nullable, .. } => {
                assert!(!nullable);
            }
            _ => panic!("Expected Add operation"),
        }
    }

    #[test]
    fn test_parse_add_column_with_default() {
        let stmt = AlterTableStatement::parse(
            "ALTER TABLE messages ADD COLUMN age INT DEFAULT 0",
            &test_namespace(),
        )
        .unwrap();

        match stmt.operation {
            ColumnOperation::Add {
                column_name,
                default_value,
                ..
            } => {
                assert_eq!(column_name, "age");
                assert_eq!(default_value, Some("0".to_string()));
            }
            _ => panic!("Expected Add operation"),
        }
    }

    #[test]
    fn test_parse_drop_column() {
        let stmt =
            AlterTableStatement::parse("ALTER TABLE messages DROP COLUMN age", &test_namespace())
                .unwrap();

        assert_eq!(stmt.table_name.as_str(), "messages");

        match stmt.operation {
            ColumnOperation::Drop { column_name } => {
                assert_eq!(column_name, "age");
            }
            _ => panic!("Expected Drop operation"),
        }
    }

    #[test]
    fn test_parse_drop_column_shorthand() {
        let stmt =
            AlterTableStatement::parse("ALTER TABLE messages DROP age", &test_namespace()).unwrap();

        match stmt.operation {
            ColumnOperation::Drop { column_name } => {
                assert_eq!(column_name, "age");
            }
            _ => panic!("Expected Drop operation"),
        }
    }

    #[test]
    fn test_parse_modify_column() {
        let stmt = AlterTableStatement::parse(
            "ALTER TABLE messages MODIFY COLUMN age BIGINT",
            &test_namespace(),
        )
        .unwrap();

        assert_eq!(stmt.table_name.as_str(), "messages");

        match stmt.operation {
            ColumnOperation::Modify {
                column_name,
                new_data_type,
                nullable,
            } => {
                assert_eq!(column_name, "age");
                assert_eq!(new_data_type, "BIGINT");
                assert_eq!(nullable, None);
            }
            _ => panic!("Expected Modify operation"),
        }
    }

    #[test]
    fn test_parse_modify_column_with_nullable() {
        let stmt = AlterTableStatement::parse(
            "ALTER TABLE messages MODIFY COLUMN age BIGINT NOT NULL",
            &test_namespace(),
        )
        .unwrap();

        match stmt.operation {
            ColumnOperation::Modify { nullable, .. } => {
                assert_eq!(nullable, Some(false));
            }
            _ => panic!("Expected Modify operation"),
        }
    }

    #[test]
    fn test_parse_alter_user_table() {
        let stmt = AlterTableStatement::parse(
            "ALTER USER TABLE messages ADD COLUMN age INT",
            &test_namespace(),
        )
        .unwrap();

        assert_eq!(stmt.table_name.as_str(), "messages");
    }

    #[test]
    fn test_parse_alter_shared_table() {
        let stmt = AlterTableStatement::parse(
            "ALTER SHARED TABLE conversations DROP COLUMN old_field",
            &test_namespace(),
        )
        .unwrap();

        assert_eq!(stmt.table_name.as_str(), "conversations");
    }

    #[test]
    fn test_parse_invalid_statement() {
        let result = AlterTableStatement::parse("SELECT * FROM messages", &test_namespace());
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_missing_column_name() {
        let result =
            AlterTableStatement::parse("ALTER TABLE messages ADD COLUMN", &test_namespace());
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_missing_operation() {
        let result = AlterTableStatement::parse("ALTER TABLE messages", &test_namespace());
        assert!(result.is_err());
    }
}
