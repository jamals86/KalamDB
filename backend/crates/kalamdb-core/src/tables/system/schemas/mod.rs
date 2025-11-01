//! Schema storage and caching infrastructure.
//!
//! This module provides the EntityStore implementation for TableDefinition
//! and a high-performance DashMap-based cache for schema lookups.
//!
//! ## Architecture
//!
//! ```text
//! SchemaCache (DashMap)        ← In-memory cache (99%+ hit rate)
//!     ↓ (on miss)
//! TableSchemaStore             ← EntityStore<TableId, TableDefinition>
//!     ↓
//! RocksDB                      ← Persistent storage
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use kalamdb_core::tables::system::schemas::{TableSchemaStore, SchemaCache};
//! use kalamdb_commons::models::TableId;
//! use kalamdb_commons::schemas::TableDefinition;
//!
//! // Create store with cache integration
//! let store = TableSchemaStore::new(backend);
//! let cache = SchemaCache::new(1000); // Max 1000 cached schemas
//!
//! // Read with caching
//! if let Some(schema) = cache.get(&table_id) {
//!     // Cache hit
//! } else {
//!     // Cache miss - read from store
//!     if let Some(schema) = store.get(&table_id)? {
//!         cache.insert(table_id.clone(), schema.clone());
//!     }
//! }
//! ```

pub mod schema_cache;
pub mod table_schema_store;

pub use schema_cache::SchemaCache;
pub use table_schema_store::TableSchemaStore;
