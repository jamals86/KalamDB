//! # kalamdb-oidc
//!
//! Asynchronous OIDC JWT validator with JWKS caching.
//!
//! Adapted from [async-oidc-jwt-validator](https://github.com/soya-miyoshi/async-oidc-jwt-validator)
//! (MIT licensed) and extended for KalamDB's multi-issuer security model.
//!
//! ## Features
//!
//! - OIDC Discovery: auto-fetches `jwks_uri` from `{issuer}/.well-known/openid-configuration`
//! - JWKS caching: keys are cached per-issuer in an `Arc<RwLock<HashMap>>` and
//!   refreshed on cache-miss (key rotation) or explicitly via `refresh_jwks_cache`.
//! - Multi-algorithm support: RS256, RS384, RS512, PS256, PS384, PS512, ES256, ES384.
//! - Issuer + audience validation via `jsonwebtoken::Validation`.
//! - Shared JWT claims type (`JwtClaims`) and token type enum (`TokenType`) used
//!   across both internal HS256 and external OIDC token paths.
//! - Unverified header/payload utilities for algorithm-based routing (`extract_algorithm_unverified`,
//!   `extract_issuer_unverified`).
//!
//! ## Usage
//!
//! ```rust,no_run
//! use kalamdb_oidc::{OidcConfig, OidcValidator, JwtClaims};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = OidcConfig::discover(
//!         "https://keycloak.example.com/realms/myapp".into(),
//!         Some("my-client-id".into()),
//!     ).await?;
//!
//!     let validator = OidcValidator::new(config);
//!     let claims: JwtClaims = validator.validate("eyJ...").await?;
//!     Ok(())
//! }
//! ```

pub mod claims;
mod config;
mod error;
pub mod utils;
mod validator;

pub use claims::{JwtClaims, TokenType, DEFAULT_JWT_EXPIRY_HOURS};
pub use config::OidcConfig;
pub use error::OidcError;
pub use utils::{extract_algorithm_unverified, extract_issuer_unverified};
pub use validator::OidcValidator;

// Re-export types users may need when constructing custom validations
pub use jsonwebtoken::{Algorithm, Validation};
