//! System Column Management Service
//!
//! Re-exported from `kalamdb-registry` crate.
//!
//! **Phase 12, User Story 5 - MVCC Architecture**:
//! Centralizes all system column logic (`_seq`, `_deleted`) for KalamDB tables.

pub use kalamdb_registry::SystemColumnsService;
