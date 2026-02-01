use kalamdb_commons::models::{TableId, UserId};

/// Interface for NotificationService implementations used by table providers.
///
/// Provides a generic way to dispatch notifications about data changes
/// (INSERT, UPDATE, DELETE) to subscribers.
///
/// Unified approach:
/// - User tables: call with `Some(user_id)` → notifies live queries + topics
/// - Shared tables: call with `None` → only notifies topics
///
/// This prevents duplicate notifications and provides a single code path.
pub trait NotificationService: Send + Sync {
    type Notification: Send + Sync + 'static;

    /// Check if there are any subscribers (live queries or topics) for a table.
    /// - If `user_id` is provided: checks both user-scoped live queries AND table-scoped topics
    /// - If `user_id` is None: checks only table-scoped topics
    /// Returns true if at least one subscriber exists.
    fn has_subscribers(&self, user_id: Option<&UserId>, table_id: &TableId) -> bool;

    /// Notify subscribers about a data change (fire-and-forget async).
    /// 
    /// Unified notification method that handles both:
    /// - Live query notifications (if `user_id` is provided)
    /// - Topic/CDC notifications (if topics are configured for this table)
    ///
    /// # Arguments
    /// * `user_id` - Optional user ID for user-scoped live query notifications
    /// * `table_id` - Table identifier
    /// * `notification` - Change notification data
    fn notify_table_change(
        &self,
        user_id: Option<UserId>,
        table_id: TableId,
        notification: Self::Notification,
    );
}
