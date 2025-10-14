// SQL parser for INSERT and SELECT statements
use crate::error::StorageError;

/// Represents a parsed SQL statement
#[derive(Debug, Clone, PartialEq)]
pub enum SqlStatement {
    Insert(InsertStatement),
    Select(SelectStatement),
}

/// Represents a parsed INSERT statement
#[derive(Debug, Clone, PartialEq)]
pub struct InsertStatement {
    pub table: String,
    pub columns: Vec<String>,
    pub values: Vec<String>,
}

/// Represents a parsed SELECT statement
#[derive(Debug, Clone, PartialEq)]
pub struct SelectStatement {
    pub table: String,
    pub columns: Vec<String>,
    pub where_clause: Option<WhereClause>,
    pub order_by: Option<OrderBy>,
    pub limit: Option<usize>,
}

/// Represents a WHERE clause
#[derive(Debug, Clone, PartialEq)]
pub struct WhereClause {
    pub conditions: Vec<Condition>,
}

/// Represents a single condition in WHERE clause
#[derive(Debug, Clone, PartialEq)]
pub struct Condition {
    pub column: String,
    pub operator: String,
    pub value: String,
}

/// Represents an ORDER BY clause
#[derive(Debug, Clone, PartialEq)]
pub struct OrderBy {
    pub column: String,
    pub direction: String, // "ASC" or "DESC"
}

/// SQL parser
pub struct SqlParser;

impl SqlParser {
    /// Parse a SQL statement
    pub fn parse(sql: &str) -> Result<SqlStatement, StorageError> {
        let sql = sql.trim();
        
        // Determine statement type
        let sql_upper = sql.to_uppercase();
        
        if sql_upper.starts_with("INSERT") {
            Self::parse_insert(sql)
        } else if sql_upper.starts_with("SELECT") {
            Self::parse_select(sql)
        } else {
            Err(StorageError::InvalidQuery(
                "Only INSERT and SELECT statements are supported".to_string(),
            ))
        }
    }
    
    /// Parse INSERT INTO statement
    /// Format: INSERT INTO messages (col1, col2, ...) VALUES (val1, val2, ...)
    fn parse_insert(sql: &str) -> Result<SqlStatement, StorageError> {
        let sql = sql.trim();
        
        // Extract table name
        let table = Self::extract_table_name_from_insert(sql)?;
        
        // Extract columns
        let columns = Self::extract_columns_from_insert(sql)?;
        
        // Extract values
        let values = Self::extract_values_from_insert(sql)?;
        
        // Validate columns and values match
        if columns.len() != values.len() {
            return Err(StorageError::InvalidQuery(format!(
                "Column count ({}) doesn't match value count ({})",
                columns.len(),
                values.len()
            )));
        }
        
        Ok(SqlStatement::Insert(InsertStatement {
            table,
            columns,
            values,
        }))
    }
    
    /// Parse SELECT statement
    /// Format: SELECT * FROM messages WHERE col = 'val' ORDER BY col DESC LIMIT 10
    fn parse_select(sql: &str) -> Result<SqlStatement, StorageError> {
        let sql = sql.trim();
        
        // Extract columns (after SELECT, before FROM)
        let columns = Self::extract_select_columns(sql)?;
        
        // Extract table name
        let table = Self::extract_table_name_from_select(sql)?;
        
        // Extract WHERE clause if present
        let where_clause = Self::extract_where_clause(sql)?;
        
        // Extract ORDER BY if present
        let order_by = Self::extract_order_by(sql)?;
        
        // Extract LIMIT if present
        let limit = Self::extract_limit(sql)?;
        
        Ok(SqlStatement::Select(SelectStatement {
            table,
            columns,
            where_clause,
            order_by,
            limit,
        }))
    }
    
    /// Extract table name from INSERT statement
    fn extract_table_name_from_insert(sql: &str) -> Result<String, StorageError> {
        let sql_upper = sql.to_uppercase();
        let insert_into = "INSERT INTO ";
        
        if let Some(pos) = sql_upper.find(insert_into) {
            let after_insert = &sql[pos + insert_into.len()..];
            
            // Find the opening parenthesis or whitespace
            let end_pos = after_insert
                .find('(')
                .or_else(|| after_insert.find(char::is_whitespace))
                .unwrap_or(after_insert.len());
            
            let table = after_insert[..end_pos].trim().to_string();
            
            if table.is_empty() {
                return Err(StorageError::InvalidQuery(
                    "Table name is required".to_string(),
                ));
            }
            
            Ok(table)
        } else {
            Err(StorageError::InvalidQuery(
                "Invalid INSERT statement format".to_string(),
            ))
        }
    }
    
    /// Extract columns from INSERT statement
    fn extract_columns_from_insert(sql: &str) -> Result<Vec<String>, StorageError> {
        // Find the column list between parentheses after table name
        if let Some(start) = sql.find('(') {
            if let Some(end) = sql[start..].find(')') {
                let columns_str = &sql[start + 1..start + end];
                let columns: Vec<String> = columns_str
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                
                if columns.is_empty() {
                    return Err(StorageError::InvalidQuery(
                        "No columns specified".to_string(),
                    ));
                }
                
                return Ok(columns);
            }
        }
        
        Err(StorageError::InvalidQuery(
            "Invalid column list format".to_string(),
        ))
    }
    
    /// Extract values from INSERT statement
    fn extract_values_from_insert(sql: &str) -> Result<Vec<String>, StorageError> {
        let sql_upper = sql.to_uppercase();
        
        // Find VALUES keyword
        if let Some(values_pos) = sql_upper.find("VALUES") {
            let after_values = &sql[values_pos + 6..].trim();
            
            // Find the value list between parentheses
            if let Some(start) = after_values.find('(') {
                if let Some(end) = after_values[start..].rfind(')') {
                    let values_str = &after_values[start + 1..start + end];
                    let values = Self::parse_value_list(values_str)?;
                    
                    if values.is_empty() {
                        return Err(StorageError::InvalidQuery(
                            "No values specified".to_string(),
                        ));
                    }
                    
                    return Ok(values);
                }
            }
        }
        
        Err(StorageError::InvalidQuery(
            "Invalid VALUES clause format".to_string(),
        ))
    }
    
    /// Parse a comma-separated list of values, handling quoted strings
    fn parse_value_list(values_str: &str) -> Result<Vec<String>, StorageError> {
        let mut values = Vec::new();
        let mut current_value = String::new();
        let mut in_quotes = false;
        let mut quote_char = ' ';
        let mut chars = values_str.chars().peekable();
        
        while let Some(ch) = chars.next() {
            match ch {
                '\'' | '"' if !in_quotes => {
                    in_quotes = true;
                    quote_char = ch;
                }
                '\'' | '"' if in_quotes && ch == quote_char => {
                    // Check for escaped quote
                    if chars.peek() == Some(&ch) {
                        current_value.push(ch);
                        chars.next(); // Consume the escaped quote
                    } else {
                        in_quotes = false;
                    }
                }
                ',' if !in_quotes => {
                    values.push(current_value.trim().to_string());
                    current_value.clear();
                }
                _ => {
                    current_value.push(ch);
                }
            }
        }
        
        // Add the last value
        if !current_value.trim().is_empty() {
            values.push(current_value.trim().to_string());
        }
        
        Ok(values)
    }
    
    /// Extract columns from SELECT statement
    fn extract_select_columns(sql: &str) -> Result<Vec<String>, StorageError> {
        let sql_upper = sql.to_uppercase();
        
        if let Some(select_pos) = sql_upper.find("SELECT") {
            if let Some(from_pos) = sql_upper.find("FROM") {
                let columns_str = &sql[select_pos + 6..from_pos].trim();
                
                if columns_str == &"*" {
                    return Ok(vec!["*".to_string()]);
                }
                
                let columns: Vec<String> = columns_str
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                
                if columns.is_empty() {
                    return Err(StorageError::InvalidQuery(
                        "No columns specified in SELECT".to_string(),
                    ));
                }
                
                return Ok(columns);
            }
        }
        
        Err(StorageError::InvalidQuery(
            "Invalid SELECT statement format".to_string(),
        ))
    }
    
    /// Extract table name from SELECT statement
    fn extract_table_name_from_select(sql: &str) -> Result<String, StorageError> {
        let sql_upper = sql.to_uppercase();
        
        if let Some(from_pos) = sql_upper.find("FROM") {
            let after_from = &sql[from_pos + 4..].trim();
            
            // Find the end of table name (WHERE, ORDER, LIMIT, or end of string)
            let end_pos = after_from
                .to_uppercase()
                .find("WHERE")
                .or_else(|| after_from.to_uppercase().find("ORDER"))
                .or_else(|| after_from.to_uppercase().find("LIMIT"))
                .unwrap_or(after_from.len());
            
            let table = after_from[..end_pos].trim().to_string();
            
            if table.is_empty() {
                return Err(StorageError::InvalidQuery(
                    "Table name is required in FROM clause".to_string(),
                ));
            }
            
            Ok(table)
        } else {
            Err(StorageError::InvalidQuery(
                "FROM clause is required in SELECT statement".to_string(),
            ))
        }
    }
    
    /// Extract WHERE clause from SELECT statement
    fn extract_where_clause(sql: &str) -> Result<Option<WhereClause>, StorageError> {
        let sql_upper = sql.to_uppercase();
        
        if let Some(where_pos) = sql_upper.find("WHERE") {
            let after_where = &sql[where_pos + 5..].trim();
            
            // Find the end of WHERE clause (ORDER BY, LIMIT, or end of string)
            let end_pos = after_where
                .to_uppercase()
                .find("ORDER")
                .or_else(|| after_where.to_uppercase().find("LIMIT"))
                .unwrap_or(after_where.len());
            
            let where_str = after_where[..end_pos].trim();
            
            // Parse conditions (simple implementation - only AND conditions)
            let conditions = Self::parse_where_conditions(where_str)?;
            
            Ok(Some(WhereClause { conditions }))
        } else {
            Ok(None)
        }
    }
    
    /// Parse WHERE conditions
    fn parse_where_conditions(where_str: &str) -> Result<Vec<Condition>, StorageError> {
        let mut conditions = Vec::new();
        
        // Split by AND (simple implementation)
        for condition_str in where_str.split(" AND ") {
            let condition_str = condition_str.trim();
            
            // Parse condition: column operator value
            let operators = [">=", "<=", "!=", "=", ">", "<"];
            
            for op in &operators {
                if let Some(op_pos) = condition_str.find(op) {
                    let column = condition_str[..op_pos].trim().to_string();
                    let value = condition_str[op_pos + op.len()..].trim().to_string();
                    
                    // Remove quotes from value if present
                    let value = if (value.starts_with('\'') && value.ends_with('\''))
                        || (value.starts_with('"') && value.ends_with('"'))
                    {
                        value[1..value.len() - 1].to_string()
                    } else {
                        value
                    };
                    
                    conditions.push(Condition {
                        column,
                        operator: op.to_string(),
                        value,
                    });
                    
                    break;
                }
            }
        }
        
        Ok(conditions)
    }
    
    /// Extract ORDER BY clause
    fn extract_order_by(sql: &str) -> Result<Option<OrderBy>, StorageError> {
        let sql_upper = sql.to_uppercase();
        
        if let Some(order_pos) = sql_upper.find("ORDER BY") {
            let after_order = &sql[order_pos + 8..].trim();
            
            // Find the end of ORDER BY clause (LIMIT or end of string)
            let end_pos = after_order
                .to_uppercase()
                .find("LIMIT")
                .unwrap_or(after_order.len());
            
            let order_str = after_order[..end_pos].trim();
            
            // Parse column and direction
            let parts: Vec<&str> = order_str.split_whitespace().collect();
            
            if parts.is_empty() {
                return Err(StorageError::InvalidQuery(
                    "Invalid ORDER BY clause".to_string(),
                ));
            }
            
            let column = parts[0].to_string();
            let direction = if parts.len() > 1 {
                parts[1].to_uppercase()
            } else {
                "ASC".to_string()
            };
            
            // Validate direction
            if direction != "ASC" && direction != "DESC" {
                return Err(StorageError::InvalidQuery(format!(
                    "Invalid ORDER BY direction: {}",
                    direction
                )));
            }
            
            Ok(Some(OrderBy { column, direction }))
        } else {
            Ok(None)
        }
    }
    
    /// Extract LIMIT clause
    fn extract_limit(sql: &str) -> Result<Option<usize>, StorageError> {
        let sql_upper = sql.to_uppercase();
        
        if let Some(limit_pos) = sql_upper.find("LIMIT") {
            let after_limit = &sql[limit_pos + 5..].trim();
            
            // Extract the number
            let limit_str: String = after_limit
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            
            if limit_str.is_empty() {
                return Err(StorageError::InvalidQuery(
                    "Invalid LIMIT value".to_string(),
                ));
            }
            
            match limit_str.parse::<usize>() {
                Ok(limit) => Ok(Some(limit)),
                Err(_) => Err(StorageError::InvalidQuery(
                    "Invalid LIMIT value".to_string(),
                )),
            }
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_insert_basic() {
        let sql = "INSERT INTO messages (conversation_id, from, content) VALUES ('conv_123', 'alice', 'Hello')";
        let result = SqlParser::parse(sql).unwrap();
        
        match result {
            SqlStatement::Insert(stmt) => {
                assert_eq!(stmt.table, "messages");
                assert_eq!(stmt.columns, vec!["conversation_id", "from", "content"]);
                assert_eq!(stmt.values, vec!["conv_123", "alice", "Hello"]);
            }
            _ => panic!("Expected INSERT statement"),
        }
    }
    
    #[test]
    fn test_parse_select_basic() {
        let sql = "SELECT * FROM messages WHERE conversation_id = 'conv_123'";
        let result = SqlParser::parse(sql).unwrap();
        
        match result {
            SqlStatement::Select(stmt) => {
                assert_eq!(stmt.table, "messages");
                assert_eq!(stmt.columns, vec!["*"]);
                assert!(stmt.where_clause.is_some());
            }
            _ => panic!("Expected SELECT statement"),
        }
    }
    
    #[test]
    fn test_parse_select_with_limit() {
        let sql = "SELECT * FROM messages LIMIT 50";
        let result = SqlParser::parse(sql).unwrap();
        
        match result {
            SqlStatement::Select(stmt) => {
                assert_eq!(stmt.limit, Some(50));
            }
            _ => panic!("Expected SELECT statement"),
        }
    }
    
    #[test]
    fn test_parse_select_with_order_by() {
        let sql = "SELECT * FROM messages ORDER BY timestamp DESC";
        let result = SqlParser::parse(sql).unwrap();
        
        match result {
            SqlStatement::Select(stmt) => {
                assert!(stmt.order_by.is_some());
                let order_by = stmt.order_by.unwrap();
                assert_eq!(order_by.column, "timestamp");
                assert_eq!(order_by.direction, "DESC");
            }
            _ => panic!("Expected SELECT statement"),
        }
    }
    
    #[test]
    fn test_parse_invalid_statement() {
        let sql = "DELETE FROM messages";
        let result = SqlParser::parse(sql);
        assert!(result.is_err());
    }
    
    #[test]
    fn test_parse_insert_column_value_mismatch() {
        let sql = "INSERT INTO messages (col1, col2) VALUES ('val1')";
        let result = SqlParser::parse(sql);
        assert!(result.is_err());
    }
}
