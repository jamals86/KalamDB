//! Flush module for flush policy management
//!
//! This module manages flush policies that determine when data is written
//! from RocksDB to Parquet storage.

pub mod policy;

pub use policy::FlushPolicy;
