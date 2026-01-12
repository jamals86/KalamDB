# HTTP Test Server Integration Tests

This folder contains integration tests that intentionally hit the real KalamDB HTTP API
over TCP (e.g. `POST /v1/api/sql`) using the near-production server wiring.

## Structure

- `../common/testserver/` contains shared helpers used by these tests (moved from `commons/`).
- `scenarios/` contains end-to-end scenario tests (live queries, stream TTL, etc.)
- Each test file in this folder is registered as a `[[test]]` target in
  `backend/Cargo.toml` (Cargo does **not** auto-discover nested tests).

## Writing a new HTTP test

All HTTP tests should import the test support module:
```rust
#[path = "../common/testserver/mod.rs"]
mod test_support;
```

Use the helper wrapper so every test gets a clean, isolated data directory and a
graceful shutdown:

- `test_support::http_server::with_http_test_server(...)`
- `server.execute_sql("...")`

Notes:

- The SQL API requires `Authorization`. The helper uses localhost `root:` (empty
  password) via Basic Auth.
- The helper serializes server startup within a process because some subsystems
  (notably Raft/bootstrap) are not restart/parallel-safe yet.
