//! Storages table EntityStore implementation (Phase 14 Step 4)
//!
//! This module provides the new EntityStore-based implementation for system.storages table.

pub mod models;
mod storages_provider;

pub use models::Storage;
pub use storages_provider::StoragesTableProvider;
