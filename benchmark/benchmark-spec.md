# KalamDB Benchmark Specification

This specification defines the complete structure, rules, and grouping model for all benchmark tests in KalamDB.

All tests are executed **exclusively via the CLI tool** and SQL queries. Benchmarks target the **local filesystem only** (no cloud storage). Each test tracks **memory growth** and **disk usage growth**.

---

## 1. Benchmark Output JSON Model

Each run produces a file:

```
bench-<version>--<branch>-<yyyy-mm-dd>-<index>.json
```

### JSON Structure

```json
{
  "meta": {
    "version": "0.2.0",
    "branch": "main",
    "timestamp": "2025-01-10T12:00:00Z",
    "machine": {
      "cpu": "AMD Ryzen 9 7950X",
      "os": "Linux 6.5",
      "memory_total_mb": 32768,
      "memory_free_mb": 21000,
      "disk_type": "NVMe",
      "disk_free_mb": 500000
    }
  },
  "tests": [
    {
      "id": "USR_INSERT_1",
      "group": "user_table",
      "subcategory": "insert",
      "description": "Insert 1 row into user table",
      "cli_total_time_ms": 12.3,
      "api_roundtrip_ms": 8.7,
      "sql_execution_ms": 3.9,
      "requests": 1,
      "avg_request_ms": 8.7,
      "memory_before_mb": 120,
      "memory_after_mb": 122,
      "disk_before_mb": 15,
      "disk_after_mb": 15.1,
      "errors": []
    }
  ]
}
```

---

## 2. Test Grouping Model

### Top-Level Groups

1. **User Table**
2. **Shared Table**
3. **Stream Table**
4. **System Tables**
5. **Concurrency Tests** (special combined category)

Every test belongs to exactly one group.

---

## 3. Benchmark Categories & Test Cases

Below is the authoritative list of all tests.

---

# GROUP 1 — USER TABLE

## Inserts

* Insert 1 row
* Insert 100 rows
* Insert 50,000 rows (batch insert) Track memory & disk after each batch.

## Select (Hot Storage)

* SELECT 1 row (LIMIT 1)
* SELECT 100 rows
* SELECT 50,000 rows
* SELECT with WHERE condition
* SELECT with ORDER BY

## Select (Cold Storage)

* SELECT 1 row (after flush)

* SELECT 100 rows (after flush)

* SELECT 50,000 rows (after flush)

* SELECT with WHERE (after flush)

* SELECT with ORDER BY (after flush)

* SELECT 1 row (LIMIT 1)

* SELECT 100 rows

* SELECT 50,000 rows

* SELECT with WHERE condition

* SELECT with ORDER BY

## Updates

* Update 1 row
* Update 100 rows
* Update 50,000 rows (batched)

## Deletes

* Delete 1 row
* Delete 100 rows
* Delete 50,000 rows (batched) — soft delete

## Live Query

* Subscribe → Insert 100 rows
* Subscribe → Update 100 rows

---

# GROUP 2 — SHARED TABLE

Same structure as user table except live queries are **not tested here**.

## Inserts

* 1 / 100 / 50k

## Select

* 1 / 100 / 50k
* WHERE
* ORDER BY

## Updates

* 1 / 100 / 50k

## Deletes

* 1 / 100 / 50k

---

# GROUP 3 — STREAM TABLE

## Inserts

* Insert 1
* Insert 100
* Insert 50,000

## Select

* LIMIT 1
* LIMIT 100
* LIMIT 50,000 (if within TTL window)

## Subscriptions

* Subscribe while inserting

(No update/delete because stream tables are immutable except TTL.)

---

# GROUP 4 — SYSTEM TABLES

Simple read-only benchmarks:

* SELECT * FROM system.tables
* SELECT * FROM system.jobs
* SELECT * FROM system.users
* SELECT * FROM system.schemas

Measure:

* SQL execution time
* Memory baseline impact
* Disk should not grow

---

# GROUP 5 — CONCURRENCY TESTS

Simulate **1 / 100 / 50,000 concurrent users**. Each user runs a subscription while operations happen in parallel. Use thread pool sizes: **1 / 50 / 100 threads**.

### Concurrent Inserts

* N users subscribe
* Insert load runs in parallel

### Concurrent Updates

* Same pattern as inserts

### Concurrent Deletes

* Same pattern as inserts

Track:

* Latency under load
* Memory growth
* Disk growth
* WebSocket event backlog (if any)

---

## 6. Detailed Test Execution Model

Every test follows this template:

```
1) Record memory_before + disk_before
2) Execute CLI command
3) Parse CLI timing & metadata
4) Measure API roundtrip timing (CLI includes this)
5) Record memory_after + disk_after
6) Store result in JSON
```

All SQL queries must come from CLI:

```
kalam "INSERT INTO app.messages VALUES (...)"
```

---

## 7. Dataset & Table Creation Model

Each run must define:

* Namespaces: `bench_user`, `bench_shared`, `bench_stream`
* Tables:

  * `bench_user.items`
  * `bench_shared.items`
  * `bench_stream.events TTL 10`

Schema is fixed:

```
id BIGINT DEFAULT SNOWFLAKE_ID(),
value TEXT,
timestamp TIMESTAMP DEFAULT NOW()
```

---

## 8. Memory & Disk Measurement Strategy

### Memory

Use OS-specific commands:

* Linux: `/proc/self/statm`
* macOS: `vm_stat`
* Windows: process working set

### Disk

Measure size of:

* RocksDB CF for that table
* Parquet directory for that namespace

Use:

```
du -sm data/bench_user/items
```

---

## 9. Naming Convention for Test IDs

Example:

* `USR_INS_1`
* `USR_SEL_50K`
* `SHR_UPD_100`
* `STR_SUB_INS`
* `SYS_TABLES_SEL`
* `CONC_INS_100_USERS_50_THREADS`

---

# 10. UI Tool Specification (Benchmark Results Viewer)

This defines the UI that reads all benchmark JSON files and displays results clearly.

## UI Structure

The UI displays **one table per benchmark group**:

* User Table
* Shared Table
* Stream Table
* System Tables
* Concurrency Tests

Each group renders its own table.

## Columns Per Table

Each table contains these columns:

* **ID** — test id (e.g., USR_INS_1)
* **Subcategory** — insert/select/update/delete
* **Description** — human description
* **Timings** — displayed as a *3‑row stacked cell*
* **Memory** — displayed as a *2‑row stacked cell*
* **Disk** — displayed as a *2‑row stacked cell*
* **Requests** — number of API requests
* **Avg Request Time** — avg_request_ms
* **Errors** — list or empty

## Column Cell Layouts

### Timing Cell (3 rows)

```
CLI: <cli_total_time_ms> ms
API: <api_roundtrip_ms> ms
SQL: <sql_execution_ms> ms
```

### Memory Cell (2 rows)

```
Before: <memory_before_mb> MB
After:  <memory_after_mb> MB
```

### Disk Cell (2 rows)

```
Before: <disk_before_mb> MB
After:  <disk_after_mb> MB
```

## UI Features

* Select version → list all test files for that version
* Select multiple versions → compare side-by-side
* Group tabs (one per test group)
* Click row to expand raw JSON
* Sort by any metric (especially cli_total_time_ms)
* Color indicators:

  * 🟢 improvement (lower timing than previous version)
  * 🔴 regression (timing or memory/disk increased)
* Download table as CSV/JSON

## File Reader Logic

* UI scans directory for pattern:

  ```
  bench-*.json
  ```
* Parse all JSONs
* Group tests by `group`
* For each group: display table

## Version Comparison Behavior

If 2+ JSON files selected:

* Add additional columns for comparison (V1 vs V2)
* Show regression % for timings, memory, disk

---

## 11. Validation Rules

### Request Timing Validation

If **api_roundtrip_ms == 0** or **avg_request_ms == 0**, the UI must:

* Mark the test as **error** automatically.
* Add an entry to `errors` array: `"request_time_zero"`.
* Display the row in **red** as a failed test.

This guarantees that any broken measurement or failed CLI run is caught.

---

## 12. Additional Benchmark Categories (Advanced Regression Tests)

These additional tests MUST be included to catch deep or long‑term regressions.

### 12.1 Flush Performance Tests

* Time to flush hot → cold
* Disk write throughput during flush
* Memory spike during flush
* Parquet file count before/after flush

### 12.2 Repetitive Query Stability

Run the **same SELECT query 1,000 times**:

* Detect memory leaks
* Validate schema cache correctness
* Ensure no performance degradation over repetitions

### 12.3 Fragmentation / Storage Bloat Test

Sequence:

1. Insert 50k
2. Delete 50k
3. Insert 50k
4. Delete 50k

Measure:

* Disk bloat (should stay controlled)
* RocksDB compaction behavior
* Memory fragmentation

### 12.4 Server Startup Benchmark

After heavy load, restart server and measure:

* Cold boot time
* Time to first successful SQL query
* Memory usage after startup

### 12.5 Payload Size Stress Tests

Insert/select using:

* TEXT 1KB
* TEXT 100KB
* TEXT 1MB

Measure:

* Insert latency
* Hot read latency
* Cold read latency
* Memory usage

---

## 13. Benchmark Code Architecture Requirements

When implementing benchmark tests:

### 13.1 Clean Helpers & Architecture

* All helper functions MUST be placed in a clean shared module.
* No duplicated logic between tests.
* Any CLI wrapper, disk/memory reader, JSON writer must be centralized.

### 13.2 Test File Structure

Each test must have **its own file**. Each group must be structured as **a folder**:

```
/benchmarks/
  /user_table/
    insert_1.rs
    insert_100.rs
    select_hot_1.rs
    ...
  /shared_table/
    ...
  /stream_table/
    ...
  /system_tables/
    ...
  /concurrency/
    ...
```

### 13.3 Separation of Concerns

* Test files only describe the test.
* Helper layer handles:

  * running CLI
  * parsing output
  * measuring memory/disk
  * writing JSON
* No test file should contain measurement logic.

### 13.4 Deterministic Test Data

* Every run must recreate tables.
* No leftover files.
* No random data except where required.

---

# 14. UI Mockup (Visual Wireframe)

Below is the visual mockup of the benchmark results UI. This is **not code**, but a structured layout showing how the interface should look.

---

# 🖥️ UI MOCKUP — BENCHMARK RESULTS VIEWER

```
┌───────────────────────────────────────────────────────────────┐
│  KalamDB Benchmark Viewer                                     │
├───────────────────────────────────────────────────────────────┤
│ Version: [ v0.2.0 ▼ ]   Compare With: [ v0.1.9 ▼ ]            │
│ JSON Files Loaded: bench-v0.2.0--main-2025-01-10-1.json ...   │
└───────────────────────────────────────────────────────────────┘

Tabs:
[ User Table ]  [ Shared Table ]  [ Stream Table ]  [ System Tables ]  [ Concurrency ]
--------------------------------------------------------------------------------------

ACTIVE TAB: User Table

Table Layout:
──────────────────────────────────────────────────────────────────────────────────────────
|  ID        | Subcategory | Description              | Timings               | Memory   |
|            |             |                          | CLI/API/SQL           | Before/After|
|            |             |                          |                       | Disk     |
──────────────────────────────────────────────────────────────────────────────────────────
| USR_INS_1  | insert      | Insert 1 row             | CLI: 12ms             | Mem:     |
|            |             |                          | API: 8ms              | 120 →122  |
|            |             |                          | SQL: 3ms              | Disk:     |
|            |             |                          |                       | 15 →15.1  |
|            |             |                          | Requests: 1           |          |
──────────────────────────────────────────────────────────────────────────────────────────
| USR_SEL_HOT_100                                             Regression: 🔴 +5%        |
| (row expands on click)                                                                   |
──────────────────────────────────────────────────────────────────────────────────────────
| ...                                                                                      |
──────────────────────────────────────────────────────────────────────────────────────────

Row Expansion (when clicked):
──────────────────────────────────────────────────────────────────────────────────────────
| Raw JSON:                                                                                 |
| {                                                                                         |
|   "id": "USR_SEL_HOT_100",                                                               |
|   "group": "user_table",                                                                 |
|   "cli_total_time_ms": 15.2,                                                             |
|   "api_roundtrip_ms": 12.1,                                                              |
|   "sql_execution_ms": 3.0,                                                               |
|   ...                                                                                     |
| }                                                                                         |
──────────────────────────────────────────────────────────────────────────────────────────

Version Comparison (when two versions selected):
──────────────────────────────────────────────────────────────────────────────────────────
| ID            | v0.2.0 Time | v0.1.9 Time | Change | Memory Change | Disk Change       |
------------------------------------------------------------------------------------------
| USR_INS_1     | 12ms        | 14ms        | 🟢 -14% | +1MB          | +0.1MB            |
| USR_INS_100   | 40ms        | 38ms        | 🔴 +5%  | 0MB           | 0MB               |
──────────────────────────────────────────────────────────────────────────────────────────
```

---

# Key Visual Rules

### Colors

* **🟢 green** → improvement
* **🔴 red** → regression
* **🟡 yellow** → neutral change
* **Row with errors** → full red background

### Stack Cells (multi‑row inside cell)

Timing:

```
CLI: 12ms
API: 8ms
SQL: 3ms
```

Memory:

```
120 → 122 MB
```

Disk:

```
15 → 15.1 MB
```

### Sorting

Columns sortable:

* ID
* cli_total_time_ms
* avg_request_ms
* memory_after_mb
* disk_after_mb
* error count

### Filtering

Filter bar:

```
[ search by text ] [ only regressions ] [ only errors ] [ hotspot tests ]
```

---

# 15. Embedded HTML Viewer Specification (Static No‑Backend UI)

The benchmark results viewer **must be implemented as a single self‑contained HTML file** with minimal JavaScript. No backend server is required.

## 15.1 Technology Requirements

* **One HTML file only** (no build step, no bundler)
* **Pure JavaScript** + **Tabulator.js** (recommended)
* **CSS embedded** in `<style>` tag
* No external backend — everything done client‑side
* Must work by simply doing: `double‑click → open in browser`

## 15.2 File Loading Requirements

* User can load JSONs via:

  * Drag & drop into the page
  * A file `<input>` selector
* Multiple JSON files can be loaded at once
* JSONs are parsed into memory only (nothing written to disk)
* All processing, grouping, comparison, coloring happens in browser

## 15.3 UI Behavior Requirements

* Render all benchmark groups in separate tabs
* Render each group's data table using Tabulator.js
* Support stacked cells for Timing, Memory, Disk
* Support expandable row to show raw JSON
* When two versions are selected → show comparison mode
* All logic must run inside the HTML/JS file
* Must work fully offline

## 15.4 Libraries Allowed

The HTML viewer uses:

* **Tabulator.js** (from CDN)
* **Minimal vanilla JS** (no frameworks)

Example include:

```html
<script src="https://cdn.jsdelivr.net/npm/tabulator-tables@5.5.0/dist/js/tabulator.min.js"></script>
```

## 15.5 Folder Structure

Only two files:

```
benchmark_viewer.html
(optional) benchmark_viewer.css  (but recommended to embed CSS inside HTML)
```

## 15.6 Implementation Notes for AI Agent

* The VS Code AI agent must generate a self‑contained HTML file
* Include JS logic for:

  * reading benchmark JSON files
  * grouping tests by `group`
  * building tables per group
  * computing regressions
  * coloring cells/rows
  * handling version comparisons
* The HTML must reflect the mockup defined earlier
* Keep the code clean and well‑commented

---

## 15.7 Percentage Change & Priority Highlighting Rules

For each test row, the UI must compute **percentage change** between two versions for:

* Performance (cli_total_time_ms)
* Memory (memory_after_mb)
* Disk (disk_after_mb)

### 15.7.1 Percentage Formula

```
percentage = ((new_value - old_value) / old_value) * 100
```

### 15.7.2 Priority Thresholds

The UI must apply **priority coloring** based on the size of the change:

#### Performance (Lower is better)

* **🟢 Green (Good)** → ≥ 5% improvement
* **🟡 Yellow (Neutral)** → change between -5% and +5%
* **🔴 Red (Critical Regression)** → ≥ 5% slower

#### Memory (Lower is better)

* **🟢 Green** → ≥ 5% memory reduction
* **🟡 Yellow** → minor change (<5%)
* **🔴 Red** → ≥ 5% increase in memory usage

#### Disk (Lower is better)

* **🟢 Green** → ≥ 5% disk reduction
* **🟡 Yellow** → minor change (<5%)
* **🔴 Red** → ≥ 5% increase in disk usage

### 15.7.3 Cell Display Format

The table must display percentage change inside the same cell:

```
12ms → 10ms  (🟢 -16.6%)
122MB → 130MB (🔴 +6.5%)
15MB → 15.2MB (🟡 +1.3%)
```

### 15.7.4 Row Priority

If **any** metric shows a red regression:

* Entire row gets **light red background**
* A "priority" label must appear:

  * 🔥 **Critical** → red regression
  * ⚠️ **Warning** → yellow
  * ✅ **OK** → all green or neutral

---

# 16. Final Notes

* Embedded HTML viewer specification added.

* Viewer requires no backend and works offline.

* Perfect for GitHub Pages, local testing, CI artifacts, etc.

* All tests are repeatable and deterministic.

* Memory/disk must be measured before & after every test.

* All results stored in a single JSON file per run.

* All tables are recreated before each run to ensure reproducibility.

* UI mockup now included.

* Ready for implementation as web UI, Tauri desktop app, or CLI TUI.

* Supports version comparison and deep analysis of regressions.

* All tests are repeatable and deterministic.

* All timing collected from CLI so benchmark does not depend on internal metrics.

* Memory/disk must be measured before & after every test.

* All results stored in a single JSON file per run.

* All tables are recreated before each run to ensure reproducibility.
