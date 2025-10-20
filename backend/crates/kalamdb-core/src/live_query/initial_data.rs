// backend/crates/kalamdb-core/src/live_query/initial_data.rs
//
// Initial data fetch for live query subscriptions.
// Provides "changes since timestamp" functionality to populate client state
// before real-time notifications begin.

use crate::catalog::TableType;
use crate::error::KalamDbError;
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
    // TODO: Add DataFusion context for SQL queries in T173
}

impl InitialDataFetcher {
    /// Create a new initial data fetcher
    pub fn new() -> Self {
        Self {}
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
        table_name: &str,
        table_type: TableType,
        options: InitialDataOptions,
    ) -> Result<InitialDataResult, KalamDbError> {
        // TODO: Implement in T173
        // This will use DataFusion to execute a SQL query like:
        // SELECT * FROM {table_name}
        // WHERE _updated >= {since_timestamp}
        //   AND (_deleted = false OR {include_deleted})
        // ORDER BY _updated DESC
        // LIMIT {limit + 1}  -- +1 to detect has_more
        
        log::warn!(
            "Initial data fetch not yet implemented for table: {} (type: {:?})",
            table_name,
            table_type
        );
        
        Ok(InitialDataResult {
            rows: vec![],
            latest_timestamp: None,
            total_available: 0,
            has_more: false,
        })
    }
    
    /// Parse table name into components
    ///
    /// # Arguments
    /// * `table_name` - Fully qualified table name
    /// * `table_type` - User or Shared table
    ///
    /// # Returns
    /// (user_id, namespace_id, table_name) for User tables
    /// (namespace_id, table_name) for Shared tables
    fn parse_table_name(
        &self,
        table_name: &str,
        table_type: TableType,
    ) -> Result<(Option<String>, String, String), KalamDbError> {
        let parts: Vec<&str> = table_name.split('.').collect();
        
        match table_type {
            TableType::User => {
                if parts.len() != 3 {
                    return Err(KalamDbError::Other(format!(
                        "Invalid user table name format: {} (expected user_id.namespace.table)",
                        table_name
                    )));
                }
                Ok((
                    Some(parts[0].to_string()),
                    parts[1].to_string(),
                    parts[2].to_string(),
                ))
            }
            TableType::Shared => {
                if parts.len() != 2 {
                    return Err(KalamDbError::Other(format!(
                        "Invalid shared table name format: {} (expected namespace.table)",
                        table_name
                    )));
                }
                Ok((None, parts[0].to_string(), parts[1].to_string()))
            }
            TableType::System | TableType::Stream => {
                // System and Stream tables don't support live queries
                Err(KalamDbError::Other(format!(
                    "Table type {:?} does not support live queries",
                    table_type
                )))
            }
        }
    }
}

impl Default for InitialDataFetcher {
    fn default() -> Self {
        Self::new()
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
        assert_eq!(options.include_deleted, false);
    }

    #[test]
    fn test_initial_data_options_since() {
        let options = InitialDataOptions::since(1729468800000);
        assert_eq!(options.since_timestamp, Some(1729468800000));
        assert_eq!(options.limit, 100);
        assert_eq!(options.include_deleted, false);
    }

    #[test]
    fn test_initial_data_options_last() {
        let options = InitialDataOptions::last(50);
        assert_eq!(options.since_timestamp, None);
        assert_eq!(options.limit, 50);
        assert_eq!(options.include_deleted, false);
    }

    #[test]
    fn test_initial_data_options_builder() {
        let options = InitialDataOptions::since(1729468800000)
            .with_limit(200)
            .with_deleted();
        
        assert_eq!(options.since_timestamp, Some(1729468800000));
        assert_eq!(options.limit, 200);
        assert_eq!(options.include_deleted, true);
    }

    #[test]
    fn test_parse_user_table_name() {
        let fetcher = InitialDataFetcher::new();
        let result = fetcher.parse_table_name("user123.messages.chat", TableType::User);
        
        assert!(result.is_ok());
        let (user_id, namespace, table) = result.unwrap();
        assert_eq!(user_id, Some("user123".to_string()));
        assert_eq!(namespace, "messages");
        assert_eq!(table, "chat");
    }

    #[test]
    fn test_parse_shared_table_name() {
        let fetcher = InitialDataFetcher::new();
        let result = fetcher.parse_table_name("public.announcements", TableType::Shared);
        
        assert!(result.is_ok());
        let (user_id, namespace, table) = result.unwrap();
        assert_eq!(user_id, None);
        assert_eq!(namespace, "public");
        assert_eq!(table, "announcements");
    }

    #[test]
    fn test_parse_invalid_user_table_name() {
        let fetcher = InitialDataFetcher::new();
        let result = fetcher.parse_table_name("messages.chat", TableType::User);
        
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_invalid_shared_table_name() {
        let fetcher = InitialDataFetcher::new();
        let result = fetcher.parse_table_name("user123.public.announcements", TableType::Shared);
        
        assert!(result.is_err());
    }
}
