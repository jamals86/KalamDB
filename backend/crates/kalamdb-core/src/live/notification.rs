//! Notification service for live queries
//!
//! Handles dispatching change notifications to subscribed clients,
//! including filtering based on WHERE clauses stored in SubscriptionState.

use super::filter_eval::matches as filter_matches;
use super::connections_manager::ConnectionsManager;
use super::types::{ChangeNotification, ChangeType};
use crate::error::KalamDbError;
use crate::providers::arrow_json_conversion::row_to_json_map;
use datafusion::scalar::ScalarValue;
use kalamdb_commons::models::{LiveQueryId, Row, TableId, UserId};
use kalamdb_system::LiveQueriesTableProvider;
use std::borrow::Cow;
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Apply column projections to a Row, returning only the requested columns.
/// Uses Cow to avoid cloning when no projections are needed (SELECT *).
#[inline]
fn apply_projections<'a>(row: &'a Row, projections: &Option<Arc<Vec<String>>>) -> Cow<'a, Row> {
    match projections {
        None => Cow::Borrowed(row),
        Some(cols) => {
            let filtered_values: BTreeMap<String, ScalarValue> = cols
                .iter()
                .filter_map(|col| row.values.get(col).map(|v| (col.clone(), v.clone())))
                .collect();
            Cow::Owned(Row::new(filtered_values))
        }
    }
}

/// Service for notifying subscribers of changes
///
/// Uses Arc<ConnectionsManager> directly since ConnectionsManager internally
/// uses DashMap for lock-free concurrent access - no RwLock wrapper needed.
pub struct NotificationService {
    /// Manager uses DashMap internally for lock-free access
    registry: Arc<ConnectionsManager>,
    live_queries_provider: Arc<LiveQueriesTableProvider>,
}

impl NotificationService {
    pub fn new(
        registry: Arc<ConnectionsManager>,
        live_queries_provider: Arc<LiveQueriesTableProvider>,
    ) -> Self {
        Self {
            registry,
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
        let has_subscriptions = self.registry.has_subscriptions(&user_id, &table_id);

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

        // Gather subscriptions for the specific user AND admin/system observers
        // DashMap provides lock-free access
        // Collect (live_id, projections, serialization_mode, notification_tx) tuples in a single pass
        let live_ids_to_notify: Vec<(LiveQueryId, Option<Arc<Vec<String>>>, kalamdb_commons::websocket::SerializationMode, _)> = {
            let all_handles = self.registry.get_subscriptions_for_table(user_id, table_id);

            let mut results = Vec::new();
            let mut seen = HashSet::new();

            // Define filtering_row for filter evaluation
            let filtering_row = &change_notification.row_data;

            // Iterate only through subscriptions for this specific table - O(k)
            for handle in all_handles {
                // Skip duplicates
                if !seen.insert(handle.live_id.to_string()) {
                    continue;
                }

                // FLUSH notifications are metadata events (not row-level changes)
                // Skip filter evaluation for FLUSH - notify all subscribers
                if matches!(change_notification.change_type, ChangeType::Flush) {
                    results.push((handle.live_id, handle.projections, handle.serialization_mode, handle.notification_tx));
                    continue;
                }

                // Check filter_expr if one exists (for INSERT/UPDATE/DELETE only)
                // Filter is stored directly in SubscriptionState - no cache lookup needed
                if let Some(ref filter_expr) = handle.filter_expr {
                    // Apply filter to row data
                    match filter_matches(filter_expr, filtering_row) {
                        Ok(true) => {
                            results.push((handle.live_id, handle.projections, handle.serialization_mode, handle.notification_tx));
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
                    results.push((handle.live_id, handle.projections, handle.serialization_mode, handle.notification_tx));
                }
            }

            results
        };

        let notification_count = live_ids_to_notify.len();

        // Send notifications and increment changes
        for (live_id, projections, serialization_mode, tx) in live_ids_to_notify {
            // Apply projections to row data (filters columns based on subscription)
            // Uses Cow to avoid cloning when no projections (SELECT *)
            let projected_row_data = apply_projections(&change_notification.row_data, &projections);
            let projected_old_data = change_notification
                .old_data
                .as_ref()
                .map(|old| apply_projections(old, &projections));

            // Convert Row to HashMap<String, JsonValue> using the subscription's serialization mode
            // This ensures consistency between SQL API and WebSocket subscriptions
            let row_json = match row_to_json_map(&projected_row_data, serialization_mode) {
                Ok(json) => json,
                Err(e) => {
                    log::error!(
                        "Failed to convert row to JSON for live_id={}: {}",
                        live_id,
                        e
                    );
                    continue;
                }
            };

            let old_json = if let Some(old) = projected_old_data {
                match row_to_json_map(&old, serialization_mode) {
                    Ok(json) => Some(json),
                    Err(e) => {
                        log::error!(
                            "Failed to convert old row to JSON for live_id={}: {}",
                            live_id,
                            e
                        );
                        continue;
                    }
                }
            } else {
                None
            };

            // Build the notification with JSON data
            let notification = match change_notification.change_type {
                ChangeType::Insert => kalamdb_commons::Notification::insert(
                    live_id.to_string(),
                    vec![row_json],
                ),
                ChangeType::Update => kalamdb_commons::Notification::update(
                    live_id.to_string(),
                    vec![row_json],
                    vec![old_json.unwrap_or_else(|| std::collections::HashMap::new())],
                ),
                ChangeType::Delete => kalamdb_commons::Notification::delete(
                    live_id.to_string(),
                    vec![row_json],
                ),
                ChangeType::Flush => kalamdb_commons::Notification::insert(
                    live_id.to_string(),
                    vec![row_json],
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

            self.increment_changes(&live_id).await?;
        }

        Ok(notification_count)
    }
}
