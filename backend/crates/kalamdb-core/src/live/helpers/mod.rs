//! Helper utilities and functions for live queries
//!
//! This module contains utility functions for:
//! - Filter expression evaluation
//! - SQL query parsing
//! - Error handling
//! - Initial data fetching
//! - Failover handling

pub mod error;
pub mod failover;
pub mod filter_eval;
pub mod initial_data;
pub mod query_parser;

// Re-export commonly used items
pub use error::{LiveError, Result as LiveResult};
pub use failover::{CleanupReport as LiveQueryCleanupReport, LiveQueryFailoverHandler};
pub use filter_eval::{matches as filter_matches, parse_where_clause};
pub use initial_data::{InitialDataFetcher, InitialDataOptions, InitialDataResult};
pub use query_parser::QueryParser;
