// KalamDB Core Library
//
// This crate provides the core storage functionality for KalamDB,
// including namespace/table management, Arrow schema handling, RocksDB storage,
// and live query subscriptions.

pub mod auth;
pub mod error;
pub mod flush;
pub mod jobs;
pub mod live_query;
pub mod schema_registry;
pub mod app_context;
pub mod sql;
pub mod storage;
pub mod system_table_registration;
pub mod tables;
pub mod test_helpers;

// // Test helpers (only compiled in test mode)
// #[cfg(test)]
// pub mod test_helpers;
