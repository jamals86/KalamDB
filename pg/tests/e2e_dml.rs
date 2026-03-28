// pg/tests/e2e_dml.rs
//
// End-to-end DML tests for the pg_kalam FDW extension.
//
// These tests target a local pgrx PostgreSQL instance and a locally running
// KalamDB server.
//
// Prerequisites:
//   1. KalamDB server running:          cd backend && cargo run
//   2. pgrx PG16 set up:               pg/scripts/pgrx-test-setup.sh
//
// Run:
//   cargo nextest run --features e2e -p kalam-pg-extension -E 'test(e2e_dml)'

#![cfg(feature = "e2e")]

mod e2e_common;

#[path = "e2e_dml/mod.rs"]
mod dml_cases;
