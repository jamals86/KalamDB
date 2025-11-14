# Running Benchmarks

## Quick Start

### Run All Benchmarks

```powershell
# Use the helper script (recommended)
.\run-benchmarks.ps1

# Or run manually by category
cargo test --release user_table
cargo test --release shared_table  
cargo test --release stream_table
cargo test --release system_tables
cargo test --release concurrency
```

### Run Individual Tests

```powershell
cargo test --release --test user_table_insert_1
cargo test --release --test shared_table_select_100
```

## Output

All tests append to **one JSON file** per commit:

```
view/results/bench-0.2.0--012-full-dml-support-a56f5d0.json
```

Format: `bench-{version}--{branch}-{commit}.json`

### Checking Results

```powershell
# View the latest results file
$file = Get-ChildItem view\results\bench-*.json | Sort-Object LastWriteTime -Descending | Select-Object -First 1
Get-Content $file.FullName | ConvertFrom-Json | Select-Object -ExpandProperty tests | Format-Table id, subcategory, cli_total_time_ms

# Count tests in file
(Get-Content $file.FullName | ConvertFrom-Json).tests.Count
```

## Viewing in Browser

```powershell
cargo run --bin benchmark-viewer --release
# Open: http://localhost:3030
```

The viewer auto-loads all JSON files and shows historical comparisons.

## Important Notes

- **Don't run `cargo test` without arguments** - it tries to run all test binaries and may fail
- **Run tests by category** (user_table, shared_table, etc.) or individually
- **Server must be running** on port 8080 before running benchmarks
- **All tests for same commit** append to the same file
- **New commit** creates a new file
