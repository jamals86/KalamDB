//! Unified DML module for MVCC append-only operations
//!
//! **Phase 12, User Story 5**: This module provides the core DML functions used by both
//! user and shared tables to implement Multi-Version Concurrency Control (MVCC).
//!
//! All INSERT/UPDATE/DELETE operations are append-only - they never modify existing versions.
//! Version resolution uses MAX(_seq) per primary key with _deleted filtering.

mod append;
mod resolve;
mod validate;
mod key_gen;

pub use append::{append_version, append_version_sync};
pub use resolve::resolve_latest_version;
pub use validate::{validate_primary_key, extract_user_pk_value};
pub use key_gen::generate_storage_key;
