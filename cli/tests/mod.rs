//! CLI integration test modules
//!
//! This module organizes CLI tests into logical groups for better maintainability
//! and parallel test execution.

pub mod common;
pub mod user_tables;
pub mod shared_tables;
pub mod cli;
pub mod flush;
pub mod subscribe;
pub mod auth;
pub mod admin;