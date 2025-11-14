# KalamDB Benchmark Results

This directory contains the JSON output files from benchmark test runs.

## Auto-Loading

When you start the benchmark viewer and open http://localhost:3030, all JSON files in this directory are **automatically discovered and loaded**.

No manual file selection needed - just start the viewer and results appear instantly!

## File Naming Convention

Files are named using the pattern:
```
bench-<version>--<branch>-<yyyy-mm-dd>-<index>.json
```

Examples:
- `bench-0.2.0--012-full-dml-support-2025-11-14-1.json`
- `bench-0.2.0--main-2025-11-14-2.json`

## Viewing Results

To view the benchmark results:

```bash
# Start the viewer
cd benchmark
cargo run --bin benchmark-viewer --release
```

Then open http://localhost:3030 in your browser.

Results from this directory load automatically and display in interactive tables organized by test group.

## JSON Structure

Each file contains:

```json
{
  "meta": {
    "version": "0.2.0",
    "branch": "012-full-dml-support",
    "timestamp": "2025-11-14T12:00:00Z",
    "machine": {
      "cpu": "...",
      "os": "...",
      "memory_total_mb": 32768,
      "memory_free_mb": 16384,
      "disk_type": "NVMe",
      "disk_free_mb": 500000
    }
  },
  "tests": [
    {
      "id": "USR_INS_1",
      "group": "user_table",
      "subcategory": "insert",
      "description": "Insert 1 row into user table",
      "cli_total_time_ms": 12.5,
      "api_roundtrip_ms": 8.2,
      "sql_execution_ms": 4.1,
      "requests": 1,
      "avg_request_ms": 8.2,
      "memory_before_mb": 120.5,
      "memory_after_mb": 121.2,
      "disk_before_mb": 15.0,
      "disk_after_mb": 15.1,
      "errors": []
    }
  ]
}
```

## Running Benchmarks

To generate new benchmark results:

```bash
cd benchmark
cargo test --release
```

This will:
1. Run all benchmark tests
2. Generate timing and resource usage data
3. Create/append to JSON files in this directory

## Notes

- Results are automatically appended to the daily benchmark file
- Each test run creates a new entry with the same version/branch/date
- Compare results across different versions using the HTML viewer
