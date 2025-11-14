//! Manifest management for Parquet batch files.

pub mod cache_service;
pub mod service;

pub use cache_service::ManifestCacheService;
pub use service::ManifestService;
