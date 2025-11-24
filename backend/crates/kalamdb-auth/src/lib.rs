// KalamDB Authentication Library
// Provides password hashing, JWT validation, Basic Auth, and authorization

pub mod basic_auth;
pub mod connection;
pub mod context;
pub mod error;
pub mod extractor;
pub mod impersonation;
pub mod ip_extractor;
pub mod jwt_auth;
pub mod oauth;
pub mod password;
pub mod rbac;
pub mod roles;
// TODO: service.rs needs refactoring to remove RocksDbAdapter dependencies
pub mod service;
pub mod user_repo;

// Re-export commonly used types
pub use context::AuthenticatedUser;
pub use error::AuthError;
pub use extractor::{extract_auth_with_repo, AuthenticatedRequest};
pub use impersonation::{ImpersonationContext, ImpersonationOrigin};
pub use ip_extractor::{extract_client_ip_secure, is_localhost_address};
pub use service::AuthService; // Temporarily disabled - needs refactoring
pub use user_repo::UserRepository;
