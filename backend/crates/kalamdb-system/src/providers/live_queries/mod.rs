//! System.live_queries table (IndexedEntityStore-based)
//!
//! This module implements the system.live_queries table using IndexedEntityStore
//! with a secondary index for efficient table_id lookups (for change broadcasts).
//!
//! ## Indexes
//!
//! 1. **TableIdIndex** - Queries by namespace_id + table_name (for change broadcasts)
//!
//! ## Note on ConnectionId lookups
//!
//! ConnectionId lookups use primary key prefix scan (`{user_id}-{connection_id}-`)
//! which is O(1) since max ~10 live queries per user. No secondary index needed.

pub mod live_queries_indexes;
pub mod live_queries_provider;
pub mod models;

pub use live_queries_indexes::{create_live_queries_indexes, table_id_index_prefix, TABLE_ID_INDEX};
pub use live_queries_provider::LiveQueriesTableProvider;
pub use models::{LiveQuery, LiveQueryStatus};
