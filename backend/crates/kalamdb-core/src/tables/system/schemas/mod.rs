//! Schema storage infrastructure.
//!
//! This module provides the EntityStore implementation for TableDefinition.
//!
//! **Phase 10 Update**: Old system SchemaCache deleted - unified SchemaCache
//! at `crate::catalog::SchemaCache` is now used for all schema caching.
//!
//! ## Architecture
//!
//! ```text
//! Unified SchemaCache              ← In-memory cache (99%+ hit rate) - see crate::catalog::SchemaCache
//!     ↓ (on miss)
//! TableSchemaStore                 ← EntityStore<TableId, TableDefinition>
//!     ↓
//! RocksDB                          ← Persistent storage
//! ```

pub mod table_schema_store;

pub use table_schema_store::TableSchemaStore;
