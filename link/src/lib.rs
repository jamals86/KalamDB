//! # kalam-link: KalamDB Client Library
//!
//! A WebAssembly-compatible client library for connecting to KalamDB servers.
//! Provides both HTTP query execution and WebSocket subscription capabilities.
//!
//! ## Features
//!
//! - **Query Execution**: Execute SQL queries via HTTP with automatic retry
//! - **WebSocket Subscriptions**: Real-time change notifications via WebSocket streams
//! - **Authentication**: HTTP Basic Auth, JWT token, and API key support
//! - **Credential Storage**: Platform-agnostic credential management trait
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
//!     let response = client
//!         .execute_query("SELECT * FROM users LIMIT 10", None, None)
//!         .await?;
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
//! use kalam_link::{KalamLinkClient, AuthProvider};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Using HTTP Basic Auth (recommended)
//! let client = KalamLinkClient::builder()
//!     .base_url("http://localhost:3000")
//!     .auth(AuthProvider::basic_auth("username".to_string(), "password".to_string()))
//!     .build()?;
//!
//! // Using system user (default "root" user created during DB initialization)
//! let client = KalamLinkClient::builder()
//!     .base_url("http://localhost:3000")
//!     .auth(AuthProvider::system_user_auth("admin_password".to_string()))
//!     .build()?;
//!
//! // Using JWT token
//! let client = KalamLinkClient::builder()
//!     .base_url("http://localhost:3000")
//!     .jwt_token("your-jwt-token")
//!     .build()?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Credential Storage
//!
//! ```rust,no_run
//! use kalam_link::credentials::{CredentialStore, Credentials, MemoryCredentialStore};
//!
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let mut store = MemoryCredentialStore::new();
//!
//! // Store JWT token credentials (obtained from login)
//! let creds = Credentials::with_details(
//!     "production".to_string(),
//!     "eyJhbGciOiJIUzI1NiJ9.jwt_token_here".to_string(),
//!     "alice".to_string(),
//!     "2025-12-31T23:59:59Z".to_string(),
//!     Some("https://db.example.com".to_string()),
//! );
//! store.set_credentials(&creds)?;
//!
//! // Retrieve credentials
//! if let Some(stored) = store.get_credentials("production")? {
//!     if !stored.is_expired() {
//!         println!("Found valid token for user: {:?}", stored.username);
//!     }
//! }
//! # Ok(())
//! # }
//! ```

pub mod error;
pub mod live;
pub mod models;
pub mod compression;
#[cfg(feature = "tokio-runtime")]
mod normalize;
pub mod seq_id;
pub mod timeouts;
pub mod timestamp;

// Credential storage (available in both native and WASM)
pub mod credentials;

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

pub use credentials::{CredentialStore, Credentials, MemoryCredentialStore};
pub use error::{KalamLinkError, Result};
pub use live::LiveConnection;
pub use models::{
    ChangeEvent, ConnectionOptions, ErrorDetail, HealthCheckResponse, HttpVersion, KalamDataType,
    LoginRequest, LoginResponse, LoginUserInfo, QueryRequest, QueryResponse, QueryResult,
    SchemaField, SubscriptionConfig, SubscriptionOptions,
};
pub use seq_id::SeqId;
pub use timeouts::{KalamLinkTimeouts, KalamLinkTimeoutsBuilder};
pub use timestamp::{
    TimestampFormat, TimestampFormatter, TimestampFormatterConfig, now, parse_iso8601,
};

#[cfg(feature = "tokio-runtime")]
pub use query::QueryExecutor;
#[cfg(feature = "tokio-runtime")]
pub use subscription::SubscriptionManager;

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
