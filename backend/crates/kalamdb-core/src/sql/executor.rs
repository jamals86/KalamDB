// SQL executor for executing parsed SQL statements
use crate::error::StorageError;
use crate::ids::SnowflakeGenerator;
use crate::models::Message;
use crate::sql::parser::{SqlStatement, InsertStatement, SelectStatement};
use crate::storage::MessageStore;
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;

/// Result of executing a SQL statement
#[derive(Debug, Clone)]
pub enum SqlResult {
    Insert {
        rows_affected: usize,
        inserted_id: i64,
    },
    Select {
        columns: Vec<String>,
        rows: Vec<Vec<JsonValue>>,
        row_count: usize,
    },
}

/// SQL executor
pub struct SqlExecutor {
    store: Arc<dyn MessageStore + Send + Sync>,
    id_generator: Arc<SnowflakeGenerator>,
    max_message_size: usize,
}

impl SqlExecutor {
    /// Create a new SQL executor
    pub fn new(
        store: Arc<dyn MessageStore + Send + Sync>,
        id_generator: Arc<SnowflakeGenerator>,
        max_message_size: usize,
    ) -> Self {
        Self {
            store,
            id_generator,
            max_message_size,
        }
    }
    
    /// Execute a parsed SQL statement
    pub fn execute(&self, statement: SqlStatement) -> Result<SqlResult, StorageError> {
        match statement {
            SqlStatement::Insert(stmt) => self.execute_insert(stmt),
            SqlStatement::Select(stmt) => self.execute_select(stmt),
        }
    }
    
    /// Execute an INSERT statement
    fn execute_insert(&self, stmt: InsertStatement) -> Result<SqlResult, StorageError> {
        // Validate table name
        if stmt.table != "messages" {
            return Err(StorageError::InvalidQuery(format!(
                "Unknown table: {}. Only 'messages' table is supported",
                stmt.table
            )));
        }
        
        // Create a map of column -> value
        let mut values_map = std::collections::HashMap::new();
        for (col, val) in stmt.columns.iter().zip(stmt.values.iter()) {
            values_map.insert(col.as_str(), val.as_str());
        }
        
        // Generate message ID (either use provided msg_id or generate new one)
        let msg_id = if let Some(id_str) = values_map.get("msg_id") {
            id_str.parse::<i64>().map_err(|_| {
                StorageError::InvalidQuery(format!("Invalid msg_id: {}", id_str))
            })?
        } else {
            self.id_generator.next_id()?
        };
        
        // Extract required fields
        let conversation_id = values_map
            .get("conversation_id")
            .ok_or_else(|| StorageError::InvalidQuery("conversation_id is required".to_string()))?
            .to_string();
        
        let from = values_map
            .get("from")
            .ok_or_else(|| StorageError::InvalidQuery("from is required".to_string()))?
            .to_string();
        
        let content = values_map
            .get("content")
            .ok_or_else(|| StorageError::InvalidQuery("content is required".to_string()))?
            .to_string();
        
        // Validate message size
        if content.len() > self.max_message_size {
            return Err(StorageError::InvalidQuery(format!(
                "Message content too large: {} bytes (max: {})",
                content.len(),
                self.max_message_size
            )));
        }
        
        // Extract optional timestamp (use provided or generate current)
        let timestamp = if let Some(ts_str) = values_map.get("timestamp") {
            ts_str.parse::<i64>().map_err(|_| {
                StorageError::InvalidQuery(format!("Invalid timestamp: {}", ts_str))
            })?
        } else {
            chrono::Utc::now().timestamp_micros()
        };
        
        // Extract optional metadata
        let metadata = if let Some(meta_str) = values_map.get("metadata") {
            if meta_str.is_empty() || *meta_str == "{}" {
                None
            } else {
                let parsed: JsonValue = serde_json::from_str(meta_str).map_err(|e| {
                    StorageError::InvalidQuery(format!("Invalid metadata JSON: {}", e))
                })?;
                Some(parsed)
            }
        } else {
            None
        };
        
        // Create message
        let message = Message::new(
            msg_id,
            conversation_id,
            from,
            timestamp,
            content,
            metadata,
        );
        
        // Validate message
        message.validate(self.max_message_size)?;
        
        // Store message
        self.store.insert_message(&message)?;
        
        Ok(SqlResult::Insert {
            rows_affected: 1,
            inserted_id: msg_id,
        })
    }
    
    /// Execute a SELECT statement
    fn execute_select(&self, stmt: SelectStatement) -> Result<SqlResult, StorageError> {
        // Validate table name
        if stmt.table != "messages" {
            return Err(StorageError::InvalidQuery(format!(
                "Unknown table: {}. Only 'messages' table is supported",
                stmt.table
            )));
        }
        
        // Convert SELECT statement to QueryParams
        let mut conversation_id = None;
        let mut since_msg_id: Option<u64> = None;
        
        if let Some(where_clause) = stmt.where_clause {
            for condition in where_clause.conditions {
                match condition.column.as_str() {
                    "conversation_id" if condition.operator == "=" => {
                        conversation_id = Some(condition.value);
                    }
                    "msg_id" if condition.operator == ">" => {
                        since_msg_id = condition.value.parse::<u64>().ok();
                    }
                    "msg_id" if condition.operator == ">=" => {
                        // For >= we need to subtract 1 since since_msg_id is exclusive
                        if let Ok(id) = condition.value.parse::<u64>() {
                            since_msg_id = Some(id.saturating_sub(1));
                        }
                    }
                    _ => {
                        // Ignore unsupported conditions for now
                    }
                }
            }
        }
        
        // Determine order
        let order = stmt.order_by.as_ref().map(|ob| ob.direction.to_lowercase());
        
        // Create query params
        let query_params = crate::storage::QueryParams {
            conversation_id,
            since_msg_id,
            limit: stmt.limit,
            order,
        };
        
        // Execute query
        let messages = self.store.query_messages(&query_params)?;
        
        // Convert messages to rows
        let columns = if stmt.columns.contains(&"*".to_string()) {
            vec![
                "msg_id".to_string(),
                "conversation_id".to_string(),
                "from".to_string(),
                "timestamp".to_string(),
                "content".to_string(),
                "metadata".to_string(),
            ]
        } else {
            stmt.columns
        };
        
        let rows: Vec<Vec<JsonValue>> = messages
            .iter()
            .map(|msg| {
                columns
                    .iter()
                    .map(|col| match col.as_str() {
                        "msg_id" => json!(msg.msg_id),
                        "conversation_id" => json!(msg.conversation_id),
                        "from" => json!(msg.from),
                        "timestamp" => json!(msg.timestamp),
                        "content" => json!(msg.content),
                        "metadata" => msg.metadata.clone().unwrap_or(json!(null)),
                        _ => json!(null),
                    })
                    .collect()
            })
            .collect();
        
        let row_count = rows.len();
        
        Ok(SqlResult::Select {
            columns,
            rows,
            row_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sql::parser::SqlParser;
    use crate::storage::RocksDbStore;
    use tempfile::TempDir;
    
    #[test]
    fn test_execute_insert() {
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(RocksDbStore::open(temp_dir.path()).unwrap());
        let id_gen = Arc::new(SnowflakeGenerator::new(1));
        let executor = SqlExecutor::new(store.clone(), id_gen, 1_000_000);
        
        let sql = "INSERT INTO messages (conversation_id, from, content) VALUES ('conv_123', 'alice', 'Hello')";
        let statement = SqlParser::parse(sql).unwrap();
        let result = executor.execute(statement).unwrap();
        
        match result {
            SqlResult::Insert { rows_affected, inserted_id } => {
                assert_eq!(rows_affected, 1);
                assert!(inserted_id > 0);
                
                // Verify message was stored
                let msg = store.get_message(inserted_id).unwrap().expect("Message should exist");
                assert_eq!(msg.conversation_id, "conv_123");
                assert_eq!(msg.from, "alice");
                assert_eq!(msg.content, "Hello");
            }
            _ => panic!("Expected INSERT result"),
        }
    }
    
    #[test]
    fn test_execute_select() {
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(RocksDbStore::open(temp_dir.path()).unwrap());
        let id_gen = Arc::new(SnowflakeGenerator::new(1));
        let executor = SqlExecutor::new(store.clone(), id_gen.clone(), 1_000_000);
        
        // Insert a message first
        let insert_sql = "INSERT INTO messages (conversation_id, from, content) VALUES ('conv_123', 'alice', 'Hello')";
        let insert_stmt = SqlParser::parse(insert_sql).unwrap();
        executor.execute(insert_stmt).unwrap();
        
        // Query the message
        let select_sql = "SELECT * FROM messages WHERE conversation_id = 'conv_123'";
        let select_stmt = SqlParser::parse(select_sql).unwrap();
        let result = executor.execute(select_stmt).unwrap();
        
        match result {
            SqlResult::Select { columns, rows, row_count } => {
                assert_eq!(row_count, 1);
                assert_eq!(columns.len(), 6);
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][1], json!("conv_123")); // conversation_id
                assert_eq!(rows[0][2], json!("alice")); // from
                assert_eq!(rows[0][4], json!("Hello")); // content
            }
            _ => panic!("Expected SELECT result"),
        }
    }
    
    #[test]
    fn test_execute_insert_invalid_table() {
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(RocksDbStore::open(temp_dir.path()).unwrap());
        let id_gen = Arc::new(SnowflakeGenerator::new(1));
        let executor = SqlExecutor::new(store, id_gen, 1_000_000);
        
        let sql = "INSERT INTO users (name) VALUES ('alice')";
        let statement = SqlParser::parse(sql).unwrap();
        let result = executor.execute(statement);
        
        assert!(result.is_err());
    }
    
    #[test]
    fn test_execute_insert_message_too_large() {
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(RocksDbStore::open(temp_dir.path()).unwrap());
        let id_gen = Arc::new(SnowflakeGenerator::new(1));
        let executor = SqlExecutor::new(store, id_gen, 100); // Very small limit
        
        let sql = "INSERT INTO messages (conversation_id, from, content) VALUES ('conv_123', 'alice', 'This message is way too long for the size limit')";
        let statement = SqlParser::parse(sql).unwrap();
        let result = executor.execute(statement);
        
        assert!(result.is_err());
    }
}
