//! Read routing context for Raft cluster mode.
//!
//! In a Raft cluster, client queries should only read from the leader node
//! to guarantee linearizable consistency. Internal operations (like background
//! jobs or live query notifications) may read from any node (including followers).
//!
//! # Usage
//!
//! ```rust
//! use kalamdb_commons::models::ReadContext;
//!
//! // Client SQL queries require leader
//! let client_ctx = ReadContext::Client;
//! assert!(client_ctx.requires_leader());
//!
//! // Internal operations can read locally
//! let internal_ctx = ReadContext::Internal;
//! assert!(!internal_ctx.requires_leader());
//! ```

use serde::{Deserialize, Serialize};

/// Read routing context that determines whether a read must go to the leader.
///
/// In Raft cluster mode:
/// - `Client` reads MUST go to the leader for linearizable consistency
/// - `Internal` reads can proceed on any node (used by jobs, notifications, etc.)
///
/// In standalone mode, this has no effect - all reads are local.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ReadContext {
    /// Client SQL query - must go to leader for consistency.
    ///
    /// This is the default for all external SQL queries. If the current node
    /// is not the leader for the target shard, a `NotLeader` error is returned
    /// with a hint to redirect to the leader node.
    #[default]
    Client,

    /// Internal operation - always reads local data.
    ///
    /// Used by:
    /// - Background jobs (FlushExecutor, CleanupExecutor, etc.)
    /// - Live query notifications (post-apply on each node)
    /// - Metrics and monitoring
    /// - Internal health checks
    ///
    /// Internal operations read local state which may be slightly behind the
    /// leader (due to Raft log replication latency), but this is acceptable
    /// for their use cases.
    Internal,
}

impl ReadContext {
    /// Returns `true` if this context requires reading from the leader node.
    ///
    /// # Returns
    /// - `true` for `Client` context
    /// - `false` for `Internal` context
    #[inline]
    pub fn requires_leader(&self) -> bool {
        matches!(self, ReadContext::Client)
    }

    /// Returns `true` if this context allows reading from any node.
    ///
    /// # Returns
    /// - `true` for `Internal` context
    /// - `false` for `Client` context
    #[inline]
    pub fn allows_follower_read(&self) -> bool {
        matches!(self, ReadContext::Internal)
    }
}

impl std::fmt::Display for ReadContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadContext::Client => write!(f, "client"),
            ReadContext::Internal => write!(f, "internal"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_requires_leader() {
        let ctx = ReadContext::Client;
        assert!(ctx.requires_leader());
        assert!(!ctx.allows_follower_read());
    }

    #[test]
    fn test_internal_allows_follower() {
        let ctx = ReadContext::Internal;
        assert!(!ctx.requires_leader());
        assert!(ctx.allows_follower_read());
    }

    #[test]
    fn test_default_is_client() {
        assert_eq!(ReadContext::default(), ReadContext::Client);
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", ReadContext::Client), "client");
        assert_eq!(format!("{}", ReadContext::Internal), "internal");
    }
}
