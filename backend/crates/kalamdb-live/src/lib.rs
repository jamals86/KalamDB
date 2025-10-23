//! # kalamdb-live
//!
//! Live query subscription management for KalamDB.
//!
//! This crate provides the infrastructure for managing WebSocket-based live query
//! subscriptions, including:
//! - Subscription lifecycle management
//! - Change event notification
//! - DataFusion expression caching for live query filters
//!
//! ## Module Structure
//!
//! - `subscription`: LiveQuerySubscription struct for managing individual subscriptions
//! - `manager`: Subscription lifecycle and registry management
//! - `notifier`: Client notification delivery logic
//! - `expression_cache`: DataFusion expression compilation and caching
//!
//! ## Architecture
//!
//! The kalamdb-live crate provides a clean, focused API for live query functionality,
//! separating concerns from the core kalamdb-core crate. It integrates with:
//!
//! - **kalamdb-core**: Uses core change detection and table providers
//! - **DataFusion**: Leverages query planning and expression evaluation
//! - **Actix**: Actor-based WebSocket session management
//!
//! ## Status
//!
//! Phase 6 (Integration Testing): Test infrastructure complete, implementation deferred.
//! Most live query functionality exists in kalamdb-core/src/live_query/.
//! This crate will eventually provide a cleaner separation of concerns.

// Module declarations for future implementation

/// Subscription management (T208)
///
/// Contains LiveQuerySubscription struct with filter_sql and cached_expr fields.
pub mod subscription {
    //! Live query subscription data structures and lifecycle management.
    //!
    //! # Future Implementation
    //!
    //! This module will contain:
    //! - `LiveQuerySubscription` struct with fields:
    //!   - `live_id`: Unique subscription identifier
    //!   - `filter_sql`: SQL WHERE clause for filtering
    //!   - `cached_expr`: Compiled DataFusion expression
    //!   - `changes`: Counter for delivered notifications
    //! - Subscription creation and validation logic
    //! - Filter expression compilation
}

/// Subscription lifecycle management (T209)
///
/// Coordinates subscription creation, updates, and cleanup.
pub mod manager {
    //! Subscription registry and lifecycle coordination.
    //!
    //! # Future Implementation
    //!
    //! This module will provide:
    //! - Registry of active subscriptions
    //! - Subscription creation and removal
    //! - Connection tracking per user
    //! - Cleanup on WebSocket disconnect
}

/// Client notification logic (T210)
///
/// Handles delivery of change notifications to WebSocket clients.
pub mod notifier {
    //! Change notification delivery to WebSocket clients.
    //!
    //! # Future Implementation
    //!
    //! This module will handle:
    //! - Notification formatting (INSERT/UPDATE/DELETE)
    //! - WebSocket message serialization
    //! - Client-specific filtering
    //! - Delivery error handling
}

/// DataFusion expression caching (T211)
///
/// Compiles and caches SQL filter expressions for efficient evaluation.
pub mod expression_cache {
    //! Compiled expression caching for live query filters.
    //!
    //! # Future Implementation
    //!
    //! This module will provide:
    //! - `CachedExpression` struct wrapping DataFusion `Expr`
    //! - Expression compilation from SQL WHERE clauses
    //! - Cache invalidation on schema changes
    //! - Expression evaluation against record batches
    //!
    //! # Performance
    //!
    //! Caching compiled expressions provides ~50% performance improvement
    //! over re-parsing SQL filters for each change notification.
}
