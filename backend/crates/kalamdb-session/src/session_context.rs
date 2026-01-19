//! Session User Context
//!
//! This module provides `SessionUserContext` - the unified way to pass user identity
//! and role information through DataFusion's session configuration.
//!
//! ## Usage
//!
//! The context is injected into DataFusion's SessionState extensions by `ExecutionContext`
//! and read by TableProviders during `scan()` for:
//! - Per-user data filtering (USER tables)
//! - Role-based access control (SYSTEM tables)
//! - Read routing in Raft clusters
//!
//! ## Architecture
//!
//! ```text
//! HTTP Handler → ExecutionContext → SessionState.extensions → TableProvider.scan()
//! ```

use datafusion::common::config::{ConfigEntry, ConfigExtension, ExtensionOptions};
use kalamdb_commons::models::{ReadContext, Role, UserId};
use std::any::Any;

/// Session-level user context passed via DataFusion's extension system
///
/// **Purpose**: Pass (user_id, role, read_context) from HTTP handler → ExecutionContext → TableProvider.scan()
/// via SessionState.config.options.extensions (ConfigExtension trait)
///
/// **Architecture**: Stateless TableProviders read this from SessionState during scan(),
/// eliminating the need for per-request provider instances or SessionState clones.
///
/// **Performance**: Storing metadata in extensions allows zero-copy table registration
/// (tables registered once in base_session_context, no clone overhead per request).
///
/// **Read Context** (Spec 021): Determines whether reads must go to the Raft leader.
/// - `Client` (default): External SQL queries - must read from leader for consistency
/// - `Internal`: Background jobs, notifications - can read from any node
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionUserContext {
    pub user_id: UserId,
    pub role: Role,
    /// Read routing context (default: Client = requires leader)
    pub read_context: ReadContext,
}

impl Default for SessionUserContext {
    fn default() -> Self {
        SessionUserContext {
            user_id: UserId::from("anonymous"),
            role: Role::User,
            read_context: ReadContext::Client, // Default to client reads (require leader)
        }
    }
}

impl SessionUserContext {
    /// Create a new session context
    pub fn new(user_id: UserId, role: Role, read_context: ReadContext) -> Self {
        Self {
            user_id,
            role,
            read_context,
        }
    }

    /// Create a session context for a client request (requires leader)
    pub fn client(user_id: UserId, role: Role) -> Self {
        Self::new(user_id, role, ReadContext::Client)
    }

    /// Create a session context for internal operations (can read from any node)
    pub fn internal(user_id: UserId, role: Role) -> Self {
        Self::new(user_id, role, ReadContext::Internal)
    }

    /// Check if this is an admin user (System or Dba role)
    #[inline]
    pub fn is_admin(&self) -> bool {
        matches!(self.role, Role::System | Role::Dba)
    }

    /// Check if this is the system role
    #[inline]
    pub fn is_system(&self) -> bool {
        matches!(self.role, Role::System)
    }
}

impl ExtensionOptions for SessionUserContext {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn cloned(&self) -> Box<dyn ExtensionOptions> {
        Box::new(self.clone())
    }

    fn set(&mut self, _key: &str, _value: &str) -> datafusion::common::Result<()> {
        // SessionUserContext is immutable - ignore set operations
        Ok(())
    }

    fn entries(&self) -> Vec<ConfigEntry> {
        // No configuration entries
        vec![]
    }
}

impl ConfigExtension for SessionUserContext {
    const PREFIX: &'static str = "kalamdb";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_session_user_context() {
        let ctx = SessionUserContext::default();
        assert_eq!(ctx.user_id.as_str(), "anonymous");
        assert_eq!(ctx.role, Role::User);
        assert_eq!(ctx.read_context, ReadContext::Client);
        assert!(!ctx.is_admin());
    }

    #[test]
    fn test_session_user_context_admin() {
        let ctx = SessionUserContext::client(UserId::new("admin"), Role::Dba);
        assert!(ctx.is_admin());
        assert!(!ctx.is_system());

        let ctx = SessionUserContext::client(UserId::new("system"), Role::System);
        assert!(ctx.is_admin());
        assert!(ctx.is_system());
    }

    #[test]
    fn test_session_user_context_internal() {
        let ctx = SessionUserContext::internal(UserId::new("job_worker"), Role::Service);
        assert_eq!(ctx.read_context, ReadContext::Internal);
        assert!(!ctx.is_admin());
    }
}
