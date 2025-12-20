# KalamDB Benchmark Suite

Comprehensive performance benchmarking system for KalamDB.

## Overview

This benchmark suite measures KalamDB performance across multiple dimensions:
- **User Tables**: Full DML operations (INSERT, SELECT, UPDATE, DELETE)
- **Shared Tables**: Multi-user data access patterns
- **Stream Tables**: Time-series data with TTL
- **System Tables**: Metadata query performance
- **Concurrency**: Multi-user load testing

## Quick Start

### Running All Benchmarks

All tests from a single run are written to **one JSON file**:

```bash
cd benchmark
cargo test --release
# Creates: bench-0.2.0--012-full-dml-support-a56f5d0.json
```

### Running Specific Test Groups

```bash
# User table tests only (appends to same file)
cargo test --release user_table

# Shared table tests (appends to same file)
cargo test --release shared_table

# System tables (appends to same file)
cargo test --release system_tables

# Concurrency tests (appends to same file)
cargo test --release concurrency
```

### Running Individual Tests

```bash
# Single test (creates/appends to file)
cargo test --release user_table_insert_1

# Specific pattern (all append to same file)
cargo test --release insert_50k
```

**File Format**: `bench-{version}--{branch}-{commit}.json`

The filename includes the git commit hash, so all tests for the same commit append to the same file. Making code changes and committing creates a new file for comparison.

## Viewing Results

After running benchmarks:

```bash
# Start the web viewer
cd benchmark
cargo run --bin benchmark-viewer --release

# Or use the script
.\view-results.ps1  # Windows
./view-results.sh   # Linux/macOS
```

Then open http://localhost:3030 in your browser.

Results are automatically loaded from `view/results/` and displayed in interactive tables.

## Directory Structure

```
benchmark/
├── Cargo.toml              # Benchmark crate configuration
├── src/
│   ├── lib.rs              # Public API
│   ├── models.rs           # Type-safe JSON models
│   └── helpers.rs          # Measurement utilities
├── tests/
│   ├── user_table/         # User table benchmarks (12 tests)
│   ├── shared_table/       # Shared table benchmarks
│   ├── stream_table/       # Stream table benchmarks
│   ├── system_tables/      # System table benchmarks
│   ├── concurrency/        # Concurrency benchmarks
│   └── TEST_TEMPLATES.md   # Implementation guide
└── view/                   # HTML viewer
    ├── index.html          # Auto-loads all results
    ├── README.md
    └── results/            # JSON output files
        └── README.md
```

## Test Categories

### User Table Tests (12 tests)
- **INSERT**: 1 / 100 / 50K rows
- **SELECT Hot**: 1 / 100 / 50K rows (from RocksDB)
- **SELECT Cold**: 1 / 100 / 50K rows (from Parquet)
- **UPDATE**: 1 / 100 / 50K rows
- **DELETE**: 1 / 100 / 50K rows

### Shared Table Tests (12 tests)
Same as User Table (no RLS overhead)

### Stream Table Tests (6 tests)
- **INSERT**: 1 / 100 / 50K rows
- **SELECT**: 1 / 100 / 50K rows (within TTL window)

### System Tables (4 tests)
- SELECT * FROM system.tables
- SELECT * FROM system.jobs
- SELECT * FROM system.users
- SELECT * FROM system.schemas

### Concurrency Tests (3 tests)
- 1 / 100 / 50K concurrent users

## Metrics Collected

Each test measures:

| Metric | Description |
|--------|-------------|
| `cli_total_time_ms` | End-to-end CLI execution time |
| `api_roundtrip_ms` | Network + API processing time |
| `sql_execution_ms` | Database query execution time |
| `requests` | Number of API requests made |
| `avg_request_ms` | Average time per request |
| `memory_before_mb` | Process memory before test |
| `memory_after_mb` | Process memory after test |
| `disk_before_mb` | Storage size before test |
| `disk_after_mb` | Storage size after test |
| `errors` | List of errors encountered |

## Architecture

### Type-Safe Models

All benchmark data uses Rust structs with serde serialization:

```rust
pub struct BenchmarkResult {
    pub meta: BenchmarkMeta,
    pub tests: Vec<TestResult>,
}

pub struct TestResult {
    pub id: String,
    pub group: TestGroup,
    pub subcategory: String,
    pub description: String,
    pub cli_total_time_ms: f64,
    pub api_roundtrip_ms: f64,
    pub sql_execution_ms: f64,
    // ... more fields
}
```

### Helper Functions

Common operations are centralized:

- `measure_memory_mb()`: OS-specific memory measurement
- `measure_disk_mb()`: Directory size calculation
- `execute_cli_timed()`: CLI wrapper with timing
- `setup_benchmark_tables()`: Test table creation
- `cleanup_benchmark_tables()`: Teardown

### Test Pattern

Every test follows this structure:

1. Setup: Create benchmark tables
2. Measure: Record baseline metrics
3. Execute: Run SQL via CLI
4. Measure: Record post-execution metrics
5. Result: Create and validate TestResult
6. Write: Append to JSON file
7. Cleanup: Drop benchmark tables

## JSON Output

All tests from a single run are written to one file in `view/results/`:

**Filename**: `bench-<version>--<branch>-<commit>.json`

Example: `bench-0.2.0--012-full-dml-support-a56f5d0.json`

The commit hash ensures:
- All tests for the same commit append to one file
- Different commits create separate files
- Easy tracking of performance across code changes

```json
{
  "meta": {
    "version": "0.2.0",
    "branch": "012-full-dml-support",
    "timestamp": "2025-11-14T15:30:00Z",
    "machine": {
      "cpu": "AMD Ryzen 9 7950X",
      "os": "windows x86_64",
      "memory_total_mb": 32768,
      "memory_free_mb": 16384,
      "disk_type": "Unknown",
      "disk_free_mb": 500000
    }
  },
  "tests": [
    { "id": "USR_INS_1", ... },
    { "id": "USR_INS_100", ... },
    { "id": "USR_SEL_HOT_1", ... }
    // All tests from this run
  ]
}
```

## Implementation Status

✅ **Completed**:
- Core infrastructure (models, helpers)
- User table tests (12 tests)
- Sample tests for other groups
- HTML viewer with Tabulator.js
- Documentation

📝 **TODO**:
- Complete remaining shared table tests (9 tests)
- Complete stream table tests (3 tests)
- Enhanced concurrency tests with threading
- Flush performance tests
- Repetitive query stability tests
- Storage bloat tests

See `tests/TEST_TEMPLATES.md` for implementation guide.

## Requirements

- Rust 1.92+
- Running KalamDB server on `localhost:8080`
- `kalam` CLI binary in PATH

## CI Integration

Add to your CI pipeline:

```yaml
- name: Run Benchmarks
  run: |
    cd benchmark
    cargo test --release
    
- name: Upload Results
  uses: actions/upload-artifact@v3
  with:
    name: benchmark-results
    path: benchmark/results/*.json
```

## Performance Baselines

Expected ranges (for reference):

| Operation | Hot Storage | Cold Storage |
|-----------|-------------|--------------|
| INSERT 1 | < 20ms | N/A |
| SELECT 1 | < 15ms | < 30ms |
| UPDATE 1 | < 25ms | N/A |
| DELETE 1 | < 20ms | N/A |

## Troubleshooting

**Tests failing with connection errors**:
- Ensure KalamDB server is running on port 8080
- Check `backend/config.toml` configuration

**Memory measurement returns 0**:
- Platform-specific implementation may need adjustment
- Check `src/helpers.rs` for your OS

**JSON files not created**:
- Verify `results/` directory exists
- Check file permissions

## Contributing

When adding new benchmarks:

1. Create test file in appropriate group directory
2. Add test entry to `Cargo.toml`
3. Follow existing test pattern
4. Update documentation
5. Run test to verify JSON output

## License

Same as KalamDB project.
