//! Notification service for live queries
//!
//! Handles dispatching change notifications to subscribed clients,
//! including filtering based on WHERE clauses.

use super::filter::FilterCache;
use super::registry::{ConnectionRegistry, NotificationSender};
use super::types::{ChangeNotification, ChangeType};
use crate::error::KalamDbError;
use kalamdb_commons::models::{LiveQueryId, Row, TableId, UserId};
use kalamdb_system::LiveQueriesTableProvider;
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Service for notifying subscribers of changes
///
/// Uses Arc<ConnectionRegistry> directly since ConnectionRegistry internally
/// uses DashMap for lock-free concurrent access - no RwLock wrapper needed.
pub struct NotificationService {
    /// Registry uses DashMap internally for lock-free access
    registry: Arc<ConnectionRegistry>,
    filter_cache: Arc<tokio::sync::RwLock<FilterCache>>,
    live_queries_provider: Arc<LiveQueriesTableProvider>,
}

impl NotificationService {
    pub fn new(
        registry: Arc<ConnectionRegistry>,
        filter_cache: Arc<tokio::sync::RwLock<FilterCache>>,
        live_queries_provider: Arc<LiveQueriesTableProvider>,
    ) -> Self {
        Self {
            registry,
            filter_cache,
            live_queries_provider,
        }
    }

    /// Get current timestamp in milliseconds
    fn current_timestamp_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
    }

    /// Increment the changes counter for a live query
    ///
    /// Uses provider's async method which handles spawn_blocking internally.
    pub async fn increment_changes(&self, live_id: &LiveQueryId) -> Result<(), KalamDbError> {
        let timestamp = Self::current_timestamp_ms();
        let live_id_string = live_id.to_string();

        self.live_queries_provider
            .increment_changes_async(&live_id_string, timestamp)
            .await
            .map_err(|e| KalamDbError::Other(format!("Failed to increment changes: {}", e)))?;

        Ok(())
    }

    /// Notify subscribers about a table change (fire-and-forget async)
    pub fn notify_async(
        self: &Arc<Self>,
        user_id: UserId,
        table_id: TableId,
        notification: ChangeNotification,
    ) {
        // Check if there are any subscriptions for this user_id and table_id
        // DashMap provides lock-free access
        let subscriptions = self.registry.get_subscriptions_for_table(&user_id, &table_id);
        let has_subscriptions = !subscriptions.is_empty();

        if has_subscriptions {
            let service = Arc::clone(self);
            tokio::spawn(async move {
                if let Err(e) = service
                    .notify_table_change(&user_id, &table_id, notification)
                    .await
                {
                    log::warn!(
                        "Failed to notify subscribers for table {}.{}: {}",
                        table_id.namespace_id().as_str(),
                        table_id.table_name().as_str(),
                        e
                    );
                }
            });
        }
    }

    /// Notify live query subscribers of a table change
    pub async fn notify_table_change(
        &self,
        user_id: &UserId,
        table_id: &TableId,
        change_notification: ChangeNotification,
    ) -> Result<usize, KalamDbError> {
        let table_name = format!(
            "{}.{}",
            table_id.namespace_id().as_str(),
            table_id.table_name().as_str()
        );

        log::info!(
            "📢 notify_table_change called for table: '{}', user: '{}', change_type: {:?}",
            table_name,
            user_id.as_str(),
            change_notification.change_type
        );

        // Get filter cache for matching
        let filter_cache = self.filter_cache.read().await;

        // Gather subscriptions for the specific user AND admin/system observers
        // DashMap provides lock-free access
        let live_ids_to_notify: Vec<LiveQueryId> = {
            let mut all_handles = Vec::new();
            // Exact user subscriptions
            all_handles.extend(self.registry.get_subscriptions_for_table(user_id, table_id));
            // Admin-like observers (observe all users)
            // TODO: Not needed!!
            let root = kalamdb_commons::models::UserId::root();
            all_handles.extend(self.registry.get_subscriptions_for_table(&root, table_id));

            let mut ids = Vec::new();
            let mut seen = HashSet::new();

            // Define filtering_row for filter evaluation
            let filtering_row = &change_notification.row_data;

            // Iterate only through subscriptions for this specific table - O(k)
            for handle in all_handles {
                // FLUSH notifications are metadata events (not row-level changes)
                // Skip filter evaluation for FLUSH - notify all subscribers
                if matches!(change_notification.change_type, ChangeType::Flush) {
                    if seen.insert(handle.live_id.to_string()) {
                        ids.push(handle.live_id.clone());
                    }
                    continue;
                }

                // Check filter if one exists (for INSERT/UPDATE/DELETE only)
                if let Some(filter) = filter_cache.get(&handle.live_id.to_string()) {
                    // Apply filter to row data
                    match filter.matches(filtering_row) {
                        Ok(true) => {
                            if seen.insert(handle.live_id.to_string()) {
                                ids.push(handle.live_id.clone());
                            }
                        }
                        Ok(false) => {
                            log::trace!(
                                "Filter didn't match for live_id={}, skipping notification",
                                handle.live_id
                            );
                        }
                        Err(e) => {
                            log::error!(
                                "Filter evaluation error for live_id={}: {}",
                                handle.live_id,
                                e
                            );
                        }
                    }
                } else {
                    // No filter, notify all subscribers
                    if seen.insert(handle.live_id.to_string()) {
                        ids.push(handle.live_id.clone());
                    }
                }
            }

            ids
        }; // registry read lock is dropped here

        // Drop filter cache read lock before acquiring write locks
        drop(filter_cache);

        let row_data = change_notification.row_data.clone();
        let old_data = change_notification.old_data.clone();

        // Now send notifications and increment changes for each live_id
        let notification_count = live_ids_to_notify.len();
        for live_id in live_ids_to_notify.iter() {
            // Send notification to WebSocket client
            if let Some(tx) = self.get_notification_sender(live_id).await {
                // Build the typed notification
                let notification = match change_notification.change_type {
                    ChangeType::Insert => kalamdb_commons::Notification::insert(
                        live_id.to_string(),
                        vec![row_data.clone()],
                    ),
                    ChangeType::Update => kalamdb_commons::Notification::update(
                        live_id.to_string(),
                        vec![row_data.clone()],
                        vec![old_data
                            .clone()
                            .unwrap_or_else(|| Row::new(BTreeMap::new()))],
                    ),
                    ChangeType::Delete => kalamdb_commons::Notification::delete(
                        live_id.to_string(),
                        vec![row_data.clone()],
                    ),
                    ChangeType::Flush => kalamdb_commons::Notification::insert(
                        live_id.to_string(),
                        vec![row_data.clone()],
                    ),
                };

                // Send notification through channel (non-blocking)
                if let Err(e) = tx.send((live_id.clone(), notification)) {
                    log::error!(
                        "Failed to send notification to WebSocket client for live_id={}: {}",
                        live_id,
                        e
                    );
                }
            }

            self.increment_changes(live_id).await?;
        }

        Ok(notification_count)
    }

    /// Get the notification sender for a specific live query
    async fn get_notification_sender(
        &self,
        live_id: &LiveQueryId,
    ) -> Option<NotificationSender> {
        // DashMap provides lock-free access
        self.registry
            .get_notification_sender(&live_id.connection_id)
            .map(|arc| (*arc).clone())
    }
}
