// KalamDB Authentication Library
// Provides password hashing, JWT validation, Basic Auth, and authorization

pub mod basic_auth;
pub mod connection;
pub mod context;
pub mod error;
pub mod extractor;
pub mod jwt_auth;
pub mod oauth;
pub mod password;
// TODO: service.rs needs refactoring to remove RocksDbAdapter dependencies
// pub mod service;
pub mod user_repo;

// Re-export commonly used types
pub use context::AuthenticatedUser;
pub use error::AuthError;
#[allow(deprecated)]
pub use extractor::{extract_auth, extract_auth_with_repo, AuthenticatedRequest};
// pub use service::AuthService; // Temporarily disabled - needs refactoring
pub use user_repo::UserRepository;
