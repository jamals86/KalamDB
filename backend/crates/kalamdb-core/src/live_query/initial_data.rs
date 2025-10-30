// backend/crates/kalamdb-core/src/live_query/initial_data.rs
//
// Initial data fetch for live query subscriptions.
// Provides "changes since timestamp" functionality to populate client state
// before real-time notifications begin.

use crate::catalog::TableType;
use crate::error::KalamDbError;
use crate::live_query::filter::FilterPredicate;
use crate::stores::system_table::{SharedTableStoreExt, UserTableStoreExt};
use crate::tables::{SharedTableStore, StreamTableStore, UserTableStore};
use chrono::DateTime;
use serde_json::Value as JsonValue;
use std::sync::Arc;

/// Options for fetching initial data when subscribing to a live query
#[derive(Debug, Clone)]
pub struct InitialDataOptions {
    /// Fetch changes since this timestamp (milliseconds since Unix epoch)
    /// If None, returns last N rows instead
    pub since_timestamp: Option<i64>,

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
    pub latest_timestamp: Option<i64>,

    /// Total number of rows available (may exceed limit)
    pub total_available: usize,

    /// Whether there are more rows beyond the limit
    pub has_more: bool,
}

/// Service for fetching initial data when subscribing to live queries
pub struct InitialDataFetcher {
    user_table_store: Option<Arc<UserTableStore>>,
    shared_table_store: Option<Arc<SharedTableStore>>,
    stream_table_store: Option<Arc<StreamTableStore>>,
}

impl InitialDataFetcher {
    /// Create a new initial data fetcher
    pub fn new(
        user_table_store: Option<Arc<UserTableStore>>,
        shared_table_store: Option<Arc<SharedTableStore>>,
        stream_table_store: Option<Arc<StreamTableStore>>,
    ) -> Self {
        Self {
            user_table_store,
            shared_table_store,
            stream_table_store,
        }
    }

    /// Fetch initial data for a table
    ///
    /// # Arguments
    /// * `table_name` - Fully qualified table name (e.g., "user123.messages.chat")
    /// * `table_type` - User or Shared table
    /// * `options` - Options for the fetch (timestamp, limit, etc.)
    ///
    /// # Returns
    /// InitialDataResult with rows and metadata
    pub async fn fetch_initial_data(
        &self,
        live_id: &crate::live_query::connection_registry::LiveId,
        table_name: &str,
        table_type: TableType,
        options: InitialDataOptions,
        filter: Option<Arc<FilterPredicate>>,
    ) -> Result<InitialDataResult, KalamDbError> {
        log::info!(
            "fetch_initial_data called: table={}, type={:?}, limit={}, since={:?}",
            table_name,
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

        let (namespace, table) = self.parse_table_name(table_name, table_type)?;
        let since_timestamp = options.since_timestamp;
        let include_deleted = options.include_deleted;

        let mut rows_with_ts: Vec<(i64, JsonValue)> = match table_type {
            TableType::User => {
                let store = self.user_table_store.as_ref().ok_or_else(|| {
                    KalamDbError::InvalidOperation(
                        "UserTableStore not configured for live query initial data".to_string(),
                    )
                })?;

                let user_id = live_id.connection_id().user_id();
                let mut rows = Vec::new();
                for (_row_id, row) in store.scan_user(&namespace, &table, user_id).map_err(|e| {
                    KalamDbError::Other(format!(
                        "Failed to scan user table {}.{}: {}",
                        namespace, table, e
                    ))
                })? {
                    if !include_deleted && row._deleted {
                        continue;
                    }

                    let timestamp = Self::extract_updated_timestamp(&serde_json::to_value(&row).unwrap());
                    if let Some(since) = since_timestamp {
                        if timestamp < since {
                            continue;
                        }
                    }

                    if let Some(predicate) = filter.as_ref() {
                        if !predicate
                            .matches(&serde_json::to_value(&row).unwrap())
                            .map_err(|e| KalamDbError::Other(e.to_string()))?
                        {
                            continue;
                        }
                    }

                    rows.push((timestamp, serde_json::to_value(row).unwrap()));
                }
                rows
            }
            TableType::Stream => {
                let store = self.stream_table_store.as_ref().ok_or_else(|| {
                    KalamDbError::InvalidOperation(
                        "StreamTableStore not configured for live query initial data".to_string(),
                    )
                })?;

                let mut rows = Vec::new();
                for (_row_id, row) in store.scan(&namespace, &table).map_err(|e| {
                    KalamDbError::Other(format!(
                        "Failed to scan stream table {}.{}: {}",
                        namespace, table, e
                    ))
                })? {
                    let timestamp = Self::extract_updated_timestamp(&row.fields);

                    if let Some(since) = since_timestamp {
                        if timestamp < since {
                            continue;
                        }
                    }

                    if let Some(predicate) = filter.as_ref() {
                        if !predicate
                            .matches(&row.fields)
                            .map_err(|e| KalamDbError::Other(e.to_string()))?
                        {
                            continue;
                        }
                    }

                    rows.push((timestamp, row.fields.clone()));
                }
                rows
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
            table_name,
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

    /// Parse table name into components
    ///
    /// # Arguments
    /// * `table_name` - Fully qualified table name
    /// * `table_type` - User or Shared table
    ///
    /// # Returns
    /// (namespace_id, table_name)
    fn parse_table_name(
        &self,
        table_name: &str,
        table_type: TableType,
    ) -> Result<(String, String), KalamDbError> {
        let parts: Vec<&str> = table_name.split('.').collect();

        if parts.len() != 2 {
            return Err(KalamDbError::Other(format!(
                "Invalid table reference '{}', expected namespace.table",
                table_name
            )));
        }

        match table_type {
            TableType::User | TableType::Shared | TableType::Stream => {
                Ok((parts[0].to_string(), parts[1].to_string()))
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

    fn is_deleted(row: &JsonValue) -> bool {
        row.get(kalamdb_commons::constants::SystemColumnNames::DELETED)
            .and_then(JsonValue::as_bool)
            .unwrap_or(false)
    }
}

impl Default for InitialDataFetcher {
    fn default() -> Self {
        Self {
            user_table_store: None,
            shared_table_store: None,
            stream_table_store: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let fetcher = InitialDataFetcher::default();
        let result = fetcher.parse_table_name("app.messages", TableType::User);

        assert!(result.is_ok());
        let (namespace, table) = result.unwrap();
        assert_eq!(namespace, "app");
        assert_eq!(table, "messages");
    }

    #[test]
    fn test_parse_shared_table_name() {
        let fetcher = InitialDataFetcher::default();
        let result = fetcher.parse_table_name("public.announcements", TableType::Shared);

        assert!(result.is_ok());
        let (namespace, table) = result.unwrap();
        assert_eq!(namespace, "public");
        assert_eq!(table, "announcements");
    }

    #[test]
    fn test_parse_invalid_user_table_name() {
        let fetcher = InitialDataFetcher::default();
        let result = fetcher.parse_table_name("invalid.format.table", TableType::User);

        assert!(result.is_err());
    }

    #[test]
    fn test_parse_invalid_shared_table_name() {
        let fetcher = InitialDataFetcher::default();
        let result = fetcher.parse_table_name("announcements", TableType::Shared);

        assert!(result.is_err());
    }
}
