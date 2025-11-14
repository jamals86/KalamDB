# 🚀 KalamDB Benchmark Quick Reference

## Prerequisites
```bash
# 1. Start KalamDB server
cd backend
cargo run --release

# 2. Verify server is running
curl http://localhost:8080/health
```

## Running Benchmarks

### Quick Start (Recommended)
```powershell
# Windows
.\benchmark\run-benchmarks.ps1

# Linux/macOS
./benchmark/run-benchmarks.sh
```

### Manual Execution
```bash
cd benchmark

# All benchmarks
cargo test --release

# Specific group
cargo test --release user_table
cargo test --release shared_table
cargo test --release stream_table
cargo test --release system_tables
cargo test --release concurrency

# Single test
cargo test --release user_table_insert_1

# Show output
cargo test --release user_table_insert_1 -- --nocapture
```

## Viewing Results

### Start Web Viewer
```bash
# Start the viewer server
cd benchmark
cargo run --bin benchmark-viewer --release

# Or use quick script
.\view-results.ps1  # Windows
./view-results.sh   # Linux/macOS
```

### View Results
1. Start the viewer (see above)
2. Open http://localhost:3030 in browser
3. Results automatically load from `view/results/`
4. Click tabs to explore different test groups

## Understanding Results

### JSON Output Location
```
benchmark/results/bench-<version>--<branch>-<date>-<index>.json
```

### Key Metrics
| Metric | What It Measures |
|--------|------------------|
| `cli_total_time_ms` | Total CLI command time (includes network + overhead) |
| `api_roundtrip_ms` | Server processing time |
| `sql_execution_ms` | Database query execution time |
| `memory_before_mb` | Process memory before operation |
| `memory_after_mb` | Process memory after operation |
| `disk_before_mb` | Storage size before operation |
| `disk_after_mb` | Storage size after operation |
| `avg_request_ms` | Average latency per request |

### Performance Indicators
- 🟢 **Green**: Performance improvement
- 🔴 **Red**: Performance regression or error
- 🟡 **Yellow**: Neutral/minor change

## Adding New Tests

### 1. Create Test File
```bash
# Example: shared_table/select_100.rs
cd benchmark/tests/shared_table
```

### 2. Copy Template
```rust
use kalamdb_benchmark::*;
use std::time::Duration;

#[test]
fn shared_table_select_100() -> anyhow::Result<()> {
    setup_benchmark_tables()?;
    std::thread::sleep(Duration::from_millis(200));
    
    let mem_before = measure_memory_mb();
    let disk_before = measure_disk_mb("backend/data/rocksdb");
    
    let sql = "SELECT * FROM bench_shared.items LIMIT 100";
    let execution = execute_cli_timed_root(sql)?;
    
    let mem_after = measure_memory_mb();
    let disk_after = measure_disk_mb("backend/data/rocksdb");
    
    let mut result = TestResult::new(
        "SHR_SEL_100",
        TestGroup::SharedTable,
        "select",
        "SELECT 100 rows from shared table",
    );
    
    result.set_timings(execution.cli_total_ms, execution.server_time_ms, execution.server_time_ms);
    result.set_memory(mem_before, mem_after);
    result.set_disk(disk_before, disk_after);
    result.set_requests(1, execution.server_time_ms);
    result.validate();
    
    append_test_result("0.2.0", "012-full-dml-support", result)?;
    cleanup_benchmark_tables()?;
    
    Ok(())
}
```

### 3. Add to Cargo.toml
```toml
[[test]]
name = "shared_table_select_100"
path = "tests/shared_table/select_100.rs"
```

### 4. Run Test
```bash
cargo test --release shared_table_select_100
```

## Troubleshooting

### Server Not Running
```
❌ Error: Connection refused

Solution:
cd backend
cargo run --release
```

### Memory Measurement Returns 0
```
❌ Warning: memory_before_mb = 0.0

Cause: Platform-specific implementation
Check: src/helpers.rs for your OS
```

### JSON File Not Created
```
❌ No output file

Solutions:
1. Check results/ directory exists
2. Verify write permissions
3. Check disk space
```

### Tests Failing
```
❌ Test failed

Solutions:
1. Ensure server is running
2. Check server health endpoint
3. Verify database is accessible
4. Review test output for SQL errors
```

## Common Patterns

### Running Full Suite
```bash
cd benchmark
cargo test --release 2>&1 | tee benchmark-output.txt
```

### Running by Size
```bash
# Small tests (1 row)
cargo test --release -- insert_1 select_1 update_1 delete_1

# Medium tests (100 rows)
cargo test --release -- 100

# Large tests (50k rows)
cargo test --release -- 50k
```

### Running by Operation
```bash
# All inserts
cargo test --release -- insert

# All selects
cargo test --release -- select

# All updates
cargo test --release -- update

# All deletes
cargo test --release -- delete
```

## Performance Baselines

Expected ranges (reference):

| Operation | Expected Time | Notes |
|-----------|---------------|-------|
| INSERT 1 | < 20ms | Single row insert |
| SELECT 1 (hot) | < 15ms | From RocksDB |
| SELECT 1 (cold) | < 30ms | From Parquet |
| UPDATE 1 | < 25ms | Single row update |
| DELETE 1 | < 20ms | Soft delete |
| INSERT 100 | < 500ms | Batch of 100 |
| SELECT 100 (hot) | < 50ms | From RocksDB |
| INSERT 50k | < 30s | Batched inserts |

## Test Groups

### User Table (12 tests)
- Full DML operations
- Hot and cold storage
- RLS overhead included

### Shared Table (12 tests planned)
- Same as User Table
- No RLS overhead
- Multi-user access

### Stream Table (6 tests planned)
- INSERT and SELECT only
- TTL-based eviction
- Immutable data

### System Tables (4 tests)
- Read-only queries
- Metadata access
- Baseline performance

### Concurrency (3 tests)
- Multi-user scenarios
- Load testing
- Scalability benchmarks

## CI Integration

```yaml
# .github/workflows/benchmark.yml
name: Benchmarks
on: [push]

jobs:
  benchmark:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Start KalamDB
        run: |
          cd backend
          cargo run --release &
          sleep 5
      
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

## File Locations

```
benchmark/
├── run-benchmarks.ps1       # Quick start (Windows)
├── run-benchmarks.sh        # Quick start (Linux/macOS)
├── README.md                # Full documentation
├── IMPLEMENTATION_SUMMARY.md # Implementation details
├── results/                 # JSON output files
│   └── bench-*.json
├── view/
│   └── index.html           # HTML viewer
└── tests/
    ├── user_table/          # 12 tests
    ├── shared_table/        # 3 tests (+ templates)
    ├── stream_table/        # 3 tests (+ templates)
    ├── system_tables/       # 4 tests
    └── concurrency/         # 3 tests
```

## Quick Tips

1. **Always use --release**: Debug builds are 10-100x slower
2. **Run on idle system**: Close other apps for accurate results
3. **Multiple runs**: Run 3x and average for stability
4. **Compare consistently**: Same machine, same config
5. **Version tracking**: Keep JSON files for historical comparison

## Getting Help

- **Full docs**: `benchmark/README.md`
- **Test templates**: `benchmark/tests/TEST_TEMPLATES.md`
- **Implementation**: `benchmark/IMPLEMENTATION_SUMMARY.md`
- **Viewer guide**: `benchmark/view/README.md`
