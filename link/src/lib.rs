//! # kalam-link: KalamDB Client Library
//!
//! A WebAssembly-compatible client library for connecting to KalamDB servers.
//! Provides both HTTP query execution and WebSocket subscription capabilities.
//!
//! ## Features
//!
//! - **Query Execution**: Execute SQL queries via HTTP with automatic retry
//! - **WebSocket Subscriptions**: Real-time change notifications via WebSocket streams
//! - **Authentication**: JWT token and API key support
//! - **WASM Compatible**: Can be compiled to WebAssembly for browser usage
//! - **Connection Pooling**: Automatic HTTP connection reuse
//! - **Configurable Timeouts**: Per-request timeout and retry configuration
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use kalam_link::{KalamLinkClient, QueryRequest};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Build a client with custom configuration
//!     let client = KalamLinkClient::builder()
//!         .base_url("http://localhost:3000")
//!         .timeout(std::time::Duration::from_secs(30))
//!         .build()?;
//!
//!     // Execute a query
//!     let response = client.execute_query("SELECT * FROM users LIMIT 10").await?;
//!     println!("Results: {:?}", response.results);
//!
//!     // Subscribe to real-time changes
//!     let mut subscription = client.subscribe("SELECT * FROM messages").await?;
//!     while let Some(event) = subscription.next().await {
//!         println!("Change detected: {:?}", event);
//!     }
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Authentication
//!
//! ```rust,no_run
//! use kalam_link::KalamLinkClient;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Using JWT token
//! let client = KalamLinkClient::builder()
//!     .base_url("http://localhost:3000")
//!     .jwt_token("your-jwt-token")
//!     .build()?;
//!
//! // Using API key
//! let client = KalamLinkClient::builder()
//!     .base_url("http://localhost:3000")
//!     .api_key("your-api-key")
//!     .build()?;
//! # Ok(())
//! # }
//! ```

pub mod error;
pub mod models;

// Native-only modules (use tokio, reqwest, tokio-tungstenite)
#[cfg(feature = "tokio-runtime")]
pub mod auth;
#[cfg(feature = "tokio-runtime")]
pub mod client;
#[cfg(feature = "tokio-runtime")]
pub mod query;
#[cfg(feature = "tokio-runtime")]
pub mod subscription;

// WASM bindings module (T041)
#[cfg(feature = "wasm")]
pub mod wasm;

// Re-export main types for convenience
#[cfg(feature = "tokio-runtime")]
pub use auth::AuthProvider;
#[cfg(feature = "tokio-runtime")]
pub use client::KalamLinkClient;

pub use error::{KalamLinkError, Result};
pub use models::{
    ChangeEvent, ErrorDetail, HealthCheckResponse, QueryRequest, QueryResponse, SubscriptionOptions,
};

#[cfg(feature = "tokio-runtime")]
pub use query::QueryExecutor;
#[cfg(feature = "tokio-runtime")]
pub use subscription::{SubscriptionConfig, SubscriptionManager};

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
