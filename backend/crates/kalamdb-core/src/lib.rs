// KalamDB Core Library
//
// This crate provides the core storage functionality for KalamDB,
// including namespace/table management, Arrow schema handling, RocksDB storage,
// and live query subscriptions.

pub mod catalog;
pub mod config;
pub mod error;
pub mod flush;
pub mod ids;
pub mod live_query;
pub mod models;
pub mod schema;
pub mod sql;
pub mod storage;
pub mod tables;
