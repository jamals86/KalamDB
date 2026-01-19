//! Miscellaneous integration tests for KalamDB.
//!
//! These tests cover various aspects of the database that don't fit into
//! specific categories like scenarios or table-specific tests.
//!
//! Test organization:
//! - **Auth Tests**: Authentication, authorization, RBAC, password security (auth/)
//! - **Schema Tests**: Schema management, cache invalidation, column ordering (schema/)
//! - **Storage Tests**: Manifest, cold storage, flush integration (storage/)
//! - **SQL Tests**: DML, DDL, data types, edge cases (sql/)
//! - **System Tests**: System tables, audit logging, configuration (system/)
//! - **Production Tests**: End-to-end flows, MVCC, production validation (production/)
//!
//! Tests are now organized in subdirectories. Use test_misc.rs driver to run all tests.

// Test modules organized by category
pub mod auth;
pub mod schema;
pub mod storage;
pub mod sql;
pub mod system;
pub mod production;
