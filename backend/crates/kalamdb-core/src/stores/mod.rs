//! EntityStore-based stores for table data.
//!
//! This module provides strongly-typed storage for user, shared, and stream tables
//! using the EntityStore trait and StorageBackend abstraction.
//!
//! ## Architecture
//!
//! These stores sit on top of `StorageBackend` (from `kalamdb-store`) and provide:
//! - Strongly-typed CRUD operations for table rows
//! - Dynamic partition management (one partition per table)
//! - Automatic partition creation
//! - JSON serialization with system column injection
//!
//! ## Stores
//!
//! - `SystemTableStore`: Generic store for all system tables (Phase 14)
//! - `UserTableStore`: Storage for user-scoped tables with user isolation
//! - `SharedTableStore`: Storage for cross-user shared tables with access control
//! - `StreamTableStore`: Storage for ephemeral stream tables with TTL

pub mod shared_table;
pub mod stream_table;
pub mod system_table; // Phase 14: Generic system table store
pub mod user_table;

pub use shared_table::SharedTableStore;
pub use stream_table::StreamTableStore;
pub use system_table::SystemTableStore; // Phase 14: Export SystemTableStore
pub use user_table::UserTableStore;
