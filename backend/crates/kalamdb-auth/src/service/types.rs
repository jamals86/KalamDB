use moka::future::Cache;
use kalamdb_commons::system::User;
use crate::jwt_auth::JwtClaims;
use kalamdb_commons::Role;

/// User cache statistics for monitoring and performance tracking
#[derive(Debug, Clone)]
pub struct UserCacheStats {
    /// Number of cache hits
    pub hits: u64,
    /// Number of cache misses
    pub misses: u64,
    /// Cache hit rate (0.0 to 1.0)
    pub hit_rate: f64,
    /// Current number of entries in cache
    pub entry_count: u64,
}

/// JWT cache statistics for monitoring and performance tracking
#[derive(Debug, Clone)]
pub struct JwtCacheStats {
    /// Number of cache hits
    pub hits: u64,
    /// Number of cache misses
    pub misses: u64,
    /// Cache hit rate (0.0 to 1.0)
    pub hit_rate: f64,
    /// Current number of entries in cache
    pub entry_count: u64,
}

/// Authentication service that orchestrates all auth methods.
///
/// Supports:
/// - HTTP Basic Authentication (username/password)
/// - JWT Bearer Token Authentication
/// - OAuth Token Authentication (Phase 10, User Story 8)
pub struct AuthService {
    /// JWT secret for token validation
    pub(crate) jwt_secret: String,
    /// List of trusted JWT issuers
    pub(crate) trusted_jwt_issuers: Vec<String>,
    /// Whether to allow remote (non-localhost) access
    pub(crate) allow_remote_access: bool,
    /// OAuth auto-provisioning enabled (Phase 10, User Story 8)
    #[allow(dead_code)]
    pub(crate) oauth_auto_provision: bool,
    /// Default role for auto-provisioned OAuth users
    #[allow(dead_code)]
    pub(crate) oauth_default_role: Role,
    /// User record cache for performance optimization
    /// Key: username, Value: User record
    /// TTL: 5 minutes, Max capacity: 1000 users
    pub(crate) user_cache: Cache<String, User>,
    /// JWT claims cache for performance optimization
    /// Key: JWT token string, Value: Validated JWT claims
    /// TTL: 10 minutes, Max capacity: 500 tokens
    pub(crate) jwt_cache: Cache<String, JwtClaims>,
}

impl AuthService {
    /// Create a new authentication service.
    ///
    /// # Arguments
    /// * `jwt_secret` - Secret key for JWT validation
    /// * `trusted_jwt_issuers` - List of trusted JWT issuer domains
    /// * `allow_remote_access` - Whether to allow non-localhost connections
    /// * `oauth_auto_provision` - Enable OAuth auto-provisioning (default: false)
    /// * `oauth_default_role` - Default role for auto-provisioned OAuth users
    pub fn new(
        jwt_secret: String,
        trusted_jwt_issuers: Vec<String>,
        allow_remote_access: bool,
        oauth_auto_provision: bool,
        oauth_default_role: Role,
    ) -> Self {
        // Initialize user cache with 5-minute TTL and 1000 max entries
        let user_cache = Cache::builder()
            .max_capacity(1000)
            .time_to_live(std::time::Duration::from_secs(300)) // 5 minutes
            .build();

        // Initialize JWT cache with 10-minute TTL and 500 max entries
        let jwt_cache = Cache::builder()
            .max_capacity(500)
            .time_to_live(std::time::Duration::from_secs(600)) // 10 minutes
            .build();

        Self {
            jwt_secret,
            trusted_jwt_issuers,
            allow_remote_access,
            oauth_auto_provision,
            oauth_default_role,
            user_cache,
            jwt_cache,
        }
    }
}
