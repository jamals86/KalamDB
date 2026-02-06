# Research & Technical Decisions: User Authentication

**Feature**: User Authentication (007-user-auth)  
**Date**: October 27, 2025  
**Status**: Phase 0 - Research Complete

---

## Overview

This document captures research findings and technical decisions for implementing authentication and authorization in KalamDB. All NEEDS CLARIFICATION items from the plan have been resolved with specific decisions, rationale, and alternatives considered.

---

## 1. Bcrypt Configuration

### Decision

**Use bcrypt with cost factor 12, async execution via `tokio::task::spawn_blocking`**

### Rationale

1. **Cost Factor 12 Analysis**:
   - Industry standard (used by Django, Rails, Laravel)
   - Approximately 250-300ms per hash/verify operation
   - Resistant to brute force: 2^12 = 4096 iterations
   - Balance between security and UX (sub-second login)
   - Can be increased to 13 or 14 in future without breaking existing hashes

2. **Benchmark Results** (estimated on modern server CPU):
   ```
   Cost 10: ~60ms  (too fast, vulnerable to GPU attacks)
   Cost 11: ~120ms (acceptable but suboptimal)
   Cost 12: ~250ms (recommended)
   Cost 13: ~500ms (too slow for typical UX)
   Cost 14: ~1000ms (only for highly sensitive systems)
   ```

3. **Async Execution**:
   - Bcrypt is CPU-intensive and blocks the thread
   - Using `tokio::task::spawn_blocking` prevents blocking async runtime
   - Tokio's blocking thread pool handles concurrent bcrypt operations
   - Maintains responsive server under high auth load

### Implementation Details

```rust
// In kalamdb-auth/src/password.rs

use bcrypt::{hash, verify, DEFAULT_COST};

const BCRYPT_COST: u32 = 12;

pub async fn hash_password(password: &str) -> Result<String, PasswordError> {
    let password = password.to_string();
    tokio::task::spawn_blocking(move || {
        hash(password, BCRYPT_COST)
            .map_err(|e| PasswordError::HashingFailed(e.to_string()))
    })
    .await
    .map_err(|e| PasswordError::TaskJoinFailed(e.to_string()))?
}

pub async fn verify_password(password: &str, hash: &str) -> Result<bool, PasswordError> {
    let password = password.to_string();
    let hash = hash.to_string();
    tokio::task::spawn_blocking(move || {
        verify(password, &hash)
            .map_err(|e| PasswordError::VerificationFailed(e.to_string()))
    })
    .await
    .map_err(|e| PasswordError::TaskJoinFailed(e.to_string()))?
}
```

### Alternatives Considered

| Alternative | Pros | Cons | Why Rejected |
|-------------|------|------|--------------|
| **Argon2** | More modern, memory-hard | Less mature Rust ecosystem, higher memory usage | Bcrypt sufficient for our needs |
| **PBKDF2** | Simple, well-understood | Vulnerable to GPU attacks | Inferior to bcrypt for passwords |
| **Scrypt** | Memory-hard, GPU-resistant | Memory requirements problematic at scale | Bcrypt balances security & scalability |
| **Cost Factor 13** | Stronger security | 500ms too slow for typical login | UX impact outweighs marginal security gain |
| **Sync bcrypt** | Simpler code | Blocks async runtime | Unacceptable performance impact |

### Configuration

**In config.toml**:
```toml
[authentication]
# Bcrypt cost factor (default: 12, range: 10-14)
# Higher values = more secure but slower
# Changing this only affects NEW passwords
bcrypt_cost = 12

# Minimum password length (default: 8)
min_password_length = 8

# Maximum password length (default: 1024, max: 72 for bcrypt)
max_password_length = 72
```

**Note**: Bcrypt has a 72-character limit. Passwords longer than 72 chars should be rejected before hashing.

---

## 2. JWT Validation Strategy

### Decision

**Use `jsonwebtoken` crate with JWKS endpoint caching, RS256/ES256 algorithms, issuer allowlist**

**IMPORTANT**: See `auth-performance.md` for comprehensive caching strategy including:
- JWT token claim caching (5-10x speedup)
- User record caching (saves 1-5ms per request)
- Background JWKS refresh
- Cache invalidation strategies

### Rationale

1. **Library Choice**:
   - `jsonwebtoken` crate is battle-tested (1000+ dependent crates)
   - Active maintenance and security updates
   - Supports all major JWT algorithms (RS256, ES256, HS256)
   - Built-in validation for exp, iss, nbf, aud claims
   - No external service dependencies

2. **Algorithm Support**:
   - **RS256** (RSA + SHA256): Industry standard for external OAuth providers
   - **ES256** (ECDSA + SHA256): Modern alternative, smaller signatures
   - **HS256** (HMAC + SHA256): For KalamDB-issued tokens (shared secret)
   - Reject insecure algorithms (none, HS384 with weak secrets)

3. **JWKS Caching Strategy**:
   - Cache public keys from `/.well-known/jwks.json` endpoints
   - 1-hour TTL (refresh automatically)
   - Invalidate on signature verification failure
   - Reduces external HTTP requests by 99%+
   - **Performance Enhancement**: Background refresh before expiry (see auth-performance.md)

4. **Issuer Validation**:
   - Maintain allowlist in config.toml
   - Reject tokens from unknown issuers
   - Prevents token substitution attacks

### Implementation Details

```rust
// In kalamdb-auth/src/jwt_auth.rs

use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,        // user_id (REQUIRED)
    pub iss: String,        // issuer (REQUIRED)
    pub exp: i64,           // expiration timestamp (REQUIRED)
    pub iat: Option<i64>,   // issued at
    pub email: Option<String>,
    pub role: Option<String>,
}

pub struct JwtValidator {
    allowed_issuers: Vec<String>,
    jwks_cache: Arc<RwLock<HashMap<String, DecodingKey>>>,
    hs256_secret: Option<String>,
}

impl JwtValidator {
    pub async fn validate(&self, token: &str) -> Result<Claims, JwtError> {
        // 1. Decode header to get algorithm and key ID
        let header = decode_header(token)?;
        
        // 2. Get decoding key (from cache or fetch)
        let decoding_key = self.get_decoding_key(&header).await?;
        
        // 3. Validate token (exp, iss, signature)
        let mut validation = Validation::new(header.alg);
        validation.set_issuer(&self.allowed_issuers);
        validation.leeway = 60; // 60s clock skew tolerance
        
        let token_data = decode::<Claims>(token, &decoding_key, &validation)?;
        
        // 4. Additional validation
        if token_data.claims.sub.is_empty() {
            return Err(JwtError::MissingSubClaim);
        }
        
        Ok(token_data.claims)
    }
    
    async fn get_decoding_key(&self, header: &Header) -> Result<DecodingKey, JwtError> {
        match header.alg {
            Algorithm::RS256 | Algorithm::ES256 => {
                // Fetch from JWKS endpoint (with caching)
                self.fetch_jwks_key(header.kid.as_ref()).await
            }
            Algorithm::HS256 => {
                // Use configured shared secret
                self.hs256_secret
                    .as_ref()
                    .map(|s| DecodingKey::from_secret(s.as_bytes()))
                    .ok_or(JwtError::MissingHs256Secret)
            }
            _ => Err(JwtError::UnsupportedAlgorithm(header.alg)),
        }
    }
}
```

### Alternatives Considered

| Alternative | Pros | Cons | Why Rejected |
|-------------|------|------|--------------|
| **DIY JWT parsing** | No dependencies | Security risk, hard to maintain | Never roll your own crypto |
| **OAuth2-specific library** | Higher-level abstractions | Over-engineered, complex | Just need JWT validation |
| **External validation service** | Centralized, could add features | Network dependency, latency | Adds failure point |
| **No issuer validation** | Simpler | Security risk | Allows token substitution |
| **No JWKS caching** | Simpler | HTTP request per validation | 100x slower |

### Configuration

**In config.toml**:
```toml
[authentication.jwt]
# Allowed token issuers (iss claim validation)
allowed_issuers = [
    "https://kalamdb.io",
    "https://accounts.google.com",
    "https://github.com",
]

# JWKS endpoint cache TTL (seconds)
jwks_cache_ttl = 3600  # 1 hour

# Clock skew tolerance (seconds)
clock_skew_leeway = 60

# HS256 shared secret for KalamDB-issued tokens (optional)
# If not set, only RS256/ES256 tokens accepted
# hs256_secret = "your-secret-key-at-least-32-chars"

# Optional: Verify user existence in system.users table
# If true, reject JWTs for users not in database
verify_user_exists = false
```

---

## 3. Authentication Middleware Pattern

### Decision

**Two-layer architecture: AuthService (library) → AuthMiddleware (Actix-Web)**

### Rationale

1. **Separation of Concerns**:
   - **AuthService** (in kalamdb-auth): Pure authentication logic, no HTTP dependencies
   - **AuthMiddleware** (in kalamdb-api): HTTP-specific adapter for Actix-Web
   - Clear boundaries enable reusability (CLI, future gRPC API, etc.)

2. **Actix-Web Integration**:
   - Use `actix-web::middleware::Middleware` trait
   - Extract Authorization header → pass to AuthService
   - Set AuthenticatedUser in request extensions
   - Handle 401/403 errors with proper HTTP responses

3. **Request Flow**:
   ```
   HTTP Request
     → AuthMiddleware::call()
       → Extract Authorization header
       → AuthService::authenticate()
         → Parse (Basic Auth or JWT)
         → Validate credentials
         → Load user from system.users
         → Check role & connection source
         → Return AuthenticatedUser
       → Set in request.extensions()
     → Route Handler
       → Extract AuthenticatedUser from extensions
       → Execute with authenticated context
   ```

### Implementation Details

**AuthService (kalamdb-auth/src/service.rs)**:
```rust
pub struct AuthService {
    db: Arc<DB>,  // RocksDB for user lookup
    jwt_validator: JwtValidator,
    config: AuthConfig,
}

impl AuthService {
    /// Authenticate request and return user context
    pub async fn authenticate(
        &self,
        auth_header: Option<&str>,
        connection_info: ConnectionInfo,
    ) -> Result<AuthenticatedUser, AuthError> {
        match auth_header {
            Some(header) if header.starts_with("Basic ") => {
                self.authenticate_basic(header, connection_info).await
            }
            Some(header) if header.starts_with("Bearer ") => {
                self.authenticate_jwt(header, connection_info).await
            }
            Some(_) => Err(AuthError::MalformedAuthorizationHeader),
            None => Err(AuthError::MissingAuthorizationHeader),
        }
    }
}
```

**AuthMiddleware (kalamdb-api/src/middleware/auth.rs)**:
```rust
use actix_web::{
    dev::{Service, ServiceRequest, ServiceResponse, Transform},
    Error, HttpResponse,
};
use kalamdb_auth::AuthService;

pub struct AuthMiddleware {
    auth_service: Arc<AuthService>,
}

impl<S, B> Transform<S, ServiceRequest> for AuthMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
{
    // Standard Actix middleware implementation
    fn call(&self, req: ServiceRequest) -> Self::Future {
        let auth_header = req.headers().get("Authorization")
            .and_then(|h| h.to_str().ok());
        
        let connection_info = extract_connection_info(&req);
        
        match self.auth_service.authenticate(auth_header, connection_info).await {
            Ok(user) => {
                req.extensions_mut().insert(user);
                // Continue to handler
            }
            Err(AuthError::MissingAuthorizationHeader) => {
                HttpResponse::Unauthorized()
                    .json(json!({
                        "error": "MISSING_AUTHORIZATION",
                        "message": "Authorization header required"
                    }))
            }
            Err(e) => {
                HttpResponse::Unauthorized()
                    .json(json!({
                        "error": "AUTHENTICATION_FAILED",
                        "message": e.to_string()
                    }))
            }
        }
    }
}
```

**Route Handler Usage**:
```rust
use actix_web::{web, HttpResponse};
use kalamdb_auth::AuthenticatedUser;

async fn create_table(
    user: web::ReqData<AuthenticatedUser>,
    body: web::Json<CreateTableRequest>,
) -> Result<HttpResponse, Error> {
    // User is automatically authenticated by middleware
    // Check authorization
    if !user.role.can_create_tables() {
        return Err(AuthorizationError::Forbidden {
            required_role: UserRole::Dba,
            user_role: user.role,
        });
    }
    
    // Proceed with table creation
    Ok(HttpResponse::Ok().json(response))
}
```

### Alternatives Considered

| Alternative | Pros | Cons | Why Rejected |
|-------------|------|------|--------------|
| **All-in-middleware** | Simpler, fewer layers | Not reusable outside HTTP | Need auth for CLI too |
| **Per-route auth** | Fine-grained control | Boilerplate, easy to forget | Middleware enforces everywhere |
| **Extractors only** | Actix-idiomatic | Still need logic layer | AuthService still needed |
| **Global state** | Easy access | Thread-safety complexity | Request extensions cleaner |

---

## 4. CLI Credential Storage

### Decision

**Store credentials in `~/.kalam/credentials.toml` with file permissions 0600 (owner read/write only)**

### Rationale

1. **XDG Base Directory Specification**:
   - Standard for Linux/macOS config files
   - `~/.config/kalamdb/` on Linux/macOS
   - `%APPDATA%\kalamdb\` on Windows
   - Well-understood by users and tools

2. **File Structure**:
   ```toml
   # ~/.kalam/credentials.toml
   
   [default]
   instance_name = "local"
   host = "localhost"
   port = 9876
   user_id = "cli_system_user_abc123"
   username = "cli_system"
   # For system users, credentials field may be omitted (localhost auth)
   
   [instances.production]
   host = "prod.kalamdb.io"
   port = 9876
   user_id = "prod_user_xyz789"
   username = "admin"
   credentials = "encrypted_token_or_password_hash"
   
   [instances.staging]
   host = "staging.kalamdb.io"
   port = 9876
   user_id = "staging_user_def456"
   username = "admin"
   credentials = "encrypted_token_or_password_hash"
   ```

3. **Security**:
   - File permissions set to 0600 on creation (chmod on Unix, ACLs on Windows)
   - Credentials field encrypted with system keyring (if available)
   - Fallback to file permissions only if keyring unavailable
   - Warn user if permissions are too open (> 0600)

4. **Multi-Instance Support**:
   - `[default]` section for main instance
   - `[instances.name]` for additional instances
   - CLI flag: `kalam --instance production` to select instance

### Implementation Details

```rust
// In cli/src/config.rs

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct CliConfig {
    pub default: InstanceConfig,
    pub instances: HashMap<String, InstanceConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InstanceConfig {
    pub instance_name: String,
    pub host: String,
    pub port: u16,
    pub user_id: String,
    pub username: String,
    pub credentials: Option<String>, // Encrypted or omitted for localhost
}

impl CliConfig {
    pub fn load() -> Result<Self, ConfigError> {
        let config_path = Self::config_path()?;
        
        if !config_path.exists() {
            return Self::create_default();
        }
        
        // Check file permissions (Unix only)
        #[cfg(unix)]
        Self::check_permissions(&config_path)?;
        
        let contents = std::fs::read_to_string(&config_path)?;
        toml::from_str(&contents).map_err(ConfigError::ParseError)
    }
    
    pub fn config_path() -> Result<PathBuf, ConfigError> {
        #[cfg(unix)]
        let config_dir = dirs::config_dir()
            .ok_or(ConfigError::NoConfigDir)?
            .join("kalamdb");
        
        #[cfg(windows)]
        let config_dir = dirs::config_dir()
            .ok_or(ConfigError::NoConfigDir)?
            .join("kalamdb");
        
        Ok(config_dir.join("credentials.toml"))
    }
    
    #[cfg(unix)]
    fn check_permissions(path: &Path) -> Result<(), ConfigError> {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(path)?;
        let mode = metadata.permissions().mode();
        
        if mode & 0o077 != 0 {
            return Err(ConfigError::InsecurePermissions {
                path: path.to_path_buf(),
                mode,
                expected: 0o600,
            });
        }
        Ok(())
    }
}
```

**Database Initialization**:
```rust
// In backend/src/lifecycle.rs or similar

pub fn initialize_database(db_path: &Path) -> Result<(), Error> {
    // ... existing initialization ...
    
    // Create default system user for CLI
    let cli_user = User {
        user_id: format!("cli_system_{}", Uuid::new_v4().simple()),
        username: "cli_system".to_string(),
        auth_type: AuthType::Internal,
        auth_data: None, // No password for localhost-only
        role: UserRole::System,
        email: None,
        metadata: serde_json::json!({
            "created_by": "database_initialization",
            "purpose": "CLI access"
        }),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        deleted_at: None,
    };
    
    store::users::create_user(&db, cli_user)?;
    
    // Write CLI config
    let cli_config = CliConfig {
        default: InstanceConfig {
            instance_name: "local".to_string(),
            host: "localhost".to_string(),
            port: 9876,
            user_id: cli_user.user_id.clone(),
            username: cli_user.username.clone(),
            credentials: None, // Localhost auth
        },
        instances: HashMap::new(),
    };
    
    cli_config.save()?;
    
    println!("✓ Created CLI system user: {}", cli_user.user_id);
    println!("✓ CLI credentials stored in: {}", CliConfig::config_path()?.display());
    
    Ok(())
}
```

### Alternatives Considered

| Alternative | Pros | Cons | Why Rejected |
|-------------|------|------|--------------|
| **OS Keychain** | Most secure | Complex cross-platform (macOS Keychain, Windows Credential Manager, Linux Secret Service) | Over-engineered for v1 |
| **Environment variables** | Simple | Visible in process list, shell history | Security risk |
| **Plaintext in config** | Simplest | Insecure | Rejected for security |
| **Home directory ~/.kalamdb** | Simple path | Non-standard, clutters home | XDG is standard |
| **Per-project config** | Fine-grained | User must configure each project | Too much friction |

---

## 5. Migration Strategy

### Decision

**Three-phase gradual migration with 6-month deprecation period**

### Rationale

1. **Zero Downtime**: Existing deployments continue working during migration
2. **User Choice**: Teams migrate on their timeline
3. **Clear Timeline**: 6-month deprecation gives ample notice
4. **Backward Compatibility**: X-API-KEY and X-USER-ID honored during transition

### Migration Phases

#### Phase 1: Dual Authentication (This Feature)

**Timeline**: Immediate (007-user-auth feature)

**Implementation**:
- Both old (X-API-KEY, X-USER-ID) and new (Authorization header) work
- Old headers take precedence if present (backward compat)
- Deprecation warnings logged to application logs
- Documentation updated with migration guide

**Code Example**:
```rust
// In AuthMiddleware

pub async fn authenticate_request(req: &ServiceRequest) -> Result<AuthenticatedUser, AuthError> {
    // Check for new auth first
    if let Some(auth_header) = req.headers().get("Authorization") {
        return self.auth_service.authenticate(auth_header, connection_info).await;
    }
    
    // Fallback to old auth (with deprecation warning)
    if let Some(api_key) = req.headers().get("X-API-KEY") {
        log::warn!(
            "DEPRECATED: X-API-KEY header is deprecated. \
             Please migrate to Authorization header. \
             Support will be removed in version 0.3.0 (estimated: June 2026)"
        );
        return self.auth_service.authenticate_api_key(api_key).await;
    }
    
    if let Some(user_id) = req.headers().get("X-USER-ID") {
        log::warn!(
            "DEPRECATED: X-USER-ID header is deprecated. \
             Please migrate to Authorization header. \
             Support will be removed in version 0.3.0 (estimated: June 2026)"
        );
        return self.auth_service.authenticate_user_id(user_id).await;
    }
    
    Err(AuthError::MissingAuthorizationHeader)
}
```

#### Phase 2: Deprecation Warnings (Future - 0.2.0 release)

**Timeline**: 3 months after Phase 1 (estimated: February 2026)

**Implementation**:
- Deprecation warnings promoted to errors in docs
- Metrics tracking old auth usage
- Email notifications to users still using old auth (if contact info available)
- Migration guide published with code examples

**Deprecation Notice Example**:
```
╔════════════════════════════════════════════════════════════════╗
║ WARNING: X-API-KEY authentication is deprecated                ║
║                                                                 ║
║ You are using the legacy X-API-KEY header which will be        ║
║ removed in version 0.3.0 (estimated release: June 2026).       ║
║                                                                 ║
║ Please migrate to the Authorization header:                    ║
║   Authorization: Basic <base64(username:password)>             ║
║   or                                                            ║
║   Authorization: Bearer <jwt_token>                            ║
║                                                                 ║
║ Migration guide: https://docs.kalamdb.io/auth-migration        ║
╚════════════════════════════════════════════════════════════════╝
```

#### Phase 3: Removal (Future - 0.3.0 release)

**Timeline**: 6 months after Phase 1 (estimated: June 2026)

**Implementation**:
- X-API-KEY and X-USER-ID headers completely removed
- Code cleanup (remove old auth paths)
- All tests updated
- Release notes clearly document breaking change

**Migration Metrics to Track**:
- % of requests using old auth vs new auth
- Top 10 API keys still in use (anonymized)
- Geographic distribution of old auth usage
- Deprecation warning click-through rate

---

## Summary of Decisions

| Topic | Decision | Key Rationale |
|-------|----------|---------------|
| **Password Hashing** | Bcrypt cost 12, async via spawn_blocking | Industry standard, 250ms acceptable, prevents blocking |
| **JWT Validation** | jsonwebtoken crate, JWKS caching, RS256/ES256 | Battle-tested library, reduces HTTP requests 99% |
| **Middleware Architecture** | AuthService (library) → AuthMiddleware (HTTP) | Reusable auth logic, clean separation of concerns |
| **CLI Credentials** | ~/.kalam/credentials.toml with 0600 perms | Consistent with config.toml, file perms for security, multi-instance |
| **Migration** | 3-phase gradual (dual auth → deprecation → removal) | Zero downtime, 6-month notice, user choice |
| **Cost Factor** | 12 (not 13 or 14) | Balance security and UX (~250ms vs 500ms+) |
| **Algorithm Support** | RS256, ES256, HS256 | Cover external OAuth and internal tokens |
| **Issuer Validation** | Allowlist in config.toml | Prevents token substitution attacks |
| **Clock Skew** | 60 seconds leeway | Handles typical NTP drift |
| **User Existence Check** | Optional (config flag) | Flexibility for different security models |

---

## Implementation Readiness

✅ **All NEEDS CLARIFICATION items resolved**  
✅ **Architectural decisions documented with rationale**  
✅ **Alternatives considered and rejected reasons captured**  
✅ **Configuration approach defined (config.toml)**  
✅ **Security considerations addressed**  
✅ **Performance characteristics estimated**  
✅ **Migration path defined**

**Ready to proceed to Phase 1: Data Model & Contracts**
