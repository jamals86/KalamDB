//! Views module for virtual table definitions
//!
//! This module contains view implementations for information_schema and other virtual tables.
//! All views follow the unified VirtualView pattern defined in view_base.rs.

pub mod view_base;
pub mod information_schema;
pub mod stats;

pub use view_base::{VirtualView, ViewTableProvider};
