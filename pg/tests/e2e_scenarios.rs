// pg/tests/e2e_scenarios.rs
//
// Real-world end-to-end scenarios for pg_kalam.
//
// These tests exercise multi-table application flows instead of isolated CRUD
// operations so the extension is validated under realistic product patterns.

#![cfg(feature = "e2e")]

mod e2e_common;

#[path = "e2e_scenarios/mod.rs"]
mod scenarios;
