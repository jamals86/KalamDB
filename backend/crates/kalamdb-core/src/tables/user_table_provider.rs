//! User table provider for DataFusion integration
//!
//! This module provides a DataFusion TableProvider implementation for user tables with:
//! - Data isolation via UserId key prefix filtering
//! - Integration with UserTableInsertHandler, UserTableUpdateHandler, UserTableDeleteHandler
//! - Schema management and version tracking
//! - Storage path templating with ${user_id} substitution
//! - Hybrid RocksDB + Parquet querying

use crate::catalog::{NamespaceId, TableMetadata, TableName, TableType, UserId};
use crate::error::KalamDbError;
use crate::tables::user_table_delete::UserTableDeleteHandler;
use crate::tables::user_table_insert::UserTableInsertHandler;
use crate::tables::user_table_update::UserTableUpdateHandler;
use async_trait::async_trait;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::datasource::TableProvider;
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::execution::context::SessionState;
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_store::UserTableStore;
use serde_json::Value as JsonValue;
use std::any::Any;
use std::sync::Arc;

/// User table provider for DataFusion
///
/// Provides SQL query access to user tables with:
/// - Automatic data isolation by UserId
/// - DML operations (INSERT, UPDATE, DELETE)
/// - Hybrid RocksDB + Parquet scanning
/// - Schema evolution support
pub struct UserTableProvider {
    /// Table metadata (namespace, table name, type, storage location, etc.)
    table_metadata: TableMetadata,

    /// Arrow schema for the table
    schema: SchemaRef,

    /// UserTableStore for DML operations
    store: Arc<UserTableStore>,

    /// Current user ID for data isolation
    current_user_id: UserId,

    /// INSERT handler
    insert_handler: Arc<UserTableInsertHandler>,

    /// UPDATE handler
    update_handler: Arc<UserTableUpdateHandler>,

    /// DELETE handler
    delete_handler: Arc<UserTableDeleteHandler>,

    /// Parquet file paths for cold data (optional)
    parquet_paths: Vec<String>,
}

impl UserTableProvider {
    /// Create a new user table provider
    ///
    /// # Arguments
    /// * `table_metadata` - Table metadata (namespace, table name, type, etc.)
    /// * `schema` - Arrow schema for the table
    /// * `store` - UserTableStore for DML operations
    /// * `current_user_id` - Current user ID for data isolation
    /// * `parquet_paths` - Optional list of Parquet file paths for cold data
    pub fn new(
        table_metadata: TableMetadata,
        schema: SchemaRef,
        store: Arc<UserTableStore>,
        current_user_id: UserId,
        parquet_paths: Vec<String>,
    ) -> Self {
        let insert_handler = Arc::new(UserTableInsertHandler::new(store.clone()));
        let update_handler = Arc::new(UserTableUpdateHandler::new(store.clone()));
        let delete_handler = Arc::new(UserTableDeleteHandler::new(store.clone()));

        Self {
            table_metadata,
            schema,
            store,
            current_user_id,
            insert_handler,
            update_handler,
            delete_handler,
            parquet_paths,
        }
    }

    /// Get the column family name for this table
    pub fn column_family_name(&self) -> String {
        self.table_metadata.column_family_name()
    }

    /// Get the namespace ID
    pub fn namespace_id(&self) -> &NamespaceId {
        &self.table_metadata.namespace
    }

    /// Get the table name
    pub fn table_name(&self) -> &TableName {
        &self.table_metadata.table_name
    }

    /// Get the table type
    pub fn table_type(&self) -> &TableType {
        &self.table_metadata.table_type
    }

    /// Get the current user ID
    pub fn current_user_id(&self) -> &UserId {
        &self.current_user_id
    }

    /// Substitute ${user_id} in storage paths with actual user ID
    ///
    /// This implements T127 - user ID path substitution
    ///
    /// # Arguments
    /// * `template` - Storage path template (e.g., "s3://bucket/users/${user_id}/data/")
    ///
    /// # Returns
    /// Storage path with ${user_id} replaced (e.g., "s3://bucket/users/user123/data/")
    pub fn substitute_user_id_in_path(&self, template: &str) -> String {
        template.replace("${user_id}", self.current_user_id.as_str())
    }

    /// Get the storage location for this user
    ///
    /// Applies ${user_id} substitution to the table's storage location
    pub fn user_storage_location(&self) -> String {
        self.substitute_user_id_in_path(&self.table_metadata.storage_location)
    }

    /// Insert a single row into this user table
    ///
    /// # Arguments
    /// * `row_data` - Row data as JSON object
    ///
    /// # Returns
    /// The generated row ID
    pub fn insert_row(&self, row_data: JsonValue) -> Result<String, KalamDbError> {
        self.insert_handler.insert_row(
            self.namespace_id(),
            self.table_name(),
            &self.current_user_id,
            row_data,
        )
    }

    /// Insert multiple rows into this user table
    ///
    /// # Arguments
    /// * `rows` - Vector of row data as JSON objects
    ///
    /// # Returns
    /// Vector of generated row IDs
    pub fn insert_batch(&self, rows: Vec<JsonValue>) -> Result<Vec<String>, KalamDbError> {
        self.insert_handler.insert_batch(
            self.namespace_id(),
            self.table_name(),
            &self.current_user_id,
            rows,
        )
    }

    /// Update a single row in this user table
    ///
    /// # Arguments
    /// * `row_id` - Row ID to update
    /// * `updates` - Updated fields as JSON object
    ///
    /// # Returns
    /// The row ID of the updated row
    pub fn update_row(
        &self,
        row_id: &str,
        updates: JsonValue,
    ) -> Result<String, KalamDbError> {
        self.update_handler.update_row(
            self.namespace_id(),
            self.table_name(),
            &self.current_user_id,
            row_id,
            updates,
        )
    }

    /// Update multiple rows in this user table
    ///
    /// # Arguments
    /// * `updates` - Vector of (row_id, updates) tuples
    ///
    /// # Returns
    /// Vector of updated row IDs
    pub fn update_batch(
        &self,
        updates: Vec<(String, JsonValue)>,
    ) -> Result<Vec<String>, KalamDbError> {
        self.update_handler.update_batch(
            self.namespace_id(),
            self.table_name(),
            &self.current_user_id,
            updates,
        )
    }

    /// Soft delete a single row in this user table
    ///
    /// # Arguments
    /// * `row_id` - Row ID to delete
    ///
    /// # Returns
    /// The row ID of the deleted row
    pub fn delete_row(&self, row_id: &str) -> Result<String, KalamDbError> {
        self.delete_handler.delete_row(
            self.namespace_id(),
            self.table_name(),
            &self.current_user_id,
            row_id,
        )
    }

    /// Soft delete multiple rows in this user table
    ///
    /// # Arguments
    /// * `row_ids` - Vector of row IDs to delete
    ///
    /// # Returns
    /// Vector of deleted row IDs
    pub fn delete_batch(&self, row_ids: Vec<String>) -> Result<Vec<String>, KalamDbError> {
        self.delete_handler.delete_batch(
            self.namespace_id(),
            self.table_name(),
            &self.current_user_id,
            row_ids,
        )
    }

    /// Get the user-specific key prefix for data isolation
    ///
    /// This implements T128 - data isolation enforcement
    ///
    /// All queries will be filtered to only access rows with this prefix,
    /// ensuring users can only see their own data.
    ///
    /// # Returns
    /// Key prefix in format "{UserId}:"
    pub fn user_key_prefix(&self) -> Vec<u8> {
        format!("{}:", self.current_user_id.as_str())
            .as_bytes()
            .to_vec()
    }
}

#[async_trait]
impl TableProvider for UserTableProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn table_type(&self) -> datafusion::datasource::TableType {
        datafusion::datasource::TableType::Base
    }

    async fn scan(
        &self,
        _state: &SessionState,
        _projection: Option<&Vec<usize>>,
        _filters: &[Expr],
        _limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        // TODO: Implement full scan with:
        // 1. RocksDB scan with user_key_prefix() for data isolation (T128)
        // 2. Parquet file scanning from user_storage_location()
        // 3. Merge results from both sources
        // 4. Apply projection, filters, and limit
        //
        // For now, return NotImplemented error
        Err(DataFusionError::NotImplemented(format!(
            "User table scanning not yet implemented for table: {}. Use insert_row(), update_row(), delete_row() methods instead.",
            self.table_name().as_str()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flush::FlushPolicy;
    use chrono::Utc;
    use datafusion::arrow::datatypes::{DataType, Field, Schema};
    use kalamdb_store::test_utils::TestDb;
    use kalamdb_store::UserTableStore;
    use serde_json::json;

    fn create_test_db() -> Arc<UserTableStore> {
        let test_db = TestDb::single_cf("user_table:chat:messages").unwrap();
        Arc::new(UserTableStore::new(test_db.db).unwrap())
    }

    fn create_test_schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("content", DataType::Utf8, true),
            Field::new("_updated", DataType::Int64, false),
            Field::new("_deleted", DataType::Boolean, false),
        ]))
    }

    fn create_test_metadata() -> TableMetadata {
        TableMetadata {
            table_name: TableName::new("messages"),
            table_type: TableType::User,
            namespace: NamespaceId::new("chat"),
            created_at: Utc::now(),
            storage_location: "s3://bucket/users/${user_id}/messages/".to_string(),
            flush_policy: FlushPolicy::row_limit(1000).unwrap(),
            schema_version: 1,
            deleted_retention_hours: Some(720),
        }
    }

    #[test]
    fn test_user_table_provider_creation() {
        let store = create_test_db();
        let schema = create_test_schema();
        let metadata = create_test_metadata();
        let user_id = UserId::new("user123".to_string());

        let provider = UserTableProvider::new(
            metadata.clone(),
            schema.clone(),
            store.clone(),
            user_id.clone(),
            vec![],
        );

        assert_eq!(provider.schema(), schema);
        assert_eq!(provider.namespace_id(), &NamespaceId::new("chat"));
        assert_eq!(provider.table_name(), &TableName::new("messages"));
        assert_eq!(provider.table_type(), &TableType::User);
        assert_eq!(provider.current_user_id(), &user_id);
        assert_eq!(
            provider.column_family_name(),
            "user_table:chat:messages"
        );
    }

    #[test]
    fn test_substitute_user_id_in_path() {
        let store = create_test_db();
        let schema = create_test_schema();
        let metadata = create_test_metadata();
        let user_id = UserId::new("user123".to_string());

        let provider = UserTableProvider::new(
            metadata,
            schema,
            store,
            user_id,
            vec![],
        );

        // Test ${user_id} substitution
        assert_eq!(
            provider.substitute_user_id_in_path("s3://bucket/users/${user_id}/messages/"),
            "s3://bucket/users/user123/messages/"
        );

        // Test user_storage_location()
        assert_eq!(
            provider.user_storage_location(),
            "s3://bucket/users/user123/messages/"
        );

        // Test path without template variable
        assert_eq!(
            provider.substitute_user_id_in_path("/data/messages/"),
            "/data/messages/"
        );
    }

    #[test]
    fn test_user_key_prefix() {
        let store = create_test_db();
        let schema = create_test_schema();
        let metadata = create_test_metadata();
        let user_id = UserId::new("user123".to_string());

        let provider = UserTableProvider::new(
            metadata,
            schema,
            store,
            user_id,
            vec![],
        );

        let prefix = provider.user_key_prefix();
        let expected = b"user123:".to_vec();

        assert_eq!(prefix, expected);
    }

    #[test]
    fn test_insert_row() {
        let store = create_test_db();
        let schema = create_test_schema();
        let metadata = create_test_metadata();
        let user_id = UserId::new("user123".to_string());

        let provider = UserTableProvider::new(
            metadata,
            schema,
            store,
            user_id,
            vec![],
        );

        let row_data = json!({
            "content": "Hello, World!"
        });

        let result = provider.insert_row(row_data);
        assert!(result.is_ok(), "Insert should succeed");
        
        let row_id = result.unwrap();
        assert!(!row_id.is_empty(), "Row ID should be generated");
    }

    #[test]
    fn test_insert_batch() {
        let store = create_test_db();
        let schema = create_test_schema();
        let metadata = create_test_metadata();
        let user_id = UserId::new("user123".to_string());

        let provider = UserTableProvider::new(
            metadata,
            schema,
            store,
            user_id,
            vec![],
        );

        let rows = vec![
            json!({"content": "Message 1"}),
            json!({"content": "Message 2"}),
            json!({"content": "Message 3"}),
        ];

        let result = provider.insert_batch(rows);
        assert!(result.is_ok(), "Batch insert should succeed");
        
        let row_ids = result.unwrap();
        assert_eq!(row_ids.len(), 3, "Should generate 3 row IDs");
    }

    #[test]
    fn test_update_row() {
        let store = create_test_db();
        let schema = create_test_schema();
        let metadata = create_test_metadata();
        let user_id = UserId::new("user123".to_string());

        let provider = UserTableProvider::new(
            metadata,
            schema,
            store,
            user_id,
            vec![],
        );

        // Insert a row first
        let row_data = json!({"content": "Original"});
        let row_id = provider.insert_row(row_data).unwrap();

        // Update the row
        let updates = json!({"content": "Updated"});
        let result = provider.update_row(&row_id, updates);
        assert!(result.is_ok(), "Update should succeed");
        assert_eq!(result.unwrap(), row_id, "Should return the updated row ID");
    }

    #[test]
    fn test_delete_row() {
        let store = create_test_db();
        let schema = create_test_schema();
        let metadata = create_test_metadata();
        let user_id = UserId::new("user123".to_string());

        let provider = UserTableProvider::new(
            metadata,
            schema,
            store,
            user_id,
            vec![],
        );

        // Insert a row first
        let row_data = json!({"content": "To be deleted"});
        let row_id = provider.insert_row(row_data).unwrap();

        // Delete the row (soft delete)
        let result = provider.delete_row(&row_id);
        assert!(result.is_ok(), "Delete should succeed");
        assert_eq!(result.unwrap(), row_id, "Should return the deleted row ID");
    }

    #[test]
    fn test_data_isolation_different_users() {
        let store = create_test_db();
        let schema = create_test_schema();
        let metadata = create_test_metadata();

        let user1_id = UserId::new("user1".to_string());
        let user2_id = UserId::new("user2".to_string());

        let provider1 = UserTableProvider::new(
            metadata.clone(),
            schema.clone(),
            store.clone(),
            user1_id.clone(),
            vec![],
        );

        let provider2 = UserTableProvider::new(
            metadata,
            schema,
            store,
            user2_id.clone(),
            vec![],
        );

        // Insert data for user1
        let row_data_1 = json!({"content": "User1 message"});
        let row_id_1 = provider1.insert_row(row_data_1).unwrap();

        // Insert data for user2
        let row_data_2 = json!({"content": "User2 message"});
        let row_id_2 = provider2.insert_row(row_data_2).unwrap();

        // Verify different key prefixes
        let prefix1 = provider1.user_key_prefix();
        let prefix2 = provider2.user_key_prefix();
        assert_ne!(prefix1, prefix2, "Different users should have different key prefixes");

        assert_eq!(prefix1, b"user1:".to_vec());
        assert_eq!(prefix2, b"user2:".to_vec());

        // Row IDs should be unique
        assert_ne!(row_id_1, row_id_2);
    }
}
