// KalamDB Core Library
//
// This crate provides the core storage functionality for KalamDB,
// including namespace/table management, Arrow schema handling, RocksDB storage,
// and live query subscriptions.

pub mod auth;
pub mod catalog;
// pub mod config;
pub mod error;
pub mod flush;
pub mod ids;
pub mod jobs;
pub mod live_query;
// pub mod metrics;
pub mod models; // Domain models for system tables and table rows
pub mod scheduler;
pub mod schema;
pub mod services;
pub mod sql;
pub mod storage;
pub mod stores; // EntityStore-based table stores
pub mod system_table_registration;
pub mod tables;

// Optional facade for initializing core stores from a generic StorageBackend
pub mod kalam_core;
