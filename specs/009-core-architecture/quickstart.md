# Quickstart (009): Core Architecture Refactor

This guide shows how to validate the refactor once implemented. It does not change runtime behavior; it ensures routes go through providers, handlers, and the unified JobManager.

## Prerequisites

- Rust stable (1.90+) and Cargo
- macOS/Linux

## Build and Test

- Build workspace: cargo build
- Run all tests: cargo test

Targets to watch:
- backend/crates/kalamdb-core (unit tests)
- backend (integration tests)
- cli and link crates

## Smoke: Provider-only access

- Grep the workspace for forbidden symbols:
  - `StorageAdapter` (0 matches)
  - `kalamdb_sql` imports (0 matches)

- Run backend tests that exercise CRUD on user/shared/stream tables. All should pass.

## Smoke: SqlExecutor handlers

- Execute DDL/DML statements in tests (see backend/tests):
  - CREATE/DROP/ALTER TABLE should hit DDL handler functions
  - SELECT/INSERT/UPDATE/DELETE should route to table stores via AppContext

## Unified Job Management smoke

- Create a flush job via the API/SQL (once wired):
  - Confirm system.jobs shows `New -> Queued -> Running -> Completed`
  - Confirm job id like `FL-xxxxx` and jobs.log entries prefixed with `[FL-xxxxx]`

- Create a job with an idempotency key and attempt creating the same again:
  - Expect “Job already running” error while active

- Simulate a failure and confirm `Retrying` then `Failed` after max attempts.

## Troubleshooting

- If tests fail due to missing fields in system.jobs, verify BC mapping:
  - `message` replaces `result`/`error_message`
  - `parameters` is an object; legacy arrays are auto-mapped on read

- If schema lookups are slow, verify handler uses SchemaRegistry (cache-first).
