// KalamDB Authentication Library
// Provides password hashing, JWT validation, Basic Auth, and authorization
//
// SINGLE SOURCE OF TRUTH: All authentication logic goes through the `unified` module.
// Both HTTP (/sql endpoint) and WebSocket handlers use the same authentication flow.

pub mod basic_auth;
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
pub mod unified;
pub mod user_repo;

// Re-export commonly used types
pub use context::AuthenticatedUser;
pub use error::AuthError;
pub use extractor::{AuthExtractError, AuthSession, OptionalAuth};
pub use impersonation::{ImpersonationContext, ImpersonationOrigin};
pub use ip_extractor::{extract_client_ip_secure, is_localhost_address};
pub use unified::{
    authenticate, extract_username_for_audit, AuthMethod, AuthRequest, AuthenticationResult,
};
pub use user_repo::UserRepository;
