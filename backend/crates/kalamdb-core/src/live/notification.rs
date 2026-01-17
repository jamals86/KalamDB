//! Notification service for live queries
//!
//! Handles dispatching change notifications to subscribed clients,
//! including filtering based on WHERE clauses stored in SubscriptionState.

use super::connections_manager::{ConnectionsManager, SubscriptionHandle};
use super::filter_eval::matches as filter_matches;
use super::types::{ChangeNotification, ChangeType};
use crate::error::KalamDbError;
use crate::error_extensions::KalamDbResultExt;
use crate::providers::arrow_json_conversion::row_to_json_map;
use datafusion::scalar::ScalarValue;
use kalamdb_commons::models::rows::Row;
use kalamdb_commons::models::{LiveQueryId, TableId, UserId};
use kalamdb_system::LiveQueriesTableProvider;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tokio::time::{self, Duration};

const NOTIFY_QUEUE_CAPACITY: usize = 10_000;
const INCREMENT_QUEUE_CAPACITY: usize = 50_000;
const INCREMENT_BATCH_MAX: usize = 2048;
const INCREMENT_FLUSH_INTERVAL_MS: u64 = 50;

struct NotificationTask {
    user_id: UserId,
    table_id: TableId,
    notification: ChangeNotification,
}

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
        },
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
    notify_tx: mpsc::Sender<NotificationTask>,
    increment_tx: mpsc::Sender<LiveQueryId>,
}

impl NotificationService {
    pub fn new(
        registry: Arc<ConnectionsManager>,
        live_queries_provider: Arc<LiveQueriesTableProvider>,
    ) -> Arc<Self> {
        let (notify_tx, mut notify_rx) = mpsc::channel(NOTIFY_QUEUE_CAPACITY);
        let (increment_tx, mut increment_rx) = mpsc::channel(INCREMENT_QUEUE_CAPACITY);
        let service = Arc::new(Self {
            registry,
            live_queries_provider,
            notify_tx,
            increment_tx,
        });

        // Notification worker (single task, no per-notification spawn)
        let notify_service = Arc::clone(&service);
        tokio::spawn(async move {
            while let Some(task) = notify_rx.recv().await {
                let handles = notify_service
                    .registry
                    .get_subscriptions_for_table(&task.user_id, &task.table_id);
                if handles.is_empty() {
                    log::debug!(
                        "NotificationWorker: No subscriptions for user={}, table={} (skipping notification)",
                        task.user_id, task.table_id
                    );
                    continue;
                }
                log::debug!(
                    "NotificationWorker: Found {} subscriptions for user={}, table={}",
                    handles.len(),
                    task.user_id,
                    task.table_id
                );

                if let Err(e) = notify_service
                    .notify_table_change_with_handles(
                        &task.user_id,
                        &task.table_id,
                        task.notification,
                        handles,
                    )
                    .await
                {
                    log::warn!("Failed to notify subscribers for table {}: {}", task.table_id, e);
                }
            }
        });

        // Increment worker (batched updates, off hot path)
        let increment_service = Arc::clone(&service);
        tokio::spawn(async move {
            let mut pending = std::collections::HashMap::new();
            let mut ticker = time::interval(Duration::from_millis(INCREMENT_FLUSH_INTERVAL_MS));

            loop {
                tokio::select! {
                    Some(live_id) = increment_rx.recv() => {
                        let counter = pending.entry(live_id).or_insert(0u64);
                        *counter += 1;
                        if pending.len() >= INCREMENT_BATCH_MAX {
                            flush_increment_batch(&increment_service, &mut pending).await;
                        }
                    }
                    _ = ticker.tick() => {
                        if !pending.is_empty() {
                            flush_increment_batch(&increment_service, &mut pending).await;
                        }
                    }
                    else => {
                        break;
                    }
                }
            }
        });

        service
    }

    /// Get current timestamp in milliseconds
    fn current_timestamp_ms() -> i64 {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64
    }

    /// Increment the changes counter for a live query
    ///
    /// Uses provider's async method which handles spawn_blocking internally.
    pub async fn increment_changes(&self, live_id: &LiveQueryId) -> Result<(), KalamDbError> {
        let timestamp = Self::current_timestamp_ms();

        self.live_queries_provider
            .increment_changes_async(live_id.as_str(), timestamp)
            .await
            .into_kalamdb_error("Failed to increment changes")?;

        Ok(())
    }

    #[inline]
    fn enqueue_increment(&self, live_id: &LiveQueryId) {
        if let Err(e) = self.increment_tx.try_send(live_id.clone()) {
            if matches!(e, mpsc::error::TrySendError::Full(_)) {
                log::warn!("Increment queue full for live_id={}, dropping", live_id);
            }
        }
    }

    /// Notify subscribers about a table change (fire-and-forget async)
    pub fn notify_async(
        self: &Arc<Self>,
        user_id: UserId,
        table_id: TableId,
        notification: ChangeNotification,
    ) {
        let task = NotificationTask {
            user_id,
            table_id,
            notification,
        };
        if let Err(e) = self.notify_tx.try_send(task) {
            if matches!(e, mpsc::error::TrySendError::Full(_)) {
                log::warn!("Notification queue full, dropping notification");
            }
        }
    }

    /// Notify live query subscribers of a table change
    pub async fn notify_table_change(
        &self,
        user_id: &UserId,
        table_id: &TableId,
        change_notification: ChangeNotification,
    ) -> Result<usize, KalamDbError> {
        // Fetch handles and delegate to common implementation
        let handles = self.registry.get_subscriptions_for_table(user_id, table_id);
        self.notify_table_change_with_handles(user_id, table_id, change_notification, handles)
            .await
    }

    /// Notify live query subscribers with pre-fetched handles
    /// Avoids double DashMap lookup when handles are already available
    async fn notify_table_change_with_handles(
        &self,
        user_id: &UserId,
        table_id: &TableId,
        change_notification: ChangeNotification,
        all_handles: Arc<dashmap::DashMap<LiveQueryId, SubscriptionHandle>>,
    ) -> Result<usize, KalamDbError> {
        log::debug!(
            "notify_table_change called for table: '{}', user: '{}', change_type: {:?}",
            table_id,
            user_id.as_str(),
            change_notification.change_type
        );

        // Send notifications and increment changes
        let mut notification_count = 0usize;
        let filtering_row = &change_notification.row_data;
        let mut full_row_json: Option<std::collections::HashMap<String, serde_json::Value>> = None;
        let mut full_old_json: Option<std::collections::HashMap<String, serde_json::Value>> = None;
        for entry in all_handles.iter() {
            let live_id = entry.key();
            let handle = entry.value();
            let should_notify = if matches!(change_notification.change_type, ChangeType::Flush) {
                true
            } else if let Some(ref filter_expr) = handle.filter_expr {
                match filter_matches(filter_expr, filtering_row) {
                    Ok(true) => true,
                    Ok(false) => {
                        log::trace!(
                            "Filter didn't match for live_id={}, skipping notification",
                            live_id
                        );
                        false
                    },
                    Err(e) => {
                        log::error!("Filter evaluation error for live_id={}: {}", live_id, e);
                        false
                    },
                }
            } else {
                true
            };

            if !should_notify {
                continue;
            }

            let projections = &handle.projections;
            let tx = &handle.notification_tx;

            let use_full_row = projections.is_none();
            let row_json = if use_full_row {
                match full_row_json {
                    Some(ref json) => json.clone(),
                    None => match row_to_json_map(&change_notification.row_data) {
                        Ok(json) => {
                            full_row_json = Some(json.clone());
                            json
                        },
                        Err(e) => {
                            log::error!(
                                "Failed to convert row to JSON for live_id={}: {}",
                                live_id,
                                e
                            );
                            continue;
                        },
                    },
                }
            } else {
                // Apply projections to row data (filters columns based on subscription)
                // Uses Cow to avoid cloning when no projections (SELECT *)
                let projected_row_data =
                    apply_projections(&change_notification.row_data, projections);
                match row_to_json_map(&projected_row_data) {
                    Ok(json) => json,
                    Err(e) => {
                        log::error!("Failed to convert row to JSON for live_id={}: {}", live_id, e);
                        continue;
                    },
                }
            };

            let old_json = if let Some(old) = change_notification.old_data.as_ref() {
                if use_full_row {
                    match full_old_json {
                        Some(ref json) => Some(json.clone()),
                        None => match row_to_json_map(old) {
                            Ok(json) => {
                                full_old_json = Some(json.clone());
                                Some(json)
                            },
                            Err(e) => {
                                log::error!(
                                    "Failed to convert old row to JSON for live_id={}: {}",
                                    live_id,
                                    e
                                );
                                continue;
                            },
                        },
                    }
                } else {
                    let projected_old_data = apply_projections(old, projections);
                    match row_to_json_map(&projected_old_data) {
                        Ok(json) => Some(json),
                        Err(e) => {
                            log::error!(
                                "Failed to convert old row to JSON for live_id={}: {}",
                                live_id,
                                e
                            );
                            continue;
                        },
                    }
                }
            } else {
                None
            };

            // Build the notification with JSON data
            // Compute live_id string once to avoid 4 allocations per notification
            let live_id_str = live_id.to_string();
            let notification = match change_notification.change_type {
                ChangeType::Insert => {
                    kalamdb_commons::Notification::insert(live_id_str, vec![row_json])
                },
                ChangeType::Update => kalamdb_commons::Notification::update(
                    live_id_str,
                    vec![row_json],
                    vec![old_json.unwrap_or_else(std::collections::HashMap::new)],
                ),
                ChangeType::Delete => {
                    kalamdb_commons::Notification::delete(live_id_str, vec![row_json])
                },
                ChangeType::Flush => {
                    kalamdb_commons::Notification::insert(live_id_str, vec![row_json])
                },
            };

            // Send notification through channel (non-blocking, bounded)
            let notification = Arc::new(notification);
            if let Err(e) = tx.try_send(notification) {
                use tokio::sync::mpsc::error::TrySendError;
                match e {
                    TrySendError::Full(_) => {
                        log::warn!(
                            "Notification channel full for live_id={}, dropping notification",
                            live_id
                        );
                    },
                    TrySendError::Closed(_) => {
                        log::debug!(
                            "Notification channel closed for live_id={}, connection likely disconnected",
                            live_id
                        );
                    },
                }
            }

            self.enqueue_increment(live_id);
            notification_count += 1;
        }

        Ok(notification_count)
    }
}

async fn flush_increment_batch(
    service: &NotificationService,
    pending: &mut std::collections::HashMap<LiveQueryId, u64>,
) {
    let timestamp = NotificationService::current_timestamp_ms();
    for (live_id, count) in pending.drain() {
        if let Err(e) = service
            .live_queries_provider
            .increment_changes_by_async(live_id.as_str(), count, timestamp)
            .await
        {
            log::error!("Failed to increment changes for live_id={}: {}", live_id, e);
        }
    }
}
