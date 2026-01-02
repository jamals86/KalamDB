//! Live query manager core implementation
//!
//! Orchestrates subscription lifecycle, initial data fetching, and notifications.
//! Uses SharedConnectionState pattern for efficient state access.

use crate::error::KalamDbError;
use crate::error_extensions::KalamDbResultExt;
use crate::live::filter_eval::parse_where_clause;
use crate::live::initial_data::{InitialDataFetcher, InitialDataOptions, InitialDataResult};
use crate::live::notification::NotificationService;
use crate::live::query_parser::QueryParser;
use crate::live::connections_manager::{ConnectionsManager, SharedConnectionState};
use crate::live::subscription::SubscriptionService;
use crate::live::types::{ChangeNotification, RegistryStats, SubscriptionResult};
use crate::sql::executor::SqlExecutor;
use datafusion::execution::context::SessionContext;
use datafusion::sql::sqlparser::ast::Expr;
use kalamdb_commons::ids::SeqId;
use kalamdb_commons::models::{ConnectionId, LiveQueryId, TableId, UserId};
use kalamdb_commons::schemas::SchemaField;
use kalamdb_commons::system::LiveQuery as SystemLiveQuery;
use kalamdb_commons::websocket::SubscriptionRequest;
use kalamdb_commons::NodeId;
use kalamdb_system::LiveQueriesTableProvider;
use std::sync::Arc;

/// Live query manager
pub struct LiveQueryManager {
    /// Unified connections manager using DashMap for lock-free concurrent access
    registry: Arc<ConnectionsManager>,
    live_queries_provider: Arc<LiveQueriesTableProvider>,
    initial_data_fetcher: Arc<InitialDataFetcher>,
    schema_registry: Arc<crate::schema_registry::SchemaRegistry>,
    node_id: NodeId,

    // Delegated services
    subscription_service: Arc<SubscriptionService>,
    notification_service: Arc<NotificationService>,
}

impl LiveQueryManager {
    /// Create a new live query manager with an external ConnectionsManager
    ///
    /// The ConnectionsManager is shared across all WebSocket handlers for
    /// centralized connection/subscription management.
    pub fn new(
        live_queries_provider: Arc<LiveQueriesTableProvider>,
        schema_registry: Arc<crate::schema_registry::SchemaRegistry>,
        registry: Arc<ConnectionsManager>,
        base_session_context: Arc<SessionContext>,
    ) -> Self {
        let node_id = registry.node_id().clone();
        let initial_data_fetcher = Arc::new(InitialDataFetcher::new(
            base_session_context,
            schema_registry.clone(),
        ));

        let subscription_service = Arc::new(SubscriptionService::new(
            registry.clone(),
            live_queries_provider.clone(),
            node_id.clone(),
        ));

        let notification_service = Arc::new(NotificationService::new(
            registry.clone(),
            live_queries_provider.clone(),
        ));

        Self {
            registry,
            live_queries_provider,
            initial_data_fetcher,
            schema_registry,
            node_id,
            subscription_service,
            notification_service,
        }
    }

    /// Get the node_id for this manager
    pub fn node_id(&self) -> &NodeId {
        &self.node_id
    }

    /// Provide shared SqlExecutor so initial data fetches reuse common execution path
    pub fn set_sql_executor(&self, executor: Arc<SqlExecutor>) {
        self.initial_data_fetcher.set_sql_executor(executor);
    }

    /// Register a live query subscription
    ///
    /// Parameters:
    /// - connection_state: Shared reference to connection state
    /// - request: Client subscription request
    /// - table_id: Pre-validated table identifier
    /// - filter_expr: Optional parsed WHERE clause
    /// - projections: Optional column projections (None = SELECT *, all columns)
    /// - batch_size: Batch size for initial data loading
    pub async fn register_subscription(
        &self,
        connection_state: &SharedConnectionState,
        request: &SubscriptionRequest,
        table_id: TableId,
        filter_expr: Option<Expr>,
        projections: Option<Vec<String>>,
        batch_size: usize,
    ) -> Result<LiveQueryId, KalamDbError> {
        self.subscription_service
            .register_subscription(connection_state, request, table_id, filter_expr, projections, batch_size)
            .await
    }

    /// Register subscription and fetch initial data (simplified API)
    ///
    /// This method handles all SQL parsing internally:
    /// - Extracts table name from SQL
    /// - Validates table exists
    /// - Checks user permissions
    /// - Parses WHERE clause for filtering
    /// - Extracts column projections
    ///
    /// The ws_handler only needs to validate subscription ID and rate limits.
    pub async fn register_subscription_with_initial_data(
        &self,
        connection_state: &SharedConnectionState,
        request: &SubscriptionRequest,
        initial_data_options: Option<InitialDataOptions>,
    ) -> Result<SubscriptionResult, KalamDbError> {
        // Get user_id from connection state
        let user_id = connection_state.read().user_id()
            .cloned()
            .ok_or_else(|| KalamDbError::InvalidOperation("Connection not authenticated".to_string()))?;

        // Parse table name from SQL
        // TODO: Parse tableid/where/projection all at once using sqlparser instead of writing ad-hoc parsers
        let raw_table = QueryParser::extract_table_name(&request.sql)?;
        let (namespace, table) = raw_table
            .split_once('.')
            .ok_or_else(|| KalamDbError::InvalidSql("Query must use namespace.table format".to_string()))?;

        let table_id = TableId::new(
            kalamdb_commons::models::NamespaceId::from(namespace),
            kalamdb_commons::models::TableName::from(table),
        );

        // Look up table definition
        let table_def = self
            .schema_registry
            .get_table_definition(&table_id)?
            .ok_or_else(|| KalamDbError::NotFound(format!("Table not found: {}", table_id)))?;

        // Permission check
        let is_admin = user_id.is_admin();
        match table_def.table_type {
            kalamdb_commons::TableType::User if !is_admin && namespace != user_id.as_str() => {
                return Err(KalamDbError::PermissionDenied("Insufficient privileges".to_string()));
            }
            kalamdb_commons::TableType::System if !is_admin => {
                return Err(KalamDbError::PermissionDenied("Insufficient privileges for system table".to_string()));
            }
            kalamdb_commons::TableType::Shared => {
                return Err(KalamDbError::InvalidOperation("Shared tables don't support subscriptions".to_string()));
            }
            _ => {}
        }

        // Determine batch size
        let batch_size = request.options.batch_size.unwrap_or(kalamdb_commons::websocket::MAX_ROWS_PER_BATCH);

        // Parse filter expression from WHERE clause (if present)
        let filter_expr: Option<Expr> = QueryParser::extract_where_clause(&request.sql)
            .map(|where_clause| {
                // Resolve placeholders like CURRENT_USER() before parsing
                let resolved = QueryParser::resolve_where_clause_placeholders(&where_clause, &user_id);
                parse_where_clause(&resolved)
            })
            .transpose()
            .map_err(|e| {
                log::warn!("Failed to parse WHERE clause for filter: {}", e);
                e
            })
            .ok()
            .flatten();

        // Extract column projections from SELECT clause (None = SELECT *, all columns)
        let projections = QueryParser::extract_projections(&request.sql);
        if let Some(ref cols) = projections {
            log::info!("Subscription projections: {:?}", cols);
        }

        // Register the subscription
        let live_id = self.register_subscription(
            connection_state,
            request,
            table_id.clone(),
            filter_expr,
            projections.clone(),
            batch_size,
        ).await?;

        // Fetch initial data if requested
        let initial_data = if let Some(fetch_options) = initial_data_options {
            // Extract WHERE clause from SQL for initial data fetch
            let where_clause = QueryParser::extract_where_clause(&request.sql);

            let result = self
                .initial_data_fetcher
                .fetch_initial_data(
                    &live_id,
                    &table_id,
                    table_def.table_type,
                    fetch_options,
                    where_clause.as_deref(),
                    projections.as_deref(),
                )
                .await?;

            // Update snapshot_end_seq in subscription state
            if let Some(snapshot_seq) = result.snapshot_end_seq {
                self.subscription_service.update_snapshot_end_seq(
                    connection_state,
                    &request.id,
                    snapshot_seq,
                );
            }

            Some(result)
        } else {
            None
        };

        // Build schema from table definition, respecting projections if specified
        let schema: Vec<SchemaField> = if let Some(ref proj_cols) = projections {
            // When projections specified, only include those columns in order
            proj_cols
                .iter()
                .enumerate()
                .filter_map(|(idx, col_name)| {
                    table_def.columns.iter().find(|c| c.column_name.eq_ignore_ascii_case(col_name)).map(|col| {
                        SchemaField::new(col.column_name.clone(), col.data_type.clone(), idx)
                    })
                })
                .collect()
        } else {
            // SELECT * - include all columns in ordinal order
            let mut cols: Vec<_> = table_def.columns.iter().collect();
            cols.sort_by_key(|c| c.ordinal_position);
            cols.iter()
                .enumerate()
                .map(|(idx, col)| SchemaField::new(col.column_name.clone(), col.data_type.clone(), idx))
                .collect()
        };

        Ok(SubscriptionResult {
            live_id,
            initial_data,
            schema,
        })
    }

    /// Fetch a batch of initial data for an existing subscription
    ///
    /// Uses subscription metadata from ConnectionState for batch fetching.
    pub async fn fetch_initial_data_batch(
        &self,
        connection_state: &SharedConnectionState,
        subscription_id: &str,
        since_seq: Option<SeqId>,
    ) -> Result<InitialDataResult, KalamDbError> {
        // Get subscription state from connection
        let sub_state = {
            let state = connection_state.read();
            state
                .subscriptions
                .get(subscription_id)
                .map(|s| s.clone())
                .ok_or_else(|| {
                    KalamDbError::NotFound(format!("Subscription not found: {}", subscription_id))
                })?
        };

        let table_def = self
            .schema_registry
            .get_table_definition(&sub_state.table_id)?
            .ok_or_else(|| {
                KalamDbError::NotFound(format!(
                    "Table {} not found for batch fetch",
                    sub_state.table_id
                ))
            })?;

        // Extract WHERE clause from stored SQL
        let where_clause = QueryParser::extract_where_clause(&sub_state.sql);

        // Get projections from subscription state (Arc<Vec<String>> -> Option<&[String]>)
        let projections_ref = sub_state.projections.as_deref().map(|v| v.as_slice());

        // Create batch options with metadata from subscription state
        let fetch_options = InitialDataOptions::batch(
            since_seq,
            sub_state.snapshot_end_seq,
            sub_state.batch_size,
        );

        self.initial_data_fetcher
            .fetch_initial_data(
                &sub_state.live_id,
                &sub_state.table_id,
                table_def.table_type,
                fetch_options,
                where_clause.as_deref(),
                projections_ref,
            )
            .await
    }

    /// Extract table name from SQL query
    pub fn extract_table_name_from_query(&self, query: &str) -> Result<String, KalamDbError> {
        QueryParser::extract_table_name(query)
    }

    /// Unregister a WebSocket connection
    pub async fn unregister_connection(
        &self,
        user_id: &UserId,
        connection_id: &ConnectionId,
    ) -> Result<Vec<LiveQueryId>, KalamDbError> {
        self.subscription_service
            .unregister_connection(user_id, connection_id)
            .await
    }

    /// Unregister a single live query subscription using SharedConnectionState
    pub async fn unregister_subscription(
        &self,
        connection_state: &SharedConnectionState,
        subscription_id: &str,
        live_id: &LiveQueryId,
    ) -> Result<(), KalamDbError> {
        self.subscription_service
            .unregister_subscription(connection_state, subscription_id, live_id)
            .await
    }

    /// Unregister a live query subscription by LiveQueryId
    ///
    /// This is a convenience method that looks up the connection state
    /// from the LiveQueryId. Used by KILL LIVE QUERY command.
    pub async fn unregister_subscription_by_id(
        &self,
        live_id: &LiveQueryId,
    ) -> Result<(), KalamDbError> {
        // Get connection from registry
        let connection_state = self.registry.get_connection(&live_id.connection_id)
            .ok_or_else(|| {
                KalamDbError::NotFound(format!(
                    "Connection not found for live query: {}",
                    live_id
                ))
            })?;

        // Extract subscription_id from live_id
        let subscription_id = live_id.subscription_id();

        self.subscription_service
            .unregister_subscription(&connection_state, subscription_id, live_id)
            .await
    }

    /// Increment the changes counter for a live query
    pub async fn increment_changes(&self, live_id: &LiveQueryId) -> Result<(), KalamDbError> {
        self.notification_service.increment_changes(live_id).await
    }

    /// Get all subscriptions for a user
    pub async fn get_user_subscriptions(
        &self,
        user_id: &UserId,
    ) -> Result<Vec<SystemLiveQuery>, KalamDbError> {
        self.live_queries_provider
            .get_by_user_id_async(user_id)
            .await
            .into_kalamdb_error("Failed to get user subscriptions")
    }

    /// Get a specific live query
    pub async fn get_live_query(
        &self,
        live_id: &str,
    ) -> Result<Option<SystemLiveQuery>, KalamDbError> {
        self.live_queries_provider
            .get_live_query_async(live_id)
            .await
            .into_kalamdb_error("Failed to get live query")
    }

    /// Get registry statistics
    pub async fn get_stats(&self) -> RegistryStats {
        RegistryStats {
            total_connections: self.registry.connection_count(),
            total_subscriptions: self.registry.subscription_count(),
            node_id: self.node_id.as_str().to_string(),
        }
    }

    /// Get the connections manager
    pub fn registry(&self) -> Arc<ConnectionsManager> {
        Arc::clone(&self.registry)
    }

    /// Notify subscribers about a table change (fire-and-forget async)
    pub fn notify_table_change_async(
        self: &Arc<Self>,
        user_id: UserId,
        table_id: TableId,
        notification: ChangeNotification,
    ) {
        self.notification_service
            .notify_async(user_id, table_id, notification)
    }

    /// Notify live query subscribers of a table change
    pub async fn notify_table_change(
        &self,
        user_id: &UserId,
        table_id: &TableId,
        change_notification: ChangeNotification,
    ) -> Result<usize, KalamDbError> {
        self.notification_service
            .notify_table_change(user_id, table_id, change_notification)
            .await
    }

    /// Handle auth expiry for a connection
    pub async fn handle_auth_expiry(&self, connection_id: &ConnectionId) -> Result<(), KalamDbError> {
        let connection_state = self.registry.get_connection(connection_id).ok_or_else(|| {
            KalamDbError::NotFound(format!("Connection not found: {}", connection_id))
        })?;
        
        let user_id = {
            let state = connection_state.read();
            state.user_id.clone().ok_or_else(|| {
                KalamDbError::NotFound(format!("User not found for connection: {}", connection_id))
            })?
        };
        
        let removed_ids = self.unregister_connection(&user_id, connection_id).await?;
        
        for live_id in removed_ids {
            let _ = self.live_queries_provider.delete_live_query_async(&live_id).await;
        }
        
        Ok(())
    }

    // ==================== Backward Compatibility ====================
    // These methods support older interfaces that use connection_id instead of SharedConnectionState

    /// Get the total number of connections
    pub fn total_connections(&self) -> usize {
        self.registry.connection_count()
    }

    /// Get the total number of subscriptions
    pub fn total_subscriptions(&self) -> usize {
        self.registry.subscription_count()
    }
}
