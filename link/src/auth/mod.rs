//! Authentication provider for KalamDB client.
//!
//! Handles JWT tokens, HTTP Basic Auth, and async dynamic auth providers.

mod provider;

pub use provider::{ArcDynAuthProvider, AuthProvider, DynamicAuthProvider, ResolvedAuth};
