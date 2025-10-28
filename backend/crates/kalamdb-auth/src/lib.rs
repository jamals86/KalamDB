// KalamDB Authentication Library
// Provides password hashing, JWT validation, Basic Auth, and authorization

pub mod password;
pub mod basic_auth;
pub mod jwt_auth;
pub mod connection;
pub mod context;
pub mod error;
pub mod service;

// Re-export commonly used types
pub use context::AuthenticatedUser;
pub use error::AuthError;
pub use service::AuthService;
