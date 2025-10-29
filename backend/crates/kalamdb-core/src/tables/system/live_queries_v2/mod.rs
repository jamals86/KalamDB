//! System.live_queries table v2 (EntityStore-based)
//!
//! This module implements the system.live_queries table using the new EntityStore architecture.
//! It replaces the old live_queries_provider with a type-safe SystemTableStore<LiveQueryId, LiveQuery>.

pub mod live_queries_table;
pub mod live_queries_store;
pub mod live_queries_provider;

pub use live_queries_table::LiveQueriesTableSchema;
pub use live_queries_store::{LiveQueriesStore, new_live_queries_store};
pub use live_queries_provider::LiveQueriesTableProvider;
