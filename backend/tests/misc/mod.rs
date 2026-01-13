//! Miscellaneous integration tests for KalamDB.
//!
//! These tests cover various aspects of the database that don't fit into
//! specific categories like scenarios or table-specific tests.
//!
//! Test organization:
//! - **Auth Tests**: Authentication, authorization, RBAC, password security
//! - **Schema Tests**: Schema management, cache invalidation, column ordering
//! - **Storage Tests**: Manifest, cold storage, flush integration
//! - **SQL Tests**: DML, DDL, data types, edge cases
//! - **System Tests**: System tables, audit logging, configuration
//! - **Integration Tests**: End-to-end flows, MVCC, production validation

#[path = "../common/testserver/mod.rs"]
#[allow(dead_code)]
mod test_support;

// Auth & Security Tests
mod test_as_user_impersonation;
mod test_auth_performance;
mod test_basic_auth;
mod test_cli_auth;
mod test_e2e_auth_flow;
mod test_jwt_auth;
mod test_last_seen;
mod test_live_queries_auth_expiry;
mod test_oauth;
mod test_password_complexity;
mod test_password_security;
mod test_rbac;
mod test_soft_delete;

// Schema & Metadata Tests
mod test_alter_table;
mod test_alter_table_after_flush;
mod test_column_id_stability;
mod test_column_ordering;
mod test_schema_cache_invalidation;
mod test_schema_consolidation;
mod test_unified_types;

// Storage & Manifest Tests
mod test_cold_storage_manifest;
mod test_manifest_cache;
mod test_manifest_flush_integration;

// SQL & Data Tests
mod test_combined_data_integrity;
mod test_datafusion_commands;
mod test_datatypes_preservation;
mod test_datetime_timezone_storage;
mod test_dml_complex;
mod test_edge_cases;
mod test_explain_index_usage;
mod test_pk_index_efficiency;
mod test_row_count_behavior;
mod test_shared_access;
mod test_stream_ttl_eviction;
mod test_system_table_index_usage;
mod test_update_delete_version_resolution;

// System & Configuration Tests
mod test_audit_logging;
mod test_config_access;
mod test_live_queries_metadata;
mod test_system_users;
mod test_system_user_init;

// Production & Concurrency Tests
mod test_mvcc_phase2;
mod test_production_concurrency;
mod test_production_validation;
