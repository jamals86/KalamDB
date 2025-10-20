//! ALTER TABLE statement parser
//!
//! Parses SQL statements like:
//! - ALTER TABLE messages ADD COLUMN age INT
//! - ALTER TABLE messages DROP COLUMN age
//! - ALTER TABLE messages MODIFY COLUMN age BIGINT
//! - ALTER USER TABLE messages ADD COLUMN age INT
//! - ALTER SHARED TABLE messages ADD COLUMN age INT

use crate::catalog::{NamespaceId, TableName};
use crate::error::KalamDbError;

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
    Drop {
        column_name: String,
    },
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
    /// Parse an ALTER TABLE statement from SQL
    ///
    /// Supports syntax:
    /// - ALTER TABLE name ADD COLUMN col_name data_type [NULL | NOT NULL] [DEFAULT value]
    /// - ALTER TABLE name DROP COLUMN col_name
    /// - ALTER TABLE name MODIFY COLUMN col_name new_data_type [NULL | NOT NULL]
    /// - ALTER USER TABLE ... (explicitly specify table type)
    /// - ALTER SHARED TABLE ... (explicitly specify table type)
    pub fn parse(sql: &str, current_namespace: &NamespaceId) -> Result<Self, KalamDbError> {
        let sql_trim = sql.trim();
        let sql_upper = sql_trim.to_uppercase();

        if !sql_upper.starts_with("ALTER") {
            return Err(KalamDbError::InvalidSql(
                "Expected ALTER TABLE statement".to_string(),
            ));
        }

        // Extract table name (handles ALTER TABLE, ALTER USER TABLE, ALTER SHARED TABLE)
        let table_name = Self::extract_table_name(sql_trim)?;

        // Determine the operation type
        let operation = if sql_upper.contains(" ADD COLUMN ") || sql_upper.contains(" ADD ") {
            Self::parse_add_column(sql_trim)?
        } else if sql_upper.contains(" DROP COLUMN ") || sql_upper.contains(" DROP ") {
            Self::parse_drop_column(sql_trim)?
        } else if sql_upper.contains(" MODIFY COLUMN ") || sql_upper.contains(" MODIFY ") {
            Self::parse_modify_column(sql_trim)?
        } else {
            return Err(KalamDbError::InvalidSql(
                "Expected ADD COLUMN, DROP COLUMN, or MODIFY COLUMN operation".to_string(),
            ));
        };

        Ok(Self {
            table_name: TableName::new(&table_name),
            namespace_id: current_namespace.clone(),
            operation,
        })
    }

    /// Extract table name from ALTER TABLE statement
    fn extract_table_name(sql: &str) -> Result<String, KalamDbError> {
        let sql_upper = sql.to_uppercase();
        
        // Try different patterns
        let after_alter = if sql_upper.contains("ALTER USER TABLE") {
            sql.split_whitespace()
                .skip(3) // Skip "ALTER USER TABLE"
                .next()
        } else if sql_upper.contains("ALTER SHARED TABLE") {
            sql.split_whitespace()
                .skip(3) // Skip "ALTER SHARED TABLE"
                .next()
        } else if sql_upper.contains("ALTER TABLE") {
            sql.split_whitespace()
                .skip(2) // Skip "ALTER TABLE"
                .next()
        } else {
            None
        };

        after_alter
            .map(|s| s.to_string())
            .ok_or_else(|| KalamDbError::InvalidSql("Table name is required".to_string()))
    }

    /// Parse ADD COLUMN operation
    fn parse_add_column(sql: &str) -> Result<ColumnOperation, KalamDbError> {
        let sql_upper = sql.to_uppercase();
        
        // Find the position of ADD COLUMN or ADD
        let add_pos = if sql_upper.contains(" ADD COLUMN ") {
            sql_upper.find(" ADD COLUMN ").unwrap()
        } else {
            sql_upper.find(" ADD ").unwrap()
        };

        // Extract everything after ADD COLUMN
        let column_def = &sql[add_pos..];
        let column_def = column_def
            .strip_prefix(" ADD COLUMN ")
            .or_else(|| column_def.strip_prefix(" add column "))
            .or_else(|| column_def.strip_prefix(" ADD "))
            .or_else(|| column_def.strip_prefix(" add "))
            .unwrap_or(column_def)
            .trim();

        // Parse column definition: name type [NULL|NOT NULL] [DEFAULT value]
        let parts: Vec<&str> = column_def.split_whitespace().collect();
        if parts.len() < 2 {
            return Err(KalamDbError::InvalidSql(
                "Column definition requires name and data type".to_string(),
            ));
        }

        let column_name = parts[0].to_string();
        let data_type = parts[1].to_string();
        
        let upper_def = column_def.to_uppercase();
        let nullable = !upper_def.contains("NOT NULL");
        
        // Extract default value if present
        let default_value = if upper_def.contains("DEFAULT") {
            let default_pos = upper_def.find("DEFAULT").unwrap();
            let default_part = &column_def[default_pos + 7..].trim();
            let value = default_part.split_whitespace().next();
            value.map(|s| s.trim_matches('\'').trim_matches('"').to_string())
        } else {
            None
        };

        Ok(ColumnOperation::Add {
            column_name,
            data_type,
            nullable,
            default_value,
        })
    }

    /// Parse DROP COLUMN operation
    fn parse_drop_column(sql: &str) -> Result<ColumnOperation, KalamDbError> {
        let sql_upper = sql.to_uppercase();
        
        // Find the position of DROP COLUMN or DROP
        let drop_pos = if sql_upper.contains(" DROP COLUMN ") {
            sql_upper.find(" DROP COLUMN ").unwrap()
        } else {
            sql_upper.find(" DROP ").unwrap()
        };

        // Extract column name after DROP COLUMN
        let column_part = &sql[drop_pos..];
        let column_name = column_part
            .strip_prefix(" DROP COLUMN ")
            .or_else(|| column_part.strip_prefix(" drop column "))
            .or_else(|| column_part.strip_prefix(" DROP "))
            .or_else(|| column_part.strip_prefix(" drop "))
            .and_then(|s| s.trim().split_whitespace().next())
            .ok_or_else(|| KalamDbError::InvalidSql("Column name is required".to_string()))?;

        Ok(ColumnOperation::Drop {
            column_name: column_name.to_string(),
        })
    }

    /// Parse MODIFY COLUMN operation
    fn parse_modify_column(sql: &str) -> Result<ColumnOperation, KalamDbError> {
        let sql_upper = sql.to_uppercase();
        
        // Find the position of MODIFY COLUMN or MODIFY
        let modify_pos = if sql_upper.contains(" MODIFY COLUMN ") {
            sql_upper.find(" MODIFY COLUMN ").unwrap()
        } else {
            sql_upper.find(" MODIFY ").unwrap()
        };

        // Extract column definition after MODIFY COLUMN
        let column_def = &sql[modify_pos..];
        let column_def = column_def
            .strip_prefix(" MODIFY COLUMN ")
            .or_else(|| column_def.strip_prefix(" modify column "))
            .or_else(|| column_def.strip_prefix(" MODIFY "))
            .or_else(|| column_def.strip_prefix(" modify "))
            .unwrap_or(column_def)
            .trim();

        // Parse column definition: name new_type [NULL|NOT NULL]
        let parts: Vec<&str> = column_def.split_whitespace().collect();
        if parts.len() < 2 {
            return Err(KalamDbError::InvalidSql(
                "Column modification requires name and new data type".to_string(),
            ));
        }

        let column_name = parts[0].to_string();
        let new_data_type = parts[1].to_string();
        
        let upper_def = column_def.to_uppercase();
        let nullable = if upper_def.contains("NULL") {
            Some(!upper_def.contains("NOT NULL"))
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
        let stmt = AlterTableStatement::parse(
            "ALTER TABLE messages DROP COLUMN age",
            &test_namespace(),
        )
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
        let stmt = AlterTableStatement::parse(
            "ALTER TABLE messages DROP age",
            &test_namespace(),
        )
        .unwrap();
        
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
        let result = AlterTableStatement::parse("ALTER TABLE messages ADD COLUMN", &test_namespace());
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_missing_operation() {
        let result = AlterTableStatement::parse("ALTER TABLE messages", &test_namespace());
        assert!(result.is_err());
    }
}
