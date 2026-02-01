use kalamdb_commons::models::{TableId, UserId};

/// Interface for NotificationService implementations used by table providers.
///
/// Provides a generic way to dispatch notifications about data changes
/// (INSERT, UPDATE, DELETE) to subscribers.
///
/// Implementations include:
/// - Live query notifications via WebSocket
/// - Topic/consumer notifications (future)
pub trait NotificationService: Send + Sync {
    type Notification: Send + Sync + 'static;

    /// Check if there are any subscribers for a given user and table.
    /// Returns true if at least one subscriber exists.
    /// Use this before constructing notifications to avoid unnecessary work.
    fn has_subscribers(&self, user_id: &UserId, table_id: &TableId) -> bool;

    /// Notify subscribers about a data change (fire-and-forget async).
    fn notify_table_change_async(
        &self,
        user_id: UserId,
        table_id: TableId,
        notification: Self::Notification,
    );
}
