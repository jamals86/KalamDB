//! Event System - Database events and their handling
//!
//! Events are emitted after successful mutations and can be used for:
//! - Live query updates
//! - Audit logging
//! - Metrics collection
//! - Cache invalidation

mod broadcast;

use kalamdb_commons::models::{NamespaceId, StorageId, TableId, UserId};
use kalamdb_commons::schemas::TableType;

pub use broadcast::EventBroadcaster;

/// Database event types
///
/// These events are emitted after successful mutations
/// to notify interested parties (live queries, audit, etc.)
#[derive(Debug, Clone)]
pub enum DatabaseEvent {
    // Table events
    TableCreated {
        table_id: TableId,
        table_type: TableType,
    },
    TableAltered {
        table_id: TableId,
        old_version: u32,
        new_version: u32,
    },
    TableDropped {
        table_id: TableId,
    },

    // Namespace events
    NamespaceCreated {
        namespace_id: NamespaceId,
    },
    NamespaceDropped {
        namespace_id: NamespaceId,
    },

    // Storage events
    StorageCreated {
        storage_id: StorageId,
    },
    StorageDropped {
        storage_id: StorageId,
    },

    // User events
    UserCreated {
        user_id: UserId,
    },
    UserUpdated {
        user_id: UserId,
    },
    UserDeleted {
        user_id: UserId,
    },

    // DML events
    RowsInserted {
        table_id: TableId,
        count: usize,
        user_id: Option<UserId>,
    },
    RowsUpdated {
        table_id: TableId,
        count: usize,
        user_id: Option<UserId>,
    },
    RowsDeleted {
        table_id: TableId,
        count: usize,
        user_id: Option<UserId>,
    },

    // Flush events
    TableFlushed {
        table_id: TableId,
        batch_id: String,
        row_count: usize,
    },
}

impl DatabaseEvent {
    /// Get the event type name
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::TableCreated { .. } => "table_created",
            Self::TableAltered { .. } => "table_altered",
            Self::TableDropped { .. } => "table_dropped",
            Self::NamespaceCreated { .. } => "namespace_created",
            Self::NamespaceDropped { .. } => "namespace_dropped",
            Self::StorageCreated { .. } => "storage_created",
            Self::StorageDropped { .. } => "storage_dropped",
            Self::UserCreated { .. } => "user_created",
            Self::UserUpdated { .. } => "user_updated",
            Self::UserDeleted { .. } => "user_deleted",
            Self::RowsInserted { .. } => "rows_inserted",
            Self::RowsUpdated { .. } => "rows_updated",
            Self::RowsDeleted { .. } => "rows_deleted",
            Self::TableFlushed { .. } => "table_flushed",
        }
    }

    /// Get the table ID if this event is table-related
    pub fn table_id(&self) -> Option<&TableId> {
        match self {
            Self::TableCreated { table_id, .. } => Some(table_id),
            Self::TableAltered { table_id, .. } => Some(table_id),
            Self::TableDropped { table_id } => Some(table_id),
            Self::RowsInserted { table_id, .. } => Some(table_id),
            Self::RowsUpdated { table_id, .. } => Some(table_id),
            Self::RowsDeleted { table_id, .. } => Some(table_id),
            Self::TableFlushed { table_id, .. } => Some(table_id),
            _ => None,
        }
    }
}

/// Trait for event emitters
pub trait EventEmitter: Send + Sync {
    /// Emit a database event
    fn emit(&self, event: DatabaseEvent);
}

/// A no-op event emitter (used when events are not needed)
pub struct NoOpEventEmitter;

impl EventEmitter for NoOpEventEmitter {
    fn emit(&self, _event: DatabaseEvent) {
        // No-op
    }
}

/// Event handler trait
pub trait EventHandler: Send + Sync {
    /// Handle a database event
    fn handle(&self, event: &DatabaseEvent);

    /// Check if this handler is interested in the event
    fn is_interested(&self, event: &DatabaseEvent) -> bool;
}
