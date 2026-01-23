# Miscellaneous Integration Tests

This folder contains integration tests that use the `TestServer` infrastructure from `../common/` but are not currently enabled in the default test suite.
Some non-TestServer unit tests have been moved into their owning crates' `tests/` directories.

All tests in this folder reference the shared test infrastructure via:
```rust
#[path = "../common/testserver/mod.rs"]
mod test_support;
```

## Test Status

Most of these tests are **disabled** or **deprecated** for various reasons:

### Authentication & Authorization Tests
- `test_basic_auth.rs` - Basic auth tests (disabled: legacy middleware)
- `test_jwt_auth.rs` - JWT token tests
- `test_oauth.rs` - OAuth integration tests
- `test_rbac.rs` - Role-based access control
- `test_as_user_impersonation.rs` - User impersonation
- `test_cli_auth.rs` - CLI authentication
- `test_e2e_auth_flow.rs` - End-to-end auth flows
- `test_password_complexity.rs` - Password complexity rules
- `test_auth_performance.rs` - Auth performance benchmarks

### Schema & DDL Tests
- `test_alter_table.rs` - ALTER TABLE operations
- `test_alter_table_after_flush.rs` - ALTER TABLE after flush
- `test_column_id_stability.rs` - Column ID stability
- `test_column_ordering.rs` - Column ordering
- `test_schema_cache_invalidation.rs` - Schema cache invalidation

### Data Management Tests
- `test_soft_delete.rs` - Soft delete functionality
- `test_update_delete_version_resolution.rs` - MVCC version resolution
- `test_mvcc_phase2.rs` - MVCC phase 2
- `test_dml_complex.rs` - Complex DML operations

### Manifest & Flush Tests
- `test_manifest_flush_integration.rs` - Manifest flush integration (enabled)
- `test_cold_storage_manifest.rs` - Cold storage manifest

### Live Queries Tests
- `test_live_queries_auth_expiry.rs` - Live query auth expiry
- `test_live_queries_metadata.rs` - Live query metadata

### System & Performance Tests
- `test_system_users.rs` - System user management
- `test_system_user_init.rs` - System user initialization
- `test_production_concurrency.rs` - Production concurrency
- `test_production_validation.rs` - Production validation
- `test_pk_index_efficiency.rs` - Primary key index efficiency
- `test_row_count_behavior.rs` - Row count behavior (enabled)
- `test_combined_data_integrity.rs` - Data integrity

### Access Control Tests
- `test_shared_access.rs` - Shared table access
- `test_audit_logging.rs` - Audit logging
- `test_last_seen.rs` - Last seen tracking

### Storage & Eviction Tests
- `test_stream_ttl_eviction.rs` - Stream TTL eviction
- `test_stream_ttl.sql` - Stream TTL SQL

### Other Tests
- `test_datafusion_commands.rs` - DataFusion commands
- `test_edge_cases.rs` - Edge cases (disabled: deprecated patterns)

## Currently Enabled Tests

Only these tests in `misc/` are enabled in `Cargo.toml`:
- `test_manifest_flush_integration.rs`
- `test_row_count_behavior.rs`

## Migration Status

Most tests in this folder should be:
1. **Migrated to testserver/** - Tests that use HTTP APIs should be rewritten to use `HttpTestServer`
2. **Updated to new patterns** - Tests using deprecated `TestServer` should be updated to current patterns
3. **Removed** - Tests that are redundant or no longer needed

## Active Test Suites

For active, maintained tests, see:
- `../testserver/` - HTTP API integration tests (actively maintained)
- `../common/` - Shared test infrastructure for both TestServer and HttpTestServer
  - `../common/testserver/` - HTTP test server utilities (HttpTestServer, flush helpers, etc.)
