//! Cluster-related types (now mostly handled via Raft replication)
//! 
//! Live query notifications are handled through Raft-replicated data appliers
//! rather than separate HTTP broadcasts.
