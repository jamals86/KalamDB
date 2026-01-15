//! System Tables and Configuration Tests
//!
//! Tests covering:
//! - Audit logging
//! - Configuration access
//! - Live queries metadata
//! - System users
//! - System user initialization

#[path = "../../common/testserver/mod.rs"]
#[allow(dead_code)]
pub(super) mod test_support;

// System Tests
mod test_audit_logging;
mod test_config_access;
mod test_live_queries_metadata;
mod test_system_users;
mod test_system_user_init;
