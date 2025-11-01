//! CLI integration test modules
//!
//! This module organizes CLI tests into logical groups for better maintainability
//! and parallel test execution.

pub mod helpers {
    pub mod common;
}

pub mod test_user_tables;
pub mod test_shared_tables;
pub mod test_cli;
pub mod test_flush;
pub mod test_subscribe;
pub mod test_auth;
pub mod test_admin;