//! Schema Management Tests
//!
//! Tests covering:
//! - ALTER TABLE operations
//! - Schema cache invalidation
//! - Column ID stability
//! - Column ordering
//! - Schema consolidation (moved to kalamdb-core tests)
//! - Unified type system (moved to kalamdb-commons tests)

// Re-export test_support from crate root
pub(super) use crate::test_support;

// Schema Tests
mod test_alter_table;
mod test_alter_table_after_flush;
mod test_column_id_stability;
mod test_column_ordering;
// mod test_schema_cache_invalidation; // Removed - file does not exist
