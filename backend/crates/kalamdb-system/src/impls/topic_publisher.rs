use kalamdb_commons::models::{rows::Row, TableId, TopicOp, UserId};

/// Interface for synchronous topic publishing from table providers.
///
/// Table providers call `publish_for_table()` directly in the write path
/// to ensure topic messages are persisted before the write is acknowledged.
/// This replaces the previous async notification-queue-based approach which
/// could drop events under high load due to `try_send()` on a bounded channel.
///
/// ## Design
///
/// - Called inline from INSERT/UPDATE/DELETE in table providers
/// - Returns the count of messages published (0 if no routes match)
/// - The implementation handles route matching, payload extraction, and RocksDB persistence
/// - Errors are reported but should not fail the table write (best-effort synchronous)
pub trait TopicPublisher: Send + Sync {
    /// Check if any topic routes are configured for a given table.
    ///
    /// Fast DashMap lookup used to skip publishing overhead when no topics exist.
    fn has_topics_for_table(&self, table_id: &TableId) -> bool;

    /// Publish a row change to matching topics synchronously.
    ///
    /// Called directly from table providers in the write path. For each matching
    /// topic route, extracts the payload, allocates an offset atomically, and
    /// persists the message to RocksDB.
    ///
    /// # Arguments
    /// * `table_id` - Table that was modified
    /// * `operation` - INSERT, UPDATE, or DELETE
    /// * `row` - The row data (new row for insert/update, tombstone for delete)
    /// * `user_id` - Optional user context (for user-scoped tables)
    ///
    /// # Returns
    /// Number of messages published across all matching topic routes.
    fn publish_for_table(
        &self,
        table_id: &TableId,
        operation: TopicOp,
        row: &Row,
        user_id: Option<&UserId>,
    ) -> std::result::Result<usize, String>;
}
