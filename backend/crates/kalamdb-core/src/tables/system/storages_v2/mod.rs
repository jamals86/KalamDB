//! Storages table EntityStore implementation (Phase 14 Step 4)
//!
//! This module provides the new EntityStore-based implementation for system.storages table.

mod storages_provider;
mod storages_store;
mod storages_table;

pub use storages_provider::StoragesTableProvider;
pub use storages_store::{new_storages_store, StoragesStore};
pub use storages_table::StoragesTableSchema;
