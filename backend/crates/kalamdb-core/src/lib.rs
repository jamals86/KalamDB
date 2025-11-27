// KalamDB Core Library
//
// This crate provides the core storage functionality for KalamDB,
// including namespace/table management, Arrow schema handling, RocksDB storage,
// and live query subscriptions.

pub mod app_context;
pub mod error;
pub mod jobs;
pub mod live;
pub mod manifest;
pub mod providers;
pub mod schema_registry;
pub mod slow_query_logger;
pub mod sql;
pub mod storage;
pub mod test_helpers;

// Re-export modules that were moved to other crates
pub mod auth {
    pub use kalamdb_auth::rbac;
    pub use kalamdb_auth::roles;
}

pub mod live_query {
    pub use crate::live::*;
}

pub mod system_columns {
    pub use crate::schema_registry::SystemColumnsService;
}
