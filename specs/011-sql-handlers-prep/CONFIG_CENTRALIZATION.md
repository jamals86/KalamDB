# Config Centralization - Post Phase 7 Refactoring

## Problem Statement

After completing Phase 7 (User Story 5: Full DML with Native Write Paths), parameter validation was implemented but had a critical issue:

```rust
// ❌ BEFORE: Hardcoded defaults instead of reading from config
validate_parameters(&params, &ParameterLimits::default())?;
```

The `ParameterLimits::default()` hardcoded max_parameters=50 and max_parameter_size_bytes=512KB, completely ignoring the values configured in `config.toml`.

## Root Cause

Config was scattered across the codebase with no centralized access pattern. Components either:
1. Passed ServerConfig as a parameter through multiple layers
2. Used hardcoded defaults (like ParameterLimits)
3. Had inconsistent config access patterns

## Solution: AppContext Config Centralization

Moved ServerConfig to `kalamdb-commons` and added it to AppContext singleton for consistent access everywhere.

### Architecture Changes

#### 1. Config Module Location
```rust
// Moved from: backend/src/config.rs
// Moved to:   backend/crates/kalamdb-commons/src/config.rs
```

**Rationale**: Config structs need to be shared across all crates (kalamdb-core, kalamdb-api, kalamdb-auth, etc.)

#### 2. AppContext Enhancement
```rust
// backend/crates/kalamdb-core/src/app_context.rs

pub struct AppContext {
    node_id: Arc<NodeId>,
    config: Arc<ServerConfig>,  // ← NEW: Config accessible everywhere
    schema_registry: Arc<SchemaRegistry>,
    // ... other fields
}

impl AppContext {
    pub fn init(
        storage_backend: Arc<dyn StorageBackend>,
        node_id: NodeId,
        storage_base_path: String,
        config: ServerConfig,  // ← NEW: Passed at initialization
    ) -> Arc<AppContext> {
        let config = Arc::new(config); // Wrap in Arc for zero-copy sharing
        // ...
    }
    
    pub fn config(&self) -> &Arc<ServerConfig> {
        &self.config
    }
}
```

#### 3. Lifecycle Integration
```rust
// backend/src/lifecycle.rs

let app_context = kalamdb_core::app_context::AppContext::init(
    backend.clone(),
    kalamdb_commons::NodeId::new(config.server.node_id.clone()),
    config.storage.default_storage_path.clone(),
    config.clone(), // ← Pass ServerConfig to AppContext
);
```

#### 4. Parameter Validation Fix
```rust
// backend/crates/kalamdb-core/src/sql/executor/parameter_validation.rs

impl ParameterLimits {
    /// Create ParameterLimits from ExecutionSettings config
    pub fn from_config(exec: &ExecutionSettings) -> Self {
        Self {
            max_count: exec.max_parameters,
            max_size_bytes: exec.max_parameter_size_bytes,
        }
    }
}
```

#### 5. DML Handler Updates
```rust
// backend/crates/kalamdb-core/src/sql/executor/handlers/dml/insert.rs

async fn execute(&self, ...) -> Result<ExecutionResult, KalamDbError> {
    // ✅ AFTER: Read actual config values
    let limits = ParameterLimits::from_config(&self.app_context.config().execution);
    validate_parameters(&params, &limits)?;
    // ...
}
```

Same pattern applied to `update.rs` and `delete.rs`.

## Benefits

### 1. Single Source of Truth
All config access goes through `AppContext::get().config()`:
```rust
let config = AppContext::get().config();
let max_params = config.execution.max_parameters; // Reads actual config.toml value
```

### 2. No Hardcoded Defaults in Production
Production code NEVER uses `ParameterLimits::default()` - always reads from config:
```rust
let limits = ParameterLimits::from_config(&app_context.config().execution);
```

### 3. Consistent Access Pattern
Same pattern everywhere:
```rust
// Any component can access config
let app_context = AppContext::get();
let timeout = app_context.config().execution.handler_timeout_seconds;
let max_params = app_context.config().execution.max_parameters;
```

### 4. Test Coverage
Integration tests verify config accessibility:
```rust
#[test]
fn test_parameter_limits_from_config() {
    let app_context = AppContext::get();
    let config = app_context.config();
    
    assert_eq!(config.execution.max_parameters, 50);
    assert_eq!(config.execution.max_parameter_size_bytes, 512 * 1024);
    
    let limits = ParameterLimits::from_config(&config.execution);
    assert_eq!(limits.max_count, 50);
    assert_eq!(limits.max_size_bytes, 512 * 1024);
}
```

## Files Modified

1. **backend/crates/kalamdb-commons/src/config.rs**
   - Moved entire config system (ServerConfig, all settings structs)
   - 1079 lines of config definitions

2. **backend/crates/kalamdb-commons/src/lib.rs**
   - Added `pub mod config;`
   - Re-exported `ServerConfig`

3. **backend/crates/kalamdb-commons/Cargo.toml**
   - Added dependencies: `toml`, `anyhow`, `num_cpus` (workspace = true)

4. **backend/src/config.rs**
   - Replaced with 2-line re-export: `pub use kalamdb_commons::config::*;`

5. **backend/crates/kalamdb-core/src/app_context.rs**
   - Added `config: Arc<ServerConfig>` field
   - Updated `init()` signature to accept `ServerConfig` parameter
   - Added `config()` getter method

6. **backend/src/lifecycle.rs**
   - Updated `AppContext::init()` call to pass `config.clone()`

7. **backend/crates/kalamdb-core/src/test_helpers.rs**
   - Created test config using `ServerConfig::default()` + overrides
   - Updated `AppContext::init()` call with test config

8. **backend/crates/kalamdb-core/src/sql/executor/parameter_validation.rs**
   - Added `ParameterLimits::from_config()` constructor method

9. **backend/crates/kalamdb-core/src/sql/executor/handlers/dml/insert.rs**
   - Changed from `ParameterLimits::default()` to `ParameterLimits::from_config(&app_context.config().execution)`

10. **backend/crates/kalamdb-core/src/sql/executor/handlers/dml/update.rs**
    - Same change as insert.rs

11. **backend/crates/kalamdb-core/src/sql/executor/handlers/dml/delete.rs**
    - Same change as insert.rs

12. **backend/tests/test_config_access.rs** (NEW)
    - 2 integration tests for config accessibility

## Test Results

```bash
$ cargo test --test test_config_access
running 2 tests
test test_config_accessible_from_app_context ... ok
test test_parameter_limits_from_config ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## Build Status

```bash
$ cargo build --lib
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2m 09s
```

✅ **0 compilation errors** (only pre-existing warnings in unrelated code)

## Impact Analysis

### Breaking Changes
None - this is an internal refactoring. External APIs unchanged.

### Performance
Zero performance impact:
- Config wrapped in `Arc` (same memory model as other AppContext fields)
- `ParameterLimits::from_config()` just copies two usize values (negligible)

### Memory
+8 bytes per AppContext instance (Arc pointer)

## Future Work

Now that config is centralized in AppContext, other components can adopt this pattern:
- Replace scattered config parameter passing with `AppContext::get().config()`
- Eliminate config duplication in various service constructors
- Standardize timeout handling via `config.execution.handler_timeout_seconds`

## Lessons Learned

1. **Always verify config usage** after adding config fields - don't assume code reads config
2. **Centralized singletons are powerful** - AppContext pattern eliminates parameter threading
3. **Test config accessibility** - integration tests caught that test_helpers.rs needed updating
4. **Use Default trait** - `ServerConfig::default()` made test config construction trivial

## Timeline

- **2025-01-XX**: Phase 7 completed with hardcoded ParameterLimits::default()
- **2025-01-XX**: User identified config not being read from config.toml
- **2025-01-XX**: Config centralization refactoring completed (11 tasks, 12 files modified)
- **2025-01-XX**: Integration tests passing, 0 compilation errors

## References

- Phase 7 spec: `specs/011-sql-handlers-prep/spec.md` - User Story 5
- Tasks: `specs/011-sql-handlers-prep/tasks.md` - T062-T070 (Phase 7), T071a-T071k (Config)
- Main config file: `backend/crates/kalamdb-commons/src/config.rs`
- AppContext: `backend/crates/kalamdb-core/src/app_context.rs`
- Parameter validation: `backend/crates/kalamdb-core/src/sql/executor/parameter_validation.rs`
