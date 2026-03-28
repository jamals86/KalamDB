// pg/tests/e2e_perf.rs
//
// Performance benchmarks for the pg_kalam FDW extension.
//
// These tests run against the same Docker Compose stack as e2e_dml tests.
// They measure throughput and latency for various operations and assert
// production-grade performance thresholds.
//
// Run with:
//   cargo nextest run --features e2e -p kalam-pg-extension -E 'test(e2e_perf)'

#![cfg(feature = "e2e")]

mod e2e_common;

#[path = "e2e_perf/mod.rs"]
mod perf_cases;
