//! Event Broadcaster - Distributes events to registered handlers

use parking_lot::RwLock;
use std::sync::Arc;

use super::{DatabaseEvent, EventEmitter, EventHandler};

/// Broadcaster that distributes events to multiple handlers
pub struct EventBroadcaster {
    handlers: RwLock<Vec<Arc<dyn EventHandler>>>,
}

impl EventBroadcaster {
    pub fn new() -> Self {
        Self {
            handlers: RwLock::new(Vec::new()),
        }
    }

    /// Register an event handler
    pub fn register(&self, handler: Arc<dyn EventHandler>) {
        self.handlers.write().push(handler);
    }

    /// Unregister all handlers (for shutdown)
    pub fn clear(&self) {
        self.handlers.write().clear();
    }

    /// Get the number of registered handlers
    pub fn handler_count(&self) -> usize {
        self.handlers.read().len()
    }
}

impl Default for EventBroadcaster {
    fn default() -> Self {
        Self::new()
    }
}

impl EventEmitter for EventBroadcaster {
    fn emit(&self, event: DatabaseEvent) {
        let handlers = self.handlers.read();
        for handler in handlers.iter() {
            if handler.is_interested(&event) {
                handler.handle(&event);
            }
        }
    }
}

/// Live Query event handler
///
/// Notifies live query manager when DML events occur
#[allow(dead_code)]
pub struct LiveQueryEventHandler {
    // Will hold reference to LiveQueryManager
    // For now, we'll use a callback
    callback: Box<dyn Fn(&DatabaseEvent) + Send + Sync>,
}

impl LiveQueryEventHandler {
    #[allow(dead_code)]
    pub fn new<F>(callback: F) -> Self
    where
        F: Fn(&DatabaseEvent) + Send + Sync + 'static,
    {
        Self {
            callback: Box::new(callback),
        }
    }
}

impl EventHandler for LiveQueryEventHandler {
    fn handle(&self, event: &DatabaseEvent) {
        (self.callback)(event);
    }

    fn is_interested(&self, event: &DatabaseEvent) -> bool {
        // Interested in DML events for live query updates
        matches!(
            event,
            DatabaseEvent::RowsInserted { .. }
                | DatabaseEvent::RowsUpdated { .. }
                | DatabaseEvent::RowsDeleted { .. }
        )
    }
}

/// Audit Log event handler
///
/// Records DDL and security events to audit log
#[allow(dead_code)]
pub struct AuditEventHandler {
    callback: Box<dyn Fn(&DatabaseEvent) + Send + Sync>,
}

impl AuditEventHandler {
    #[allow(dead_code)]
    pub fn new<F>(callback: F) -> Self
    where
        F: Fn(&DatabaseEvent) + Send + Sync + 'static,
    {
        Self {
            callback: Box::new(callback),
        }
    }
}

impl EventHandler for AuditEventHandler {
    fn handle(&self, event: &DatabaseEvent) {
        (self.callback)(event);
    }

    fn is_interested(&self, event: &DatabaseEvent) -> bool {
        // Interested in DDL and user events for audit logging
        matches!(
            event,
            DatabaseEvent::TableCreated { .. }
                | DatabaseEvent::TableAltered { .. }
                | DatabaseEvent::TableDropped { .. }
                | DatabaseEvent::NamespaceCreated { .. }
                | DatabaseEvent::NamespaceDropped { .. }
                | DatabaseEvent::UserCreated { .. }
                | DatabaseEvent::UserUpdated { .. }
                | DatabaseEvent::UserDeleted { .. }
        )
    }
}
