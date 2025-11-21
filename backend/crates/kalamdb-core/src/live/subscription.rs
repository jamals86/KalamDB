//! Subscription service for live queries
//!
//! Handles registration and unregistration of live query subscriptions,
//! including permission checks, filter compilation, and system table updates.

use super::connection_registry::{
    ConnectionId, LiveId, LiveQueryOptions, LiveQueryRegistry, NodeId,
};
use super::filter::FilterCache;
use super::query_parser::QueryParser;
use crate::error::KalamDbError;
use kalamdb_commons::models::{NamespaceId, TableId, TableName, UserId};
use kalamdb_commons::schemas::TableType;
use kalamdb_commons::system::LiveQuery as SystemLiveQuery;
use kalamdb_commons::LiveQueryId;
use kalamdb_system::LiveQueriesTableProvider;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Service for managing subscriptions
pub struct SubscriptionService {
    registry: Arc<tokio::sync::RwLock<LiveQueryRegistry>>,
    filter_cache: Arc<tokio::sync::RwLock<FilterCache>>,
    live_queries_provider: Arc<LiveQueriesTableProvider>,
    schema_registry: Arc<crate::schema_registry::SchemaRegistry>,
    node_id: NodeId,
}

impl SubscriptionService {
    pub fn new(
        registry: Arc<tokio::sync::RwLock<LiveQueryRegistry>>,
        filter_cache: Arc<tokio::sync::RwLock<FilterCache>>,
        live_queries_provider: Arc<LiveQueriesTableProvider>,
        schema_registry: Arc<crate::schema_registry::SchemaRegistry>,
        node_id: NodeId,
    ) -> Self {
        Self {
            registry,
            filter_cache,
            live_queries_provider,
            schema_registry,
            node_id,
        }
    }

    /// Get current timestamp in milliseconds
    fn current_timestamp_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
    }

    /// Register a live query subscription
    pub async fn register_subscription(
        &self,
        connection_id: ConnectionId,
        query_id: String,
        query: String,
        options: LiveQueryOptions,
    ) -> Result<LiveId, KalamDbError> {
        // Parse SQL to extract table reference and WHERE clause
        let raw_table = QueryParser::extract_table_name(&query)?;
        let (namespace, table) = raw_table.split_once('.').ok_or_else(|| {
            KalamDbError::InvalidSql(format!(
                "Query must reference table as namespace.table: {}",
                raw_table
            ))
        })?;

        let namespace_id = NamespaceId::from(namespace);
        let table_name = TableName::from(table);
        let table_id = TableId::new(namespace_id.clone(), table_name.clone());
        let table_def = self
            .schema_registry
            .get_table_definition(&table_id)?
            .ok_or_else(|| {
                KalamDbError::NotFound(format!(
                    "Table {}.{} not found for subscription",
                    namespace, table
                ))
            })?;

        if table_def.table_type == TableType::Shared {
            return Err(KalamDbError::InvalidOperation(
                "Shared table subscriptions are not supported".to_string(),
            ));
        }

        // Security Check: Enforce table access permissions
        let user_id = connection_id.user_id();
        let is_admin = user_id.is_admin();

        match table_def.table_type {
            TableType::User => {
                if !is_admin && namespace != user_id.as_str() {
                    return Err(KalamDbError::Unauthorized(format!(
                        "Insufficient privileges to subscribe to user table '{}.{}'",
                        namespace, table
                    )));
                }
            }
            TableType::System => {
                if !is_admin {
                    return Err(KalamDbError::Unauthorized(format!(
                        "Insufficient privileges to subscribe to system table '{}.{}'",
                        namespace, table
                    )));
                }
            }
            _ => {}
        }

        let mut where_clause = QueryParser::extract_where_clause(&query);

        // Generate LiveId
        let live_id = LiveId::new(connection_id.clone(), table_id.clone(), query_id);

        // Auto-inject user_id filter for user tables (row-level security)
        if table_def.table_type == TableType::User {
            let user_id = connection_id.user_id();
            if !is_admin {
                let user_filter = format!("user_id = '{}'", user_id.as_str());
                where_clause = if let Some(existing_clause) = where_clause {
                    Some(format!("{} AND {}", user_filter, existing_clause))
                } else {
                    Some(user_filter)
                };
            }
        }

        // Compile and cache the filter if WHERE clause exists
        if let Some(clause) = where_clause {
            let resolved_clause = QueryParser::resolve_where_clause_placeholders(
                &clause,
                &UserId::new(connection_id.user_id().to_string()),
            );
            let mut filter_cache = self.filter_cache.write().await;
            filter_cache.insert(live_id.to_string(), &resolved_clause)?;
        }

        let timestamp = Self::current_timestamp_ms();

        // Serialize options to JSON
        let options_json = serde_json::to_string(&options).map_err(|e| {
            KalamDbError::SerializationError(format!("Failed to serialize options: {}", e))
        })?;

        // Create record for system.live_queries
        let live_query_record = SystemLiveQuery {
            live_id: LiveQueryId::new(live_id.to_string()),
            connection_id: connection_id.to_string(),
            namespace_id: table_id.namespace_id().clone(),
            table_name: table_id.table_name().clone(),
            query_id: live_id.query_id().to_string(),
            user_id: kalamdb_commons::UserId::new(connection_id.user_id().to_string()),
            query: query.clone(),
            options: Some(options_json),
            created_at: timestamp,
            last_update: timestamp,
            changes: 0,
            node: self.node_id.as_str().to_string(),
        };

        // Insert into system.live_queries
        self.live_queries_provider
            .insert_live_query(live_query_record)?;

        // Add to in-memory registry
        let user_id = UserId::new(connection_id.user_id().to_string());

        // Register subscription in in-memory registry
        let registry = self.registry.read().await;
        registry.register_subscription(
            user_id.clone(),
            table_id.clone(),
            live_id.clone(),
            connection_id.clone(),
            options,
        )?;

        Ok(live_id)
    }

    /// Unregister a single live query subscription
    pub async fn unregister_subscription(&self, live_id: &LiveId) -> Result<(), KalamDbError> {
        eprintln!(
            "[SubscriptionService] unregister_subscription start: {}",
            live_id
        );
        // Remove cached filter first
        {
            let mut filter_cache = self.filter_cache.write().await;
            filter_cache.remove(&live_id.to_string());
        }
        eprintln!("[SubscriptionService] filter removed: {}", live_id);

        // Remove from in-memory registry
        let connection_id = {
            let registry = self.registry.write().await;
            registry.unregister_subscription(live_id)
        };
        eprintln!(
            "[SubscriptionService] registry.unregister_subscription done: {:?}",
            connection_id.is_some()
        );

        if connection_id.is_none() {
            return Err(KalamDbError::NotFound(format!(
                "Live query not found: {}",
                live_id
            )));
        }

        eprintln!(
            "[SubscriptionService] unregister_subscription end: {}",
            live_id
        );
        Ok(())
    }

    /// Unregister a WebSocket connection
    pub async fn unregister_connection(
        &self,
        _user_id: &UserId,
        connection_id: &ConnectionId,
    ) -> Result<Vec<LiveId>, KalamDbError> {
        // Remove from in-memory registry and get all live_ids
        let live_ids = {
            let registry = self.registry.read().await;
            registry.unregister_connection(connection_id)
        };

        // Remove cached filters for all live queries
        {
            let mut filter_cache = self.filter_cache.write().await;
            for live_id in &live_ids {
                filter_cache.remove(&live_id.to_string());
            }
        }

        // Delete from system.live_queries
        self.live_queries_provider
            .delete_by_connection_id(&connection_id.to_string())?;

        Ok(live_ids)
    }
}
