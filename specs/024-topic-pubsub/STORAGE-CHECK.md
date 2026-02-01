# STORAGE CHECK - Storage Health & Connectivity Testing

## Overview

This specification defines the `STORAGE CHECK` command and underlying `StorageHealthService` that validates storage backend connectivity and provides health information. The service is used both for on-demand health checks via SQL and for automatic validation during `CREATE STORAGE` and `ALTER STORAGE` operations.

## SQL Syntax

```sql
-- Basic connectivity check
STORAGE CHECK <storage_id>;

-- Extended check with size information (when available)
STORAGE CHECK <storage_id> EXTENDED;
```

## Design Goals

1. **Fail-Fast on Create/Alter**: Validate storage connectivity BEFORE persisting storage metadata
2. **On-Demand Health Checks**: Allow administrators to verify storage health at any time
3. **Comprehensive Testing**: Test all CRUD operations (create dir, write, list, read, delete)
4. **Size Reporting**: Report storage capacity information when available from the backend

## Architecture

### StorageHealthService (kalamdb-filestore)

Located in `backend/crates/kalamdb-filestore/src/health/` with the following structure:

```
health/
├── mod.rs          # Module exports
├── models.rs       # Result types (StorageHealthResult, HealthStatus)
└── service.rs      # StorageHealthService implementation
```

### Health Check Sequence

The health check performs the following test sequence:

1. **Build ObjectStore**: Create ObjectStore from storage configuration
2. **Create Test Directory**: Create `.kalamdb-health-check/` directory
3. **Write Test File**: PUT a 16-byte test file with content `"KALAMDB_HEALTH\n"`
4. **List Files**: LIST files in the test directory to verify it appears
5. **Read Test File**: GET the test file and verify contents match
6. **Delete Test File**: DELETE the test file
7. **Cleanup Directory**: DELETE PREFIX to clean up test artifacts

Test file path format: `.kalamdb-health-check/{timestamp_ms}-{random_suffix}.tmp`

### Result Types

```rust
pub enum HealthStatus {
    Healthy,      // All tests passed
    Degraded,     // Some tests failed (e.g., write-only or read-only)
    Unreachable,  // Cannot connect to storage backend
}

pub struct StorageHealthResult {
    pub status: HealthStatus,
    pub readable: bool,
    pub writable: bool,
    pub listable: bool,
    pub deletable: bool,
    pub latency_ms: u64,
    pub total_bytes: Option<u64>,   // Available for some backends
    pub used_bytes: Option<u64>,    // Available for some backends
    pub error: Option<String>,
    pub tested_at: i64,             // Unix timestamp
}
```

## Integration Points

### CREATE STORAGE Handler

Before persisting storage to `system.storages`:
1. Build `Storage` struct from statement
2. Call `StorageHealthService::run_full_health_check(&storage).await`
3. If any test fails → return error with details, do NOT persist
4. If all tests pass → proceed with `applier.create_storage()`

### ALTER STORAGE Handler

Before updating storage in `system.storages`:
1. Apply modifications to storage struct
2. Call `StorageHealthService::run_full_health_check(&modified_storage).await`
3. If any test fails → return error, do NOT update
4. If all tests pass → proceed with `update_storage()`

### STORAGE CHECK SQL Command

Handler flow:
1. Lookup storage by ID from `StoragesTableProvider`
2. If not found → return error "Storage not found"
3. Run health check (basic or extended based on `EXTENDED` flag)
4. Return results as a single-row table

## Output Format

The `STORAGE CHECK` command returns a single-row result table:

| Column | Type | Description |
|--------|------|-------------|
| storage_id | VARCHAR | Storage identifier |
| status | VARCHAR | "healthy", "degraded", or "unreachable" |
| readable | BOOLEAN | True if read test passed |
| writable | BOOLEAN | True if write test passed |
| listable | BOOLEAN | True if list test passed |
| deletable | BOOLEAN | True if delete test passed |
| latency_ms | BIGINT | Total operation latency in milliseconds |
| total_bytes | BIGINT | Total storage capacity (NULL if unavailable) |
| used_bytes | BIGINT | Used storage space (NULL if unavailable) |
| error | VARCHAR | Error message if any test failed (NULL otherwise) |
| tested_at | TIMESTAMP | When the health check was performed |

## Authorization

- `STORAGE CHECK` requires `dba` or `system` role (same as CREATE/ALTER STORAGE)
- Regular users cannot check storage health

## Error Handling

### On Connectivity Failure

```
Error: Storage connectivity test failed for 'my_s3_storage':
  - PUT failed: Connection refused (endpoint: http://localhost:9000)
  
Storage was NOT created. Please verify your storage configuration and try again.
```

### Health Check Timeout

- Default timeout: 30 seconds per operation
- Total check timeout: 60 seconds
- On timeout: Mark as `Unreachable` with timeout error message

## Test Scenarios

### Unit Tests (kalamdb-filestore)

1. `test_health_check_local_filesystem` - Healthy local storage
2. `test_health_check_missing_directory` - Directory doesn't exist
3. `test_health_check_no_write_permission` - Read-only filesystem
4. `test_health_check_result_structure` - Verify all fields populated

### Integration Tests (CLI)

1. `test_storage_check_local` - Check healthy local storage
2. `test_storage_check_invalid_storage` - Check non-existent storage
3. `test_create_storage_connectivity_failure` - Create with bad endpoint
4. `test_alter_storage_connectivity_failure` - Alter to bad config

## Future Enhancements

1. **UI Health Button**: Add "Check Health" button per storage row in Admin UI
2. **Background Health Monitoring**: Periodic health checks with alerting
3. **Size Reporting for Cloud**: Implement bucket size APIs for S3/GCS/Azure

## Implementation Checklist

- [x] Create `backend/crates/kalamdb-filestore/src/health/mod.rs`
- [x] Create `backend/crates/kalamdb-filestore/src/health/models.rs`
- [x] Create `backend/crates/kalamdb-filestore/src/health/service.rs`
- [x] Add `HealthCheckFailed` error variant to `FilestoreError`
- [x] Re-export health module from `lib.rs`
- [x] Add `StorageCheckStatement` to `kalamdb-sql/src/ddl/`
- [x] Add parser support for `STORAGE CHECK` syntax
- [x] Create `CheckStorageHandler` in `kalamdb-core`
- [x] Integrate health check into `CreateStorageHandler`
- [x] Integrate health check into `AlterStorageHandler`
- [x] Add unit tests for `StorageHealthService`
- [x] Add CLI integration tests
- [x] Update documentation
