// KalamDB Core Library
//
// This crate provides the core storage functionality for KalamDB,
// including namespace/table management, Arrow schema handling, RocksDB storage,
// and live query subscriptions.

pub mod error;
pub mod jobs;
pub mod live;
pub mod manifest;
pub mod providers;
pub mod slow_query_logger;
pub mod app_context;
pub mod sql;
pub mod storage;
pub mod test_helpers;
pub mod schema_registry;

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

// // Test helpers (only compiled in test mode)
// #[cfg(test)]
// pub mod test_helpers;
