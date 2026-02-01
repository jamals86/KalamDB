//! Notification service for live queries and consumers
//!
//! Handles dispatching change notifications to subscribed clients,
//! including filtering based on WHERE clauses stored in SubscriptionState.
//!
//! Used by:
//! - WebSocket live query subscribers
//! - Topic pub/sub routing (via TopicPublisherService)

use super::helpers::filter_eval::matches as filter_matches;
use super::manager::ConnectionsManager;
use super::models::{ChangeNotification, ChangeType, SubscriptionHandle};
use super::topic_publisher::TopicPublisherService;
use crate::error::KalamDbError;
use crate::providers::arrow_json_conversion::row_to_json_map;
use datafusion::scalar::ScalarValue;
use kalamdb_commons::constants::SystemColumnNames;
use kalamdb_commons::ids::SeqId;
use kalamdb_commons::models::rows::Row;
use kalamdb_commons::models::{LiveQueryId, TableId, TopicOp, UserId};
use kalamdb_system::NotificationService as NotificationServiceTrait;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::{mpsc, OnceCell};

const NOTIFY_QUEUE_CAPACITY: usize = 10_000;

struct NotificationTask {
    user_id: UserId,
    table_id: TableId,
    notification: ChangeNotification,
}

#[inline]
fn extract_seq(change_notification: &ChangeNotification) -> Option<SeqId> {
    change_notification
        .row_data
        .values
        .get(SystemColumnNames::SEQ)
        .and_then(|value| match value {
            ScalarValue::Int64(Some(seq)) => Some(SeqId::from(*seq)),
            ScalarValue::UInt64(Some(seq)) => Some(SeqId::from(*seq as i64)),
            _ => None,
        })
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
///
/// Also handles topic pub/sub routing via TopicPublisherService.
pub struct NotificationService {
    /// Manager uses DashMap internally for lock-free access
    registry: Arc<ConnectionsManager>,
    notify_tx: mpsc::Sender<NotificationTask>,
    /// Topic publisher for CDC → topic routing (set after AppContext creation)
    topic_publisher: OnceCell<Arc<TopicPublisherService>>,
}

impl NotificationService {
    /// Check if there are any subscribers for a given user and table
    pub fn has_subscribers(&self, user_id: &UserId, table_id: &TableId) -> bool {
        self.registry.has_subscriptions(user_id, table_id)
    }

    /// Set the topic publisher for CDC → topic routing
    ///
    /// Called after AppContext creation to break circular dependency.
    pub fn set_topic_publisher(&self, topic_publisher: Arc<TopicPublisherService>) {
        if self.topic_publisher.set(topic_publisher).is_err() {
            log::warn!("TopicPublisher already set in NotificationService");
        }
    }

    /// Get the topic publisher (if set)
    fn topic_publisher(&self) -> Option<&Arc<TopicPublisherService>> {
        self.topic_publisher.get()
    }

    pub fn new(registry: Arc<ConnectionsManager>) -> Arc<Self> {
        let (notify_tx, mut notify_rx) = mpsc::channel(NOTIFY_QUEUE_CAPACITY);
        let service = Arc::new(Self {
            registry,
            notify_tx,
            topic_publisher: OnceCell::new(),
        });

        // Notification worker (single task, no per-notification spawn)
        let notify_service = Arc::clone(&service);
        tokio::spawn(async move {
            while let Some(task) = notify_rx.recv().await {
                // TODO: Route to topic publisher if configured
                // Challenge: TopicPublisherService.route_and_publish expects RecordBatch
                // but NotificationTask has ChangeNotification (Row-based)
                // Solution: Move topic routing to DML handlers where RecordBatch is available

                // Route to live query subscriptions
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
                    log::warn!(
                        "Failed to notify subscribers for table {}: {}",
                        task.table_id,
                        e
                    );
                }
            }
        });

        service
    }

    /// Notify subscribers about a table change (fire-and-forget async)
    pub fn notify_async(
        &self,
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

    // /// Notify live query subscribers of a table change
    // pub async fn notify_table_change(
    //     &self,
    //     user_id: &UserId,
    //     table_id: &TableId,
    //     change_notification: ChangeNotification,
    // ) -> Result<usize, KalamDbError> {
    //     // Fetch handles and delegate to common implementation
    //     let handles = self.registry.get_subscriptions_for_table(user_id, table_id);
    //     self.notify_table_change_with_handles(user_id, table_id, change_notification, handles)
    //         .await
    // }

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
        let seq_value = extract_seq(&change_notification);
        for entry in all_handles.iter() {
            let live_id = entry.key();
            let handle = entry.value();
            let should_notify = if let Some(ref filter_expr) = handle.filter_expr {
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
                }
            };

            // Send notification through channel (non-blocking, bounded)
            let notification = Arc::new(notification);
            let flow_control = &handle.flow_control;

            if !flow_control.is_initial_complete() {
                if let Some(snapshot_seq) = flow_control.snapshot_end_seq() {
                    if let Some(seq) = seq_value {
                        if seq.as_i64() <= snapshot_seq {
                            continue;
                        }
                    }
                }

                flow_control.buffer_notification(Arc::clone(&notification), seq_value);
                continue;
            }

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

            notification_count += 1;
        }

        Ok(notification_count)
    }
}

impl NotificationServiceTrait for NotificationService {
    type Notification = ChangeNotification;

    fn has_subscribers(&self, user_id: &UserId, table_id: &TableId) -> bool {
        self.has_subscribers(user_id, table_id)
    }

    fn notify_table_change_async(
        &self,
        user_id: UserId,
        table_id: TableId,
        notification: ChangeNotification,
    ) {
        self.notify_async(user_id, table_id, notification);
    }
}
