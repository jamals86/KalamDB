//! Providers module - Unified table providers with BaseTableProvider trait
//!
//! **Phase 13: Provider Consolidation**
//!
//! This module introduces the new providers/ architecture that eliminates ~1200 lines
//! of duplicate code across User/Shared/Stream table providers.
//!
//! **Key Changes**:
//! - BaseTableProvider<K, V> trait with generic storage abstraction
//! - TableProviderCore shared structure for common services
//! - UserTableProvider with RLS via user_id parameter
//! - SharedTableProvider without RLS (ignores user_id)
//! - StreamTableProvider with RLS + TTL filtering (hot-only storage)
//! - No separate handlers - all DML logic inline in providers
//!
//! **Migration Path**:
//! 1. Phase 13.3: Create providers/ module (this file) ✅
//! 2. Phase 13.4: Migrate call sites to use new providers
//! 3. Phase 13.6: Delete old tables/ module wrappers
//!
//! **Deprecation Notice**:
//! The old `tables/` module with UserTableShared, handlers, and wrappers is
//! deprecated and will be removed in Phase 13.6. Use providers/ instead.

pub mod arrow_json_conversion; // Shared Arrow<->JSON utilities used by providers and flush
pub mod base;
pub mod core;
pub mod flush; // Phase 13.7: Consolidated flush logic from tables/
pub mod helpers;
pub mod parquet;
pub mod pk_helpers; // PK index helpers - parse_pk_value and other common utilities
pub mod scan_row;
pub mod shared;
pub mod streams;
pub mod unified_dml; // Phase 13.6: Moved from tables/
pub mod users;
pub mod version_resolution; // Phase 13.6: Moved from tables/

// Re-export key types for convenience
pub use base::{BaseTableProvider, TableProviderCore};
pub use flush::{
    FlushJobResult, FlushMetadata, SharedTableFlushJob, SharedTableFlushMetadata, TableFlush,
    UserTableFlushJob, UserTableFlushMetadata,
};
pub use shared::SharedTableProvider;
pub use streams::StreamTableProvider;
pub use users::UserTableProvider;

// Re-export unified DML functions
pub use unified_dml::{
    append_version, append_version_sync, extract_user_pk_value, generate_storage_key,
    resolve_latest_version, validate_primary_key,
};

// Re-export version resolution helpers
pub use version_resolution::{
    resolve_latest_version as resolve_latest_version_batch, scan_with_version_resolution_to_kvs,
};

/// Provider consolidation summary
///
/// **Code Reduction**: ~1200 lines eliminated
/// - UserTableShared wrapper: ~200 lines
/// - Duplicate DML methods: ~800 lines
/// - Handler indirection: ~200 lines
///
/// **Architecture Benefits**:
/// - Generic trait over storage key (K) and value (V) types
/// - Shared core reduces memory overhead (Arc<TableProviderCore>)
/// - Direct DML implementation (no handler layer)
/// - Stateless providers with per-operation user_id passing
/// - SessionState extraction for DataFusion integration
pub const PROVIDER_CONSOLIDATION_VERSION: &str = "13.0.0";
