//! Storage health check and connectivity testing.
//!
//! This module provides the `StorageHealthService` which validates storage backend
//! connectivity by performing a series of CRUD operations (create dir, write, list,
//! read, delete) on a test file.
//!
//! # Usage
//!
//! ```ignore
//! use kalamdb_filestore::health::{StorageHealthService, HealthStatus};
//!
//! let result = StorageHealthService::run_full_health_check(&storage).await?;
//! match result.status {
//!     HealthStatus::Healthy => println!("Storage is healthy"),
//!     HealthStatus::Degraded => println!("Storage has issues: {:?}", result.error),
//!     HealthStatus::Unreachable => println!("Cannot connect to storage"),
//! }
//! ```

mod models;
mod service;

pub use models::{ConnectivityTestResult, HealthStatus, StorageHealthResult};
pub use service::StorageHealthService;

#[cfg(test)]
mod tests;
