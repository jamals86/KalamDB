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
//! Phase 6 Complete: Core subscription and expression caching implemented.
//! Integration with WebSocket handling exists in kalamdb-core/src/live_query/.

// Module implementations

/// Subscription management (T208, T212)
///
/// Contains LiveQuerySubscription struct with filter_sql and cached_expr fields.
pub mod subscription;

/// DataFusion expression caching (T211, T213)
///
/// Compiles and caches SQL filter expressions for efficient evaluation.
pub mod expression_cache;

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
    //!
    //! # Note
    //!
    //! Most of this functionality currently exists in:
    //! - `kalamdb-core/src/live_query/manager.rs`
    //! - `kalamdb-core/src/live_query/registry.rs`
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
    //!
    //! # Note
    //!
    //! Most of this functionality currently exists in:
    //! - `kalamdb-core/src/live_query/notifier.rs`
    //! - `kalamdb-api/src/websocket/session.rs`
}

// Re-export main types for convenience
pub use expression_cache::CachedExpression;
pub use subscription::LiveQuerySubscription;
