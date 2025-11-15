// backend/crates/kalamdb-live/src/initial_data.rs
//
// Initial data fetch for live query subscriptions.
// Provides "changes since timestamp" functionality to populate client state
// before real-time notifications begin.

use crate::error::KalamDbError;
use super::filter::FilterPredicate;
use crate::schema_registry::TableType;
// Removed unused store imports after provider-based snapshots for streams
use chrono::DateTime;
use kalamdb_commons::models::TableId;
use serde_json::Value as JsonValue;
use std::sync::Arc;

/// Options for fetching initial data when subscribing to a live query
#[derive(Debug, Clone)]
pub struct InitialDataOptions {
    /// Fetch changes since this timestamp (milliseconds since Unix epoch)
    /// If None, returns last N rows instead
    pub since_timestamp: Option<i64>, //TODO: Use SeqId

    /// Maximum number of rows to return
    /// Default: 100
    pub limit: usize,

    /// Include soft-deleted rows (_deleted=true)
    /// Default: false
    pub include_deleted: bool,
}

impl Default for InitialDataOptions {
    fn default() -> Self {
        Self {
            since_timestamp: None,
            limit: 100,
            include_deleted: false,
        }
    }
}

impl InitialDataOptions {
    /// Create options to fetch changes since a specific timestamp
    pub fn since(timestamp_ms: i64) -> Self {
        Self {
            since_timestamp: Some(timestamp_ms),
            limit: 100,
            include_deleted: false,
        }
    }

    /// Create options to fetch the last N rows
    pub fn last(limit: usize) -> Self {
        Self {
            since_timestamp: None,
            limit,
            include_deleted: false,
        }
    }

    /// Set the maximum number of rows to return
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    /// Include soft-deleted rows in the result
    pub fn with_deleted(mut self) -> Self {
        self.include_deleted = true;
        self
    }
}

/// Result of an initial data fetch
#[derive(Debug, Clone)]
pub struct InitialDataResult {
    /// The fetched rows (as JSON objects)
    pub rows: Vec<JsonValue>,

    /// Timestamp of the most recent row in the result
    /// Can be used as the starting point for real-time notifications
    pub latest_timestamp: Option<i64>,  //TODO: Use SeqId

    /// Total number of rows available (may exceed limit)
    pub total_available: usize,

    /// Whether there are more rows beyond the limit
    pub has_more: bool,
}

/// Service for fetching initial data when subscribing to live queries
pub struct InitialDataFetcher {
    backend: Option<Arc<dyn kalamdb_store::StorageBackend>>,
    schema_registry: Arc<crate::schema_registry::SchemaRegistry>,
}

impl InitialDataFetcher {
    /// Create a new initial data fetcher
    pub fn new(backend: Option<Arc<dyn kalamdb_store::StorageBackend>>, schema_registry: Arc<crate::schema_registry::SchemaRegistry>) -> Self {
        Self { backend, schema_registry }
    }

    /// Fetch initial data for a table
    ///
    /// # Arguments
    /// * `table_id` - Table identifier with namespace and table name
    /// * `table_type` - User or Shared table
    /// * `options` - Options for the fetch (timestamp, limit, etc.)
    ///
    /// # Returns
    /// InitialDataResult with rows and metadata
    pub async fn fetch_initial_data(
        &self,
        _live_id: &super::connection_registry::LiveId,
        table_id: &TableId,
        table_type: TableType,
        options: InitialDataOptions,
        filter: Option<Arc<FilterPredicate>>,
    ) -> Result<InitialDataResult, KalamDbError> {
        log::info!(
            "fetch_initial_data called: table={}, type={:?}, limit={}, since={:?}",
            table_id,
            table_type,
            options.limit,
            options.since_timestamp
        );

        let limit = options.limit;
        if limit == 0 {
            log::debug!("Limit is 0, returning empty result");
            return Ok(InitialDataResult {
                rows: Vec::new(),
                latest_timestamp: None,
                total_available: 0,
                has_more: false,
            });
        }

        let since_timestamp = options.since_timestamp;
        let include_deleted = options.include_deleted;

        let mut rows_with_ts: Vec<(i64, JsonValue)> = match table_type {
            TableType::User => {
                // Use the registered provider to avoid creating a fresh store
                let provider = self
                    .schema_registry
                    .get_provider(table_id)
                    .ok_or_else(|| KalamDbError::Other(format!(
                        "Provider not found for user table {}",
                        table_id
                    )))?;

                // Downcast to UserTableProvider
                if let Some(user_provider) = provider.as_any().downcast_ref::<crate::providers::UserTableProvider>() {
                    let mut rows = Vec::new();
                    for row_fields in user_provider.snapshot_all_rows_json()? {
                        // Extract timestamp from _updated field or use 0
                        let timestamp = Self::extract_updated_timestamp(&row_fields);
                        
                        // Check if deleted (skip if include_deleted is false)
                        if !include_deleted {
                            if let Some(deleted) = row_fields.get("_deleted").and_then(|v| v.as_bool()) {
                                if deleted {
                                    continue;
                                }
                            }
                        }

                        if let Some(since) = since_timestamp {
                            if timestamp < since {
                                continue;
                            }
                        }

                        if let Some(predicate) = filter.as_ref() {
                            if !predicate
                                .matches(&row_fields)
                                .map_err(|e| KalamDbError::Other(e.to_string()))?
                            {
                                continue;
                            }
                        }

                        rows.push((timestamp, row_fields));
                    }
                    rows
                } else {
                    return Err(KalamDbError::Other("Cached provider type mismatch for user table".to_string()));
                }
            }
            TableType::Stream => {
                // Use the registered provider to avoid creating a fresh in-memory store
                let provider = self
                    .schema_registry
                    .get_provider(table_id)
                    .ok_or_else(|| KalamDbError::Other(format!(
                        "Provider not found for stream table {}",
                        table_id
                    )))?;

                // Downcast to StreamTableProvider
                if let Some(stream_provider) = provider.as_any().downcast_ref::<crate::providers::StreamTableProvider>() {
                    let mut rows = Vec::new();
                    for row_fields in stream_provider.snapshot_all_rows_json()? {
                        let timestamp = Self::extract_updated_timestamp(&row_fields);

                        if let Some(since) = since_timestamp {
                            if timestamp < since {
                                continue;
                            }
                        }

                        if let Some(predicate) = filter.as_ref() {
                            if !predicate
                                .matches(&row_fields)
                                .map_err(|e| KalamDbError::Other(e.to_string()))?
                            {
                                continue;
                            }
                        }

                        rows.push((timestamp, row_fields));
                    }
                    rows
                } else {
                    return Err(KalamDbError::Other("Cached provider type mismatch for stream table".to_string()));
                }
            }
            TableType::Shared | TableType::System => {
                return Err(KalamDbError::InvalidOperation(format!(
                    "Initial data fetch is not supported for {:?} tables",
                    table_type
                )));
            }
        };

        rows_with_ts.sort_by(|a, b| b.0.cmp(&a.0));

        let total_available = rows_with_ts.len();
        let has_more = total_available > limit;
        if has_more {
            rows_with_ts.truncate(limit);
        }

        let latest_timestamp = rows_with_ts.first().map(|(ts, _)| *ts);
        let rows = rows_with_ts.into_iter().map(|(_, row)| row).collect();

        log::info!(
            "fetch_initial_data complete: table={}, returned {} rows (total available: {}, has_more: {})",
            table_id,
            total_available.min(limit),
            total_available,
            has_more
        );

        Ok(InitialDataResult {
            rows,
            latest_timestamp,
            total_available,
            has_more,
        })
    }

    /// Parse table name into components (deprecated - use TableId directly)
    ///
    /// # Arguments
    /// * `table_id` - Table identifier with namespace and table name
    /// * `table_type` - User or Shared table
    ///
    /// # Returns
    /// (namespace_id, table_name)
    #[deprecated(note = "Use TableId directly instead of parsing strings")]
    fn parse_table_name(
        &self,
        table_id: &TableId,
        table_type: TableType,
    ) -> Result<(String, String), KalamDbError> {
        match table_type {
            TableType::User | TableType::Shared | TableType::Stream => {
                Ok((table_id.namespace_id().as_str().to_string(), table_id.table_name().as_str().to_string()))
            }
            TableType::System => Err(KalamDbError::Other(
                "System tables do not support live queries".to_string(),
            )),
        }
    }

    fn extract_updated_timestamp(row: &JsonValue) -> i64 {
        row.get(kalamdb_commons::constants::SystemColumnNames::UPDATED)
            .and_then(JsonValue::as_str)
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.timestamp_millis())
            .unwrap_or(0)
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_tables::user_tables::user_table_store::{new_user_table_store, UserTableRow};
    use kalamdb_store::entity_store::EntityStore;
    use kalamdb_commons::UserId;
    use kalamdb_commons::ids::{SeqId, UserTableRowId};
    use kalamdb_commons::models::{NamespaceId, TableName};
    use kalamdb_commons::models::{ConnectionId as ConnId, LiveId as CommonsLiveId};
    use kalamdb_store::test_utils::InMemoryBackend;
    use std::sync::Arc;

    #[test]
    fn test_initial_data_options_default() {
        let options = InitialDataOptions::default();
        assert_eq!(options.since_timestamp, None);
        assert_eq!(options.limit, 100);
        assert!(!options.include_deleted);
    }

    #[test]
    fn test_initial_data_options_since() {
        let options = InitialDataOptions::since(1729468800000);
        assert_eq!(options.since_timestamp, Some(1729468800000));
        assert_eq!(options.limit, 100);
        assert!(!options.include_deleted);
    }

    #[test]
    fn test_initial_data_options_last() {
        let options = InitialDataOptions::last(50);
        assert_eq!(options.since_timestamp, None);
        assert_eq!(options.limit, 50);
        assert!(!options.include_deleted);
    }

    #[test]
    fn test_initial_data_options_builder() {
        let options = InitialDataOptions::since(1729468800000)
            .with_limit(200)
            .with_deleted();

        assert_eq!(options.since_timestamp, Some(1729468800000));
        assert_eq!(options.limit, 200);
        assert!(options.include_deleted);
    }

    #[test]
    fn test_parse_user_table_name() {
        let schema_registry = Arc::new(crate::schema_registry::SchemaRegistry::new(100));
        let fetcher = InitialDataFetcher::new(None, schema_registry);
        let table_id = TableId::from_strings("app", "messages");
        let result = fetcher.parse_table_name(&table_id, TableType::User);

        assert!(result.is_ok());
        let (namespace, table) = result.unwrap();
        assert_eq!(namespace, "app");
        assert_eq!(table, "messages");
    }

    #[test]
    fn test_parse_shared_table_name() {
        let schema_registry = Arc::new(crate::schema_registry::SchemaRegistry::new(100));
        let fetcher = InitialDataFetcher::new(None, schema_registry);
        let table_id = TableId::from_strings("public", "announcements");
        let result = fetcher.parse_table_name(&table_id, TableType::Shared);

        assert!(result.is_ok());
        let (namespace, table) = result.unwrap();
        assert_eq!(namespace, "public");
        assert_eq!(table, "announcements");
    }

    #[tokio::test]
    async fn test_user_table_initial_fetch_returns_rows() {
        // Setup in-memory user table with one row (userA)
        let backend: Arc<dyn kalamdb_store::StorageBackend> = Arc::new(InMemoryBackend::new());
        let ns = NamespaceId::new("batch_test");
        let tbl = TableName::new("items");
        let table_id = kalamdb_commons::models::TableId::new(ns.clone(), tbl.clone());
        let store = Arc::new(new_user_table_store(backend.clone(), &ns, &tbl));

        let user_id = UserId::from("userA");
        let seq = SeqId::new(1234567890);
        let row_id = UserTableRowId::new(user_id.clone(), seq);
        
        let row = UserTableRow {
            user_id: user_id.clone(),
            _seq: seq,
            fields: serde_json::json!({"id": 1, "name": "Item One"}),
            _deleted: false,
        };
        
        // Insert row using EntityStore trait
        EntityStore::put(&*store, &row_id, &row).expect("put row");

        // Build schema registry and register provider
        let schema_registry = Arc::new(crate::schema_registry::SchemaRegistry::new(100));
        
        // Create a minimal AppContext for the test
        use crate::app_context::AppContext;
        use kalamdb_store::test_utils::InMemoryBackend;
        let test_backend: Arc<dyn kalamdb_store::StorageBackend> = Arc::new(InMemoryBackend::new());
        let app_context = Arc::new(AppContext::new_for_test(test_backend.clone(), schema_registry.clone()));
        
        // Create a mock provider with the store
        use crate::providers::UserTableProvider;
        use crate::providers::base::TableProviderCore;
        let core = Arc::new(TableProviderCore::from_app_context(&app_context));
        let provider = Arc::new(UserTableProvider::new(
            core,
            table_id.clone(),
            store,
            "id".to_string(),
        ));
        
        // Register the provider in schema_registry
        schema_registry.insert_provider(table_id.clone(), provider).expect("register provider");
        
        let fetcher = InitialDataFetcher::new(Some(backend), schema_registry);

        // LiveId for connection user 'root' (distinct from row.user_id)
        let conn = ConnId::new("root".to_string(), "conn1".to_string());
        let live = CommonsLiveId::new(conn, table_id.clone(), "q1".to_string());

        // Fetch initial data (default options: last 100)
        let res = fetcher
            .fetch_initial_data(&live, &table_id, TableType::User, InitialDataOptions::last(100), None)
            .await
            .expect("initial fetch");

        assert_eq!(res.rows.len(), 1, "Expected one row in initial snapshot");
    }
}
