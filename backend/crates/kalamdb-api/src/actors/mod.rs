//! WebSocket session state management
//!
//! This module previously provided state-based WebSocket handling.
//! Now consolidated into kalamdb_core::live::ConnectionRegistry.
//!
//! Legacy types removed:
//! - HeartbeatManager (replaced by ConnectionRegistry)
//! - WebSocketState (replaced by ws_handler)

// Re-export from kalamdb-core for backward compatibility
pub use kalamdb_core::live::ConnectionRegistry;
pub use kalamdb_core::live::ConnectionEvent;
