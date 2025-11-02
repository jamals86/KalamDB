//! User table INSERT operations
//!
//! This module handles INSERT operations for user tables with:
//! - UserId-scoped key format: {UserId}:{row_id}
//! - Automatic system column injection (_updated = NOW(), _deleted = false)
//! - DEFAULT function evaluation (T534-T539)
//! - NOT NULL constraint enforcement (T554-T559)
//! - kalamdb-store for RocksDB operations
//! - Data isolation enforcement

use crate::catalog::{NamespaceId, TableName, UserId};
use crate::error::KalamDbError;
use crate::live_query::manager::{ChangeNotification, LiveQueryManager};
use crate::stores::system_table::UserTableStoreExt;
use crate::tables::user_tables::user_table_store::{UserTableRow, UserTableRowId};
use crate::tables::UserTableStore;
use arrow::datatypes::Schema;
use chrono::Utc;
use kalamdb_commons::schemas::ColumnDefault;
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::sync::Arc;

/// User table INSERT handler
///
/// Coordinates INSERT operations for user tables, enforcing:
/// - Data isolation via UserId key prefix
/// - System column injection (_updated, _deleted) - handled by UserTableStore
/// - kalamdb-store for RocksDB writes
/// - Real-time notifications via LiveQueryManager (async fire-and-forget)
pub struct UserTableInsertHandler {
    store: Arc<UserTableStore>,
    live_query_manager: Option<Arc<LiveQueryManager>>,
}

impl UserTableInsertHandler {
    /// Create a new user table INSERT handler
    ///
    /// # Arguments
    /// * `store` - UserTableStore instance for RocksDB operations
    pub fn new(store: Arc<UserTableStore>) -> Self {
        Self {
            store,
            live_query_manager: None,
        }
    }

    /// Set the live query manager for real-time notifications
    ///
    /// # Arguments
    /// * `manager` - LiveQueryManager instance for WebSocket subscriptions
    ///
    /// # Returns
    /// Self with manager configured (builder pattern)
    pub fn with_live_query_manager(mut self, manager: Arc<LiveQueryManager>) -> Self {
        self.live_query_manager = Some(manager);
        self
    }

    /// Insert a single row into a user table
    ///
    /// # Arguments
    /// * `namespace_id` - Namespace containing the table
    /// * `table_name` - Name of the table
    /// * `user_id` - User ID for data isolation
    /// * `row_data` - Row data as JSON object
    ///
    /// # Returns
    /// The generated row ID
    ///
    /// # Key Format
    /// RocksDB key format: `{UserId}:{row_id}` (handled by UserTableStore)
    ///
    /// # System Columns
    /// System columns (_updated, _deleted) are automatically injected by UserTableStore
    pub fn insert_row(
        &self,
        namespace_id: &NamespaceId,
        table_name: &TableName,
        user_id: &UserId,
        row_data: JsonValue,
    ) -> Result<String, KalamDbError> {
        // Validate row data is an object
        if !row_data.is_object() {
            return Err(KalamDbError::InvalidOperation(
                "Row data must be a JSON object".to_string(),
            ));
        }

        // Generate row ID (using timestamp + random component for uniqueness)
        let row_id = self.generate_row_id()?;

        // Create the key and entity for storage
        let _key = UserTableRowId::new(user_id.clone(), row_id.clone());
        let entity = UserTableRow {
            row_id: row_id.clone(),
            user_id: user_id.as_str().to_string(),
            fields: row_data.clone(),
            _updated: chrono::Utc::now().to_rfc3339(),
            _deleted: false,
        };

        // Delegate to UserTableStore
        // Store the entity
        UserTableStoreExt::put(
            self.store.as_ref(),
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str(),
            &row_id,
            &entity,
        )
        .map_err(|e| KalamDbError::InvalidOperation(format!("Failed to insert row: {}", e)))?;

        log::debug!(
            "Inserted row into {}.{} for user {} with row_id {}",
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str(),
            row_id
        );

        // ✅ REQUIREMENT 2: Notification AFTER storage success
        // ✅ REQUIREMENT 1 & 3: Async fire-and-forget pattern
        if let Some(manager) = &self.live_query_manager {
            log::info!(
                "🔔 Sending INSERT notification for table {}.{}",
                namespace_id.as_str(),
                table_name.as_str()
            );
            // CRITICAL: Use fully qualified table name (namespace.table_name) for notification matching
            let qualified_table_name = format!("{}.{}", namespace_id.as_str(), table_name.as_str());

            // Add user_id to notification data for filter matching
            let mut notification_data = row_data;
            if let Some(obj) = notification_data.as_object_mut() {
                obj.insert("user_id".to_string(), serde_json::json!(user_id.as_str()));
            }

            let notification =
                ChangeNotification::insert(qualified_table_name.clone(), notification_data);

            let mgr = Arc::clone(manager);
            tokio::spawn(async move {
                // ✅ REQUIREMENT 2: Log errors, don't propagate
                if let Err(e) = mgr
                    .notify_table_change(&qualified_table_name, notification)
                    .await
                {
                    log::warn!("Failed to notify subscribers for INSERT: {}", e);
                } else {
                    log::info!(
                        "✅ INSERT notification sent successfully for table {}",
                        qualified_table_name
                    );
                }
            });
        } else {
            log::warn!(
                "⚠️  No LiveQueryManager configured - INSERT notification skipped for table {}.{}",
                namespace_id.as_str(),
                table_name.as_str()
            );
        }

        Ok(row_id)
    }

    /// Insert multiple rows in batch
    ///
    /// # Arguments
    /// * `namespace_id` - Namespace containing the table
    /// * `table_name` - Name of the table
    /// * `user_id` - User ID for data isolation
    /// * `rows` - Vector of row data as JSON objects
    ///
    /// # Returns
    /// Vector of generated row IDs
    pub fn insert_batch(
        &self,
        namespace_id: &NamespaceId,
        table_name: &TableName,
        user_id: &UserId,
        rows: Vec<JsonValue>,
    ) -> Result<Vec<String>, KalamDbError> {
        log::info!(
            "🔧 UserTableInsertHandler::insert_batch called for table {}.{} with {} rows",
            namespace_id.as_str(),
            table_name.as_str(),
            rows.len()
        );
        log::info!(
            "🔧 LiveQueryManager present: {}",
            self.live_query_manager.is_some()
        );

        let mut row_ids = Vec::with_capacity(rows.len());

        // Insert each row individually (UserTableStore doesn't have batch insert yet)
        for row_data in rows {
            // Validate row data is an object
            if !row_data.is_object() {
                return Err(KalamDbError::InvalidOperation(
                    "All rows must be JSON objects".to_string(),
                ));
            }

            // Generate row ID
            let row_id = self.generate_row_id()?;

            // Create the entity for storage
            let entity = UserTableRow {
                row_id: row_id.clone(),
                user_id: user_id.as_str().to_string(),
                fields: row_data.clone(),
                _updated: chrono::Utc::now().to_rfc3339(),
                _deleted: false,
            };

            // Delegate to UserTableStore
            UserTableStoreExt::put(
                self.store.as_ref(),
                namespace_id.as_str(),
                table_name.as_str(),
                user_id.as_str(),
                &row_id,
                &entity,
            )
            .map_err(|e| KalamDbError::Other(format!("Failed to insert row in batch: {}", e)))?;

            // ✅ REQUIREMENT 6: Notify for EACH row in batch (no message loss)
            // ✅ REQUIREMENT 1 & 3: Async fire-and-forget pattern
            log::info!(
                "🔔 About to send batch INSERT notification for table {}.{}",
                namespace_id.as_str(),
                table_name.as_str()
            );
            if let Some(manager) = &self.live_query_manager {
                log::info!("🔔 Manager found, creating notification...");
                // CRITICAL: Use fully qualified table name (namespace.table_name) for notification matching
                let qualified_table_name =
                    format!("{}.{}", namespace_id.as_str(), table_name.as_str());

                // Add user_id to notification data for filter matching
                let mut notification_data = row_data.clone();
                if let Some(obj) = notification_data.as_object_mut() {
                    obj.insert("user_id".to_string(), serde_json::json!(user_id.as_str()));
                }

                let notification =
                    ChangeNotification::insert(qualified_table_name.clone(), notification_data);

                let mgr = Arc::clone(manager);
                tokio::spawn(async move {
                    log::info!(
                        "🔔 Spawned task, calling notify_table_change for {}...",
                        qualified_table_name
                    );
                    // ✅ REQUIREMENT 2: Log errors, don't propagate
                    if let Err(e) = mgr
                        .notify_table_change(&qualified_table_name, notification)
                        .await
                    {
                        log::warn!("Failed to notify subscribers for batch INSERT: {}", e);
                    } else {
                        log::info!(
                            "✅ Batch INSERT notification sent successfully for table {}",
                            qualified_table_name
                        );
                    }
                });
            } else {
                log::warn!("⚠️  No LiveQueryManager in batch insert!");
            }

            row_ids.push(row_id);
        }

        log::debug!(
            "Inserted {} rows into {}.{} for user {}",
            row_ids.len(),
            namespace_id.as_str(),
            table_name.as_str(),
            user_id.as_str()
        );

        Ok(row_ids)
    }

    /// Generate a unique row ID
    ///
    /// Format: {timestamp_ms}_{random_component}
    ///
    /// This provides time-ordered keys and uniqueness even for concurrent inserts
    fn generate_row_id(&self) -> Result<String, KalamDbError> {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::time::{SystemTime, UNIX_EPOCH};

        // Thread-safe counter for uniqueness within same millisecond
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| KalamDbError::Other(format!("System time error: {}", e)))?
            .as_millis() as u64;
        let counter = COUNTER.fetch_add(1, Ordering::SeqCst);

        // Combine timestamp + counter for uniqueness
        // Format: timestamp_counter (e.g., 1703001234567_42)
        Ok(format!("{}_{}", now_ms, counter))
    }

    /// Apply DEFAULT functions and validate NOT NULL constraints (T534-T539, T554-T559)
    ///
    /// This method:
    /// 1. Detects omitted columns (T535)
    /// 2. Evaluates DEFAULT functions for omitted columns (T536)
    /// 3. Validates NOT NULL constraints (T554-T559)
    ///
    /// # Arguments
    /// * `row_data` - Mutable row data to update with DEFAULT values
    /// * `schema` - Arrow schema with nullable information
    /// * `column_defaults` - Map of column_name -> ColumnDefault
    /// * `user_id` - User ID for CURRENT_USER function
    ///
    /// # Returns
    /// Ok(()) if validation passes and defaults applied
    /// Err if NOT NULL violation or DEFAULT function evaluation fails
    pub fn apply_defaults_and_validate(
        row_data: &mut JsonValue,
        schema: &Schema,
        column_defaults: &HashMap<String, ColumnDefault>,
        user_id: &UserId,
    ) -> Result<(), KalamDbError> {
        // Get the object map (we already validated it's an object in insert_row)
        let row_obj = row_data.as_object_mut().ok_or_else(|| {
            KalamDbError::InvalidOperation("Row data must be an object".to_string())
        })?;

        // Iterate through all columns in schema
        for field in schema.fields() {
            let column_name = field.name();

            // Skip system columns (handled by UserTableStore)
            if column_name.starts_with('_') {
                continue;
            }

            let value_present = row_obj.contains_key(column_name);
            let value_is_null = row_obj.get(column_name).is_some_and(|v| v.is_null());

            // T535: Detect omitted columns and apply DEFAULT
            if !value_present || value_is_null {
                // Column omitted - check if DEFAULT exists
                if let Some(default_spec) = column_defaults.get(column_name) {
                    // T536-T537: Evaluate DEFAULT function
                    let default_value = Self::evaluate_default_function(default_spec, user_id)?;
                    row_obj.insert(column_name.to_string(), default_value);
                    continue;
                } else if !field.is_nullable() {
                    // T554-T557: NOT NULL violation for omitted column without DEFAULT
                    return Err(KalamDbError::InvalidOperation(format!(
                        "NOT NULL violation: column '{}' cannot be null and has no DEFAULT value",
                        column_name
                    )));
                }
            }

            if value_is_null && !field.is_nullable() {
                // T554-T557: NOT NULL violation for explicitly NULL value without DEFAULT
                return Err(KalamDbError::InvalidOperation(format!(
                    "NOT NULL violation: column '{}' cannot be null",
                    column_name
                )));
            }
        }

        Ok(())
    }

    /// Evaluate a DEFAULT function call (T536-T537, T538)
    ///
    /// Supported functions:
    /// - NOW() / CURRENT_TIMESTAMP() - returns current timestamp in ISO 8601 format
    /// - SNOWFLAKE_ID() - returns 64-bit snowflake ID
    /// - UUID_V7() - returns UUID v7 (time-sortable)
    /// - ULID() - returns ULID (26-char URL-safe)
    /// - CURRENT_USER() - returns current user ID
    ///
    /// # Arguments
    /// * `default_spec` - The DEFAULT specification
    /// * `user_id` - User ID for CURRENT_USER function
    ///
    /// # Returns
    /// JSON value with the evaluated result
    fn evaluate_default_function(
        default_spec: &ColumnDefault,
        user_id: &UserId,
    ) -> Result<JsonValue, KalamDbError> {
        match default_spec {
            ColumnDefault::FunctionCall { name: func_name, .. } => {
                let func_upper = func_name.to_uppercase();

                match func_upper.as_str() {
                    "NOW" | "CURRENT_TIMESTAMP" => {
                        // Return current timestamp in ISO 8601 format
                        let now = Utc::now();
                        Ok(json!(now.to_rfc3339()))
                    }
                    "SNOWFLAKE_ID" => {
                        // Generate snowflake ID
                        use crate::ids::SnowflakeGenerator;
                        let generator = SnowflakeGenerator::new(0);
                        let id = generator.next_id().map_err(|e| {
                            KalamDbError::InvalidOperation(format!(
                                "Failed to generate snowflake ID: {}",
                                e
                            ))
                        })?;
                        Ok(json!(id))
                    }
                    "UUID_V7" => {
                        // Generate UUID v7 (time-sortable UUID)
                        use uuid::Uuid;
                        let uuid = Uuid::now_v7();
                        Ok(json!(uuid.to_string()))
                    }
                    "ULID" => {
                        // Generate ULID (26-char URL-safe, time-sortable)
                        use ulid::Ulid;
                        let ulid = Ulid::new();
                        Ok(json!(ulid.to_string()))
                    }
                    "CURRENT_USER" => {
                        // Return current user ID
                        Ok(json!(user_id.as_str()))
                    }
                    _ => {
                        // T539: Unknown function error
                        Err(KalamDbError::InvalidOperation(format!(
                            "Unknown DEFAULT function: {}. Supported functions: NOW, CURRENT_TIMESTAMP, SNOWFLAKE_ID, UUID_V7, ULID, CURRENT_USER",
                            func_name
                        )))
                    }
                }
            }
            ColumnDefault::Literal(literal_value) => {
                // Return literal value as-is (it's already a serde_json::Value)
                Ok(literal_value.clone())
            }
            ColumnDefault::None => {
                // No default - return NULL
                Ok(JsonValue::Null)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tables::user_tables::user_table_store::new_user_table_store;
    use kalamdb_store::test_utils::InMemoryBackend;

    fn setup_test_handler() -> UserTableInsertHandler {
        let backend = Arc::new(InMemoryBackend::new());
        let store = Arc::new(new_user_table_store(
            backend,
            &NamespaceId::new("test_ns"),
            &TableName::new("test_table"),
        ));
        UserTableInsertHandler::new(store)
    }

    #[test]
    fn test_insert_row() {
        let handler = setup_test_handler();

        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());
        let user_id = UserId::new("user123".to_string());

        let row_data = serde_json::json!({
            "name": "Alice",
            "age": 30
        });

        let row_id = handler
            .insert_row(&namespace_id, &table_name, &user_id, row_data)
            .unwrap();

        // Verify row_id format
        assert!(row_id.contains('_'));
    }

    #[test]
    fn test_insert_batch() {
        let handler = setup_test_handler();

        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());
        let user_id = UserId::new("user123".to_string());

        let rows = vec![
            serde_json::json!({"name": "Alice", "age": 30}),
            serde_json::json!({"name": "Bob", "age": 25}),
            serde_json::json!({"name": "Charlie", "age": 35}),
        ];

        let row_ids = handler
            .insert_batch(&namespace_id, &table_name, &user_id, rows)
            .unwrap();

        assert_eq!(row_ids.len(), 3);
        for row_id in row_ids {
            assert!(row_id.contains('_'));
        }
    }

    #[test]
    fn test_data_isolation() {
        let handler = setup_test_handler();

        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());
        let user1 = UserId::new("user1".to_string());
        let user2 = UserId::new("user2".to_string());

        let row_data = serde_json::json!({"name": "Alice"});

        // Insert for user1
        let row_id1 = handler
            .insert_row(&namespace_id, &table_name, &user1, row_data.clone())
            .unwrap();

        // Insert for user2
        let row_id2 = handler
            .insert_row(&namespace_id, &table_name, &user2, row_data.clone())
            .unwrap();

        // Verify keys have different row IDs (user isolation handled by UserTableStore)
        assert!(row_id1 != row_id2);
    }

    #[test]
    fn test_generate_row_id_uniqueness() {
        let handler = setup_test_handler();

        let mut row_ids = std::collections::HashSet::new();

        // Generate 1000 row IDs and verify uniqueness
        for _ in 0..1000 {
            let row_id = handler.generate_row_id().unwrap();
            assert!(
                row_ids.insert(row_id.clone()),
                "Duplicate row ID generated: {}",
                row_id
            );
        }
    }

    #[test]
    fn test_insert_non_object_fails() {
        let handler = setup_test_handler();

        let namespace_id = NamespaceId::new("test_ns".to_string());
        let table_name = TableName::new("test_table".to_string());
        let user_id = UserId::new("user123".to_string());

        // Try to insert a non-object (array)
        let row_data = serde_json::json!(["not", "an", "object"]);

        let result = handler.insert_row(&namespace_id, &table_name, &user_id, row_data);

        assert!(result.is_err());
        match result {
            Err(KalamDbError::InvalidOperation(msg)) => {
                assert!(msg.contains("JSON object"));
            }
            _ => panic!("Expected InvalidOperation error"),
        }
    }

    // T534-T539: DEFAULT function evaluation tests

    #[test]
    fn test_apply_default_now() {
        use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
        use std::collections::HashMap;

        let schema = Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new(
                "created_at",
                DataType::Timestamp(TimeUnit::Millisecond, None),
                false,
            ),
        ]);

        let mut column_defaults = HashMap::new();
        column_defaults.insert(
            "created_at".to_string(),
            ColumnDefault::function("NOW", vec![]),
        );

        let mut row_data = json!({ "id": 123 });
        let user_id = UserId::from("test_user");

        let result = UserTableInsertHandler::apply_defaults_and_validate(
            &mut row_data,
            &schema,
            &column_defaults,
            &user_id,
        );

        assert!(result.is_ok());
        assert!(row_data["created_at"].is_string());
        // Verify it looks like an ISO 8601 timestamp
        let timestamp_str = row_data["created_at"].as_str().unwrap();
        assert!(timestamp_str.contains('T'));
    }

    #[test]
    fn test_apply_default_snowflake_id() {
        use arrow::datatypes::{DataType, Field, Schema};
        use std::collections::HashMap;

        let schema = Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("name", DataType::Utf8, true),
        ]);

        let mut column_defaults = HashMap::new();
        column_defaults.insert(
            "id".to_string(),
            ColumnDefault::function("SNOWFLAKE_ID", vec![]),
        );

        let mut row_data = json!({ "name": "test" });
        let user_id = UserId::from("test_user");

        let result = UserTableInsertHandler::apply_defaults_and_validate(
            &mut row_data,
            &schema,
            &column_defaults,
            &user_id,
        );

        assert!(result.is_ok());
        assert!(row_data["id"].is_number());
        let id = row_data["id"].as_i64().unwrap();
        assert!(id > 0);
    }

    #[test]
    fn test_apply_default_uuid_v7() {
        use arrow::datatypes::{DataType, Field, Schema};
        use std::collections::HashMap;

        let schema = Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("name", DataType::Utf8, true),
        ]);

        let mut column_defaults = HashMap::new();
        column_defaults.insert(
            "id".to_string(),
            ColumnDefault::function("UUID_V7", vec![]),
        );

        let mut row_data = json!({ "name": "test" });
        let user_id = UserId::from("test_user");

        let result = UserTableInsertHandler::apply_defaults_and_validate(
            &mut row_data,
            &schema,
            &column_defaults,
            &user_id,
        );

        assert!(result.is_ok());
        assert!(row_data["id"].is_string());
        let uuid_str = row_data["id"].as_str().unwrap();
        // UUID format: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
        assert_eq!(uuid_str.len(), 36);
        assert_eq!(uuid_str.chars().filter(|&c| c == '-').count(), 4);
    }

    #[test]
    fn test_apply_default_ulid() {
        use arrow::datatypes::{DataType, Field, Schema};
        use std::collections::HashMap;

        let schema = Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("name", DataType::Utf8, true),
        ]);

        let mut column_defaults = HashMap::new();
        column_defaults.insert(
            "id".to_string(),
            ColumnDefault::function("ULID", vec![]),
        );

        let mut row_data = json!({ "name": "test" });
        let user_id = UserId::from("test_user");

        let result = UserTableInsertHandler::apply_defaults_and_validate(
            &mut row_data,
            &schema,
            &column_defaults,
            &user_id,
        );

        assert!(result.is_ok());
        assert!(row_data["id"].is_string());
        let ulid_str = row_data["id"].as_str().unwrap();
        // ULID format: 26 characters (Crockford base32)
        assert_eq!(ulid_str.len(), 26);
    }

    #[test]
    fn test_apply_default_current_user() {
        use arrow::datatypes::{DataType, Field, Schema};
        use std::collections::HashMap;

        let schema = Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("owner", DataType::Utf8, false),
        ]);

        let mut column_defaults = HashMap::new();
        column_defaults.insert(
            "owner".to_string(),
            ColumnDefault::function("CURRENT_USER", vec![]),
        );

        let mut row_data = json!({ "id": 123 });
        let user_id = UserId::from("alice");

        let result = UserTableInsertHandler::apply_defaults_and_validate(
            &mut row_data,
            &schema,
            &column_defaults,
            &user_id,
        );

        assert!(result.is_ok());
        assert_eq!(row_data["owner"].as_str().unwrap(), "alice");
    }

    // T554-T559: NOT NULL constraint tests

    #[test]
    fn test_not_null_violation_omitted_column() {
        use arrow::datatypes::{DataType, Field, Schema};
        use std::collections::HashMap;

        let schema = Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("name", DataType::Utf8, false), // NOT NULL, no DEFAULT
        ]);

        let column_defaults = HashMap::new(); // No DEFAULT for 'name'

        let mut row_data = json!({ "id": 123 }); // 'name' omitted
        let user_id = UserId::from("test_user");

        let result = UserTableInsertHandler::apply_defaults_and_validate(
            &mut row_data,
            &schema,
            &column_defaults,
            &user_id,
        );

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("NOT NULL violation"));
        assert!(err_msg.contains("name"));
    }

    #[test]
    fn test_not_null_violation_explicit_null() {
        use arrow::datatypes::{DataType, Field, Schema};
        use std::collections::HashMap;

        let schema = Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("name", DataType::Utf8, false), // NOT NULL
        ]);

        let column_defaults = HashMap::new();

        let mut row_data = json!({ "id": 123, "name": null }); // Explicit NULL
        let user_id = UserId::from("test_user");

        let result = UserTableInsertHandler::apply_defaults_and_validate(
            &mut row_data,
            &schema,
            &column_defaults,
            &user_id,
        );

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("NOT NULL violation"));
        assert!(err_msg.contains("name"));
    }

    #[test]
    fn test_nullable_column_allows_null() {
        use arrow::datatypes::{DataType, Field, Schema};
        use std::collections::HashMap;

        let schema = Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("description", DataType::Utf8, true), // NULLABLE
        ]);

        let column_defaults = HashMap::new();

        let mut row_data = json!({ "id": 123, "description": null });
        let user_id = UserId::from("test_user");

        let result = UserTableInsertHandler::apply_defaults_and_validate(
            &mut row_data,
            &schema,
            &column_defaults,
            &user_id,
        );

        assert!(result.is_ok()); // Should succeed
    }

    #[test]
    fn test_not_null_with_default_success() {
        use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
        use std::collections::HashMap;

        let schema = Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new(
                "created_at",
                DataType::Timestamp(TimeUnit::Millisecond, None),
                false,
            ), // NOT NULL with DEFAULT
        ]);

        let mut column_defaults = HashMap::new();
        column_defaults.insert(
            "created_at".to_string(),
            ColumnDefault::function("NOW", vec![]),
        );

        let mut row_data = json!({ "id": 123 }); // 'created_at' omitted but has DEFAULT
        let user_id = UserId::from("test_user");

        let result = UserTableInsertHandler::apply_defaults_and_validate(
            &mut row_data,
            &schema,
            &column_defaults,
            &user_id,
        );

        assert!(result.is_ok());
        assert!(row_data["created_at"].is_string());
    }

    #[test]
    fn test_unknown_default_function_error() {
        use arrow::datatypes::{DataType, Field, Schema};
        use std::collections::HashMap;

        let schema = Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("value", DataType::Utf8, false),
        ]);

        let mut column_defaults = HashMap::new();
        column_defaults.insert(
            "value".to_string(),
            ColumnDefault::function("UNKNOWN_FUNC", vec![]),
        );

        let mut row_data = json!({ "id": 123 });
        let user_id = UserId::from("test_user");

        let result = UserTableInsertHandler::apply_defaults_and_validate(
            &mut row_data,
            &schema,
            &column_defaults,
            &user_id,
        );

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Unknown DEFAULT function"));
        assert!(err_msg.contains("UNKNOWN_FUNC"));
    }
}
