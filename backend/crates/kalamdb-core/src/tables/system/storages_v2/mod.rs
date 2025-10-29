//! Storages table EntityStore implementation (Phase 14 Step 4)
//!
//! This module provides the new EntityStore-based implementation for system.storages table.

mod storages_table;
mod storages_store;
mod storages_provider;

pub use storages_table::StoragesTableSchema;
pub use storages_store::{new_storages_store, StoragesStore};
pub use storages_provider::StoragesTableProvider;
