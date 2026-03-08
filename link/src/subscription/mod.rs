//! WebSocket subscription management for real-time updates.
//!
//! `models` (subscription models) is always available — no `tokio-runtime` needed.
//! The WebSocket manager requires the `tokio-runtime` feature.

pub mod models;
mod live_rows_config;
mod live_rows_event;
mod live_rows_materializer;

pub use models::{SubscriptionConfig, SubscriptionInfo, SubscriptionOptions, SubscriptionRequest};
pub use live_rows_config::LiveRowsConfig;
pub use live_rows_event::LiveRowsEvent;
pub use live_rows_materializer::LiveRowsMaterializer;

#[cfg(feature = "tokio-runtime")]
mod live_rows_subscription;
#[cfg(feature = "tokio-runtime")]
mod manager;
#[cfg(feature = "tokio-runtime")]
mod reader;
#[cfg(feature = "tokio-runtime")]
pub use live_rows_subscription::LiveRowsSubscription;
#[cfg(feature = "tokio-runtime")]
pub use manager::SubscriptionManager;
#[cfg(feature = "tokio-runtime")]
pub(crate) use reader::ws_reader_loop;
