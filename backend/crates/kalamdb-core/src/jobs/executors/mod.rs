//! Job Executors Module
//!
//! Unified job executor trait and registry for all job types.
//!
//! This module provides:
//! - `JobExecutor` trait: Common interface for all job executors
//! - `JobDecision`: Result type for job execution (Completed, Retry, Failed)
//! - `JobContext`: Execution context with app access and auto-prefixed logging
//! - `JobRegistry`: Thread-safe registry mapping JobType to executors

pub mod executor_trait;
pub mod registry;

// Re-export key types
pub use executor_trait::{JobContext, JobDecision, JobExecutor};
pub use registry::JobRegistry;
