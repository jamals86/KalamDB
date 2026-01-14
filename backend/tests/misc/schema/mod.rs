//! Schema Management Tests
//!
//! Tests covering:
//! - ALTER TABLE operations
//! - Schema cache invalidation
//! - Column ID stability
//! - Column ordering
//! - Schema consolidation
//! - Unified type system

#[path = "../../common/mod.rs"]
#[allow(dead_code)]
pub(super) mod test_support;

// Schema Tests
mod test_alter_table;
mod test_alter_table_after_flush;
mod test_column_id_stability;
mod test_column_ordering;
mod test_schema_cache_invalidation;
mod test_schema_consolidation;
mod test_unified_types;
