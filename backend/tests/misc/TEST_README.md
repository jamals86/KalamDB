# Test Execution Notes

## test_explain_index_usage

This test file contains unit tests for the common test utilities module, including tests that initialize the TestServer and Raft consensus layer.

### Known Issue: Parallel Execution

Due to the shared nature of the AppContext singleton and Raft initialization across all tests in this binary, **this test must be run with `--test-threads=1`** to avoid race conditions during Raft leadership election.

### Running This Test

```bash
# Correct way (serial execution)
cargo test --test test_explain_index_usage -- --test-threads=1

# This will fail with "Not leader for group meta" errors (parallel execution)
cargo test --test test_explain_index_usage
```

### Why Serial Execution?

The test binary includes both:
1. Unit tests from the `common` module (testing helper functions)
2. Integration tests that create TestServer instances

When tests run in parallel:
- Multiple tests may attempt to use the shared Raft cluster simultaneously
- The static `RAFT_INITIALIZED` flag prevents re-initialization
- But concurrent access during the initial setup phase causes leadership election issues

When tests run serially:
- The first test to create a TestServer properly initializes Raft
- Subsequent tests use the already-initialized and stable Raft cluster
- All tests pass successfully

### Test Results

- **Serial execution (`--test-threads=1`)**: ✅ 28/28 tests pass
- **Parallel execution (default)**: ❌ 22/28 tests pass, 6 fail with "Not leader" errors

### Impact

This does not affect:
- Production code (no parallel initialization issues)
- Other test files (all pass with parallel execution)
- CI/CD pipelines (can specify --test-threads=1 for this specific test)
