# Authentication Performance Optimization

**Feature**: User Authentication (007-user-auth)  
**Date**: October 27, 2025  
**Status**: Performance Analysis & Caching Strategy

---

## Overview

This document analyzes the authentication performance characteristics and proposes caching strategies to minimize latency and database lookups. The goal is to achieve **<50ms authentication overhead** (95th percentile) for JWT validation.

---

## Current Authentication Flow (Without Caching)

### HTTP Basic Auth Flow

```
Request → AuthMiddleware
  → Extract Basic Auth header (0.1ms)
  → Decode base64 (0.1ms)
  → AuthService::authenticate_basic()
    → RocksDB lookup by username (1-5ms) ⚠️
    → bcrypt verify_password (250ms) ⚠️ CPU-INTENSIVE
    → Create AuthenticatedUser (0.1ms)
  → Set in request.extensions() (0.1ms)

Total: ~250-260ms per request
Bottleneck: bcrypt verification (necessary for security)
```

**Performance Characteristics**:
- ❌ **250ms minimum** due to bcrypt cost factor 12
- ✅ Cannot be cached (password changes, revoked users)
- ✅ Async execution prevents blocking (spawn_blocking)
- 📊 **Acceptable for login operations** (happens once per session)

**Optimization**: Use JWT tokens after initial Basic Auth login to avoid repeated bcrypt operations.

---

### JWT Validation Flow (Current - No User Caching)

```
Request → AuthMiddleware
  → Extract Bearer token (0.1ms)
  → AuthService::authenticate_jwt()
    → JwtValidator::validate()
      → decode_header (0.5ms)
      → get_decoding_key (0.5-50ms) ⚠️
        → If RS256/ES256: JWKS cache lookup (0.5ms) or HTTP fetch (50-200ms)
        → If HS256: immediate from config (0.1ms)
      → jsonwebtoken::decode() (1-2ms) ⚠️ SIGNATURE VERIFICATION
      → validate claims (exp, iss, nbf) (0.2ms)
    → RocksDB lookup by user_id (1-5ms) ⚠️
    → Check deleted_at != NULL (0.1ms)
    → Create AuthenticatedUser (0.1ms)
  → Set in request.extensions() (0.1ms)

Total (cached JWKS): ~3-10ms per request ✅
Total (uncached JWKS): ~50-210ms per request ⚠️
Total (HS256): ~2-8ms per request ✅
```

**Performance Breakdown**:
- JWKS caching: **Saves 50-200ms** (HTTP request to OAuth provider)
- JWT signature verification: **1-2ms** (cryptographic operation, unavoidable)
- User lookup in RocksDB: **1-5ms** (can be cached! ⚡)

---

## Proposed Caching Strategy

### 1. JWT Token Cache (Claims Cache)

**Purpose**: Cache decoded JWT claims to skip signature verification for repeated requests

**Implementation**: In-memory LRU cache with TTL

```rust
use lru::LruCache;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::time::{Duration, Instant};

struct CachedToken {
    claims: Claims,
    cached_at: Instant,
}

pub struct TokenCache {
    cache: Arc<RwLock<LruCache<String, CachedToken>>>,
    ttl: Duration,
}

impl TokenCache {
    pub fn new(capacity: usize, ttl_seconds: u64) -> Self {
        Self {
            cache: Arc::new(RwLock::new(LruCache::new(capacity))),
            ttl: Duration::from_secs(ttl_seconds),
        }
    }
    
    pub async fn get(&self, token: &str) -> Option<Claims> {
        let cache = self.cache.read().await;
        
        if let Some(cached) = cache.peek(token) {
            // Check if cache entry is still valid
            if cached.cached_at.elapsed() < self.ttl {
                return Some(cached.claims.clone());
            }
        }
        
        None
    }
    
    pub async fn set(&self, token: String, claims: Claims) {
        let mut cache = self.cache.write().await;
        cache.put(token, CachedToken {
            claims,
            cached_at: Instant::now(),
        });
    }
    
    pub async fn invalidate(&self, token: &str) {
        let mut cache = self.cache.write().await;
        cache.pop(token);
    }
    
    pub async fn clear(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }
}
```

**Configuration**:
```toml
[authentication.jwt.cache]
# Enable JWT token caching (default: true)
enabled = true

# Maximum number of cached tokens (LRU eviction)
capacity = 10000

# Cache TTL in seconds (default: 300 = 5 minutes)
# Should be LESS than token exp to prevent caching expired tokens
ttl = 300

# Cache hit ratio logging interval (0 = disabled)
log_stats_interval_seconds = 60
```

**Cache Key**: SHA-256 hash of the raw JWT token (to prevent memory exhaustion from long tokens)

**Performance Impact**:
- ✅ **Cache hit**: ~0.5ms (hash lookup + clone)
- ⚠️ **Cache miss**: ~3-10ms (full validation + cache write)
- 📊 **Expected hit rate**: 95%+ (same token reused across requests in short time window)

**Trade-offs**:

| Aspect | Benefit | Risk | Mitigation |
|--------|---------|------|------------|
| **Speed** | 10-20x faster on cache hit | Stale data if user deleted | TTL < 5 min, user cache sync |
| **Memory** | ~500 bytes per token × 10k = 5MB | Memory exhaustion | LRU eviction + capacity limit |
| **Security** | None | Revoked tokens accepted during TTL | Short TTL (5 min default) |

**When to Clear Cache**:
1. User deleted/updated → Invalidate all tokens for that user_id
2. Configuration change → Clear entire cache
3. Manual admin command → Clear entire cache

---

### 2. User Lookup Cache

**Purpose**: Cache `User` records from RocksDB to skip database lookups

**Implementation**: In-memory cache keyed by `user_id` and `username`

```rust
use moka::future::Cache;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
pub struct CachedUser {
    pub user_id: String,
    pub username: String,
    pub email: Option<String>,
    pub role: UserRole,
    pub auth_type: String,
    pub auth_data: Option<String>,
    pub deleted_at: Option<DateTime<Utc>>,
}

pub struct UserCache {
    // Cache by user_id (primary)
    by_id: Cache<String, Arc<CachedUser>>,
    // Cache by username (for Basic Auth lookups)
    by_username: Cache<String, Arc<CachedUser>>,
}

impl UserCache {
    pub fn new(capacity: u64, ttl_seconds: u64) -> Self {
        let ttl = Duration::from_secs(ttl_seconds);
        
        Self {
            by_id: Cache::builder()
                .max_capacity(capacity)
                .time_to_live(ttl)
                .build(),
            by_username: Cache::builder()
                .max_capacity(capacity)
                .time_to_live(ttl)
                .build(),
        }
    }
    
    pub async fn get_by_id(&self, user_id: &str) -> Option<Arc<CachedUser>> {
        self.by_id.get(user_id).await
    }
    
    pub async fn get_by_username(&self, username: &str) -> Option<Arc<CachedUser>> {
        self.by_username.get(username).await
    }
    
    pub async fn insert(&self, user: CachedUser) {
        let arc_user = Arc::new(user);
        self.by_id.insert(arc_user.user_id.clone(), arc_user.clone()).await;
        self.by_username.insert(arc_user.username.clone(), arc_user).await;
    }
    
    pub async fn invalidate_user(&self, user_id: &str, username: &str) {
        self.by_id.invalidate(user_id).await;
        self.by_username.invalidate(username).await;
    }
    
    pub async fn clear(&self) {
        self.by_id.invalidate_all();
        self.by_username.invalidate_all();
    }
}
```

**Configuration**:
```toml
[authentication.user_cache]
# Enable user record caching (default: true)
enabled = true

# Maximum number of cached users
capacity = 50000

# Cache TTL in seconds (default: 600 = 10 minutes)
ttl = 600

# Check deleted_at on every request even if cached
# Prevents deleted users from being cached
always_check_deleted = true
```

**Cache Library Choice**: Use **moka** instead of LRU
- Async-friendly (built for Tokio)
- Automatic TTL expiration (no manual cleanup)
- Thread-safe without RwLock
- Better performance for high concurrency

**Dependency**:
```toml
[dependencies]
moka = { version = "0.12", features = ["future"] }
```

**Performance Impact**:
- ✅ **Cache hit**: ~0.1ms (hash map lookup)
- ⚠️ **Cache miss**: ~1-5ms (RocksDB read + cache write)
- 📊 **Expected hit rate**: 99%+ (same users making multiple requests)

**Invalidation Strategy**:

| Event | Action |
|-------|--------|
| User created | Insert into cache immediately |
| User updated | Invalidate by user_id + username, next request refreshes |
| User deleted (soft) | Invalidate by user_id + username |
| User restored | Invalidate by user_id + username |
| Role changed | Invalidate (CRITICAL for authorization) |
| Password changed | Invalidate (old Basic Auth credentials fail anyway) |
| last_seen updated | Do NOT invalidate (updated async, non-critical field) |

---

### 3. JWKS Cache (Already Planned)

**Status**: ✅ Already documented in research.md

**Current Strategy**:
- Cache public keys from `/.well-known/jwks.json`
- 1-hour TTL (3600 seconds)
- Invalidate on signature verification failure
- HashMap keyed by `(issuer, key_id)`

**Enhancement**: Add background refresh before expiration

```rust
pub struct JwksCache {
    cache: Arc<RwLock<HashMap<String, CachedJwk>>>,
    ttl: Duration,
    refresh_before: Duration,  // NEW: Refresh 5 min before expiry
}

struct CachedJwk {
    key: DecodingKey,
    cached_at: Instant,
}

impl JwksCache {
    async fn get_or_refresh(&self, issuer: &str, kid: &str) -> Result<DecodingKey, JwksError> {
        let cache = self.cache.read().await;
        
        if let Some(cached) = cache.get(&(issuer.to_string(), kid.to_string())) {
            let age = cached.cached_at.elapsed();
            
            // If within refresh window, trigger background refresh
            if age > (self.ttl - self.refresh_before) && age < self.ttl {
                let issuer = issuer.to_string();
                let kid = kid.to_string();
                let cache_clone = self.cache.clone();
                
                tokio::spawn(async move {
                    // Refresh in background without blocking request
                    if let Ok(new_key) = fetch_jwks_key(&issuer, &kid).await {
                        let mut cache = cache_clone.write().await;
                        cache.insert((issuer, kid), CachedJwk {
                            key: new_key,
                            cached_at: Instant::now(),
                        });
                    }
                });
            }
            
            return Ok(cached.key.clone());
        }
        
        // Cache miss - fetch synchronously
        drop(cache);
        self.fetch_and_cache(issuer, kid).await
    }
}
```

**Configuration**:
```toml
[authentication.jwt.jwks_cache]
# JWKS cache TTL (default: 3600 = 1 hour)
ttl = 3600

# Refresh keys this many seconds before expiry (default: 300 = 5 min)
# Prevents expiry during high traffic
refresh_before_expiry = 300

# Maximum concurrent JWKS fetches (prevent thundering herd)
max_concurrent_fetches = 5
```

---

## Integrated Authentication Flow (With All Caches)

### Optimized JWT Flow

```
Request → AuthMiddleware
  → Extract Bearer token (0.1ms)
  → SHA-256 hash token for cache key (0.3ms)
  → TokenCache::get(token_hash)
    ├─ CACHE HIT (95%+): Return cached claims (0.1ms) ✅
    │   → UserCache::get_by_id(claims.sub)
    │     ├─ CACHE HIT (99%+): Return cached user (0.1ms) ✅
    │     └─ CACHE MISS: RocksDB lookup (1-5ms) → cache it
    │   → Create AuthenticatedUser (0.1ms)
    │   → Spawn async last_seen update (0.1ms, non-blocking) ⚡
    │   → Total: ~0.5ms ⚡⚡⚡
    │
    └─ CACHE MISS (5%):
        → JwtValidator::validate(token)
          → decode_header (0.5ms)
          → JwksCache::get_or_refresh(issuer, kid)
            ├─ CACHE HIT (99%+): Return key (0.1ms)
            └─ CACHE MISS: HTTP fetch (50-200ms)
          → jsonwebtoken::decode() (1-2ms)
          → validate claims (0.2ms)
        → TokenCache::set(token_hash, claims) (0.1ms)
        → UserCache::get_by_id(claims.sub) (0.1-5ms)
        → Create AuthenticatedUser (0.1ms)
        → Spawn async last_seen update (0.1ms, non-blocking) ⚡
        → Total: ~3-10ms (cached JWKS) or ~50-210ms (uncached JWKS)

Overall Average (95% token cache hit): ~0.5-1ms ⚡⚡⚡
Overall Average (5% token cache miss): ~3-10ms ✅

Note: last_seen update happens asynchronously and doesn't block response
```

**Performance Improvement**:
- **Before caching**: 3-10ms per request (with JWKS cache)
- **After caching**: 0.5-1ms per request (95th percentile)
- **Speedup**: 5-10x faster 🚀

---

## Memory Usage Estimation

| Cache | Capacity | Size per Entry | Total Memory |
|-------|----------|----------------|--------------|
| **Token Cache** | 10,000 | ~500 bytes (SHA-256 hash + Claims) | ~5 MB |
| **User Cache** | 50,000 | ~300 bytes (CachedUser struct) | ~15 MB |
| **JWKS Cache** | 100 keys | ~2 KB (RSA public key) | ~200 KB |
| **Total** | - | - | **~20 MB** |

**Scaling**:
- For 1M active users: Increase user_cache capacity to 100k → ~30 MB
- For 100k concurrent sessions: Increase token_cache to 100k → ~50 MB
- **Total at scale**: ~100 MB (negligible on modern servers)

---

## Cache Coherence & Invalidation

### Problem: Stale Data

**Scenario**: Admin deletes user → User's JWT still cached → Deleted user can access system for up to 5 minutes

**Solutions**:

#### 1. Proactive Invalidation (Recommended)

```rust
// In user management service

pub async fn delete_user(&self, user_id: &str) -> Result<(), Error> {
    // 1. Soft delete in database
    let user = self.mark_deleted(user_id).await?;
    
    // 2. Invalidate caches IMMEDIATELY
    self.token_cache.invalidate_by_user_id(user_id).await;
    self.user_cache.invalidate_user(user_id, &user.username).await;
    
    Ok(())
}
```

**Token cache needs reverse index**:
```rust
pub struct TokenCache {
    // Original: token_hash → claims
    by_token: Cache<String, Claims>,
    
    // NEW: user_id → set of token_hashes
    by_user: Arc<RwLock<HashMap<String, HashSet<String>>>>,
}

impl TokenCache {
    pub async fn set(&self, token_hash: String, claims: Claims) {
        // Store in primary cache
        self.by_token.insert(token_hash.clone(), claims.clone()).await;
        
        // Store in reverse index
        let mut by_user = self.by_user.write().await;
        by_user.entry(claims.sub.clone())
            .or_insert_with(HashSet::new)
            .insert(token_hash);
    }
    
    pub async fn invalidate_by_user_id(&self, user_id: &str) {
        // Get all token hashes for this user
        let mut by_user = self.by_user.write().await;
        
        if let Some(token_hashes) = by_user.remove(user_id) {
            drop(by_user);  // Release write lock
            
            // Invalidate all tokens
            for token_hash in token_hashes {
                self.by_token.invalidate(&token_hash).await;
            }
        }
    }
}
```

**Memory overhead**: ~50 bytes per token for reverse index (acceptable)

#### 2. Always Check deleted_at (Paranoid Mode)

```rust
pub async fn authenticate_jwt(&self, token: &str) -> Result<AuthenticatedUser, AuthError> {
    // 1. Validate JWT (from cache or full validation)
    let claims = self.validate_jwt_with_cache(token).await?;
    
    // 2. Get user record (from cache or DB)
    let user = self.get_user_by_id(&claims.sub).await?;
    
    // 3. ALWAYS check deleted_at, even if from cache
    if user.deleted_at.is_some() {
        // Invalidate cache immediately
        self.user_cache.invalidate_user(&user.user_id, &user.username).await;
        self.token_cache.invalidate_by_user_id(&user.user_id).await;
        
        return Err(AuthError::UserDeleted);
    }
    
    Ok(AuthenticatedUser::from_user(user))
}
```

**Trade-off**: Adds ~0.1ms per request but guarantees no stale deleted users

#### 3. Cache Versioning (Future Enhancement)

```rust
// Global cache version (increment on any user change)
static CACHE_VERSION: AtomicU64 = AtomicU64::new(0);

struct CachedUser {
    user: User,
    version: u64,  // Cache version when stored
}

impl UserCache {
    pub async fn get_by_id(&self, user_id: &str) -> Option<User> {
        if let Some(cached) = self.by_id.get(user_id).await {
            // Invalidate if version mismatch
            if cached.version != CACHE_VERSION.load(Ordering::Relaxed) {
                self.by_id.invalidate(user_id).await;
                return None;
            }
            return Some(cached.user.clone());
        }
        None
    }
}

// On any user change:
pub fn invalidate_all_caches() {
    CACHE_VERSION.fetch_add(1, Ordering::Relaxed);
}
```

**Note**: Overkill for single-instance deployments, useful for distributed systems

---

## Configuration Recommendations

### Development Environment

```toml
[authentication.jwt.cache]
enabled = true
capacity = 1000
ttl = 60  # 1 minute (fast testing)

[authentication.user_cache]
enabled = true
capacity = 1000
ttl = 60  # 1 minute

[authentication.jwt.jwks_cache]
ttl = 300  # 5 minutes
refresh_before_expiry = 60  # 1 minute
```

### Production Environment

```toml
[authentication.jwt.cache]
enabled = true
capacity = 50000  # ~25MB
ttl = 300  # 5 minutes (balance security & performance)
log_stats_interval_seconds = 300  # Log hit rate every 5 min

[authentication.user_cache]
enabled = true
capacity = 100000  # ~30MB
ttl = 600  # 10 minutes
always_check_deleted = true  # Paranoid mode

[authentication.jwt.jwks_cache]
ttl = 3600  # 1 hour
refresh_before_expiry = 300  # 5 minutes
max_concurrent_fetches = 10
```

### High-Security Environment

```toml
[authentication.jwt.cache]
enabled = false  # Disable token caching
# Every request validates signature

[authentication.user_cache]
enabled = true
capacity = 10000
ttl = 60  # 1 minute only
always_check_deleted = true

[authentication.jwt.jwks_cache]
ttl = 300  # 5 minutes (shorter rotation)
refresh_before_expiry = 60
```

---

## Monitoring & Observability

### Metrics to Track

```rust
#[derive(Default)]
pub struct AuthMetrics {
    // Token cache
    pub token_cache_hits: AtomicU64,
    pub token_cache_misses: AtomicU64,
    
    // User cache
    pub user_cache_hits: AtomicU64,
    pub user_cache_misses: AtomicU64,
    
    // JWKS cache
    pub jwks_cache_hits: AtomicU64,
    pub jwks_cache_misses: AtomicU64,
    pub jwks_fetch_failures: AtomicU64,
    
    // Authentication
    pub auth_success_count: AtomicU64,
    pub auth_failure_count: AtomicU64,
    pub auth_duration_ms: AtomicU64,  // Rolling average
    
    // Cache invalidations
    pub cache_invalidations: AtomicU64,
}
```

### Log Examples

```
[INFO] Auth cache stats: token_hit_rate=96.5%, user_hit_rate=99.2%, jwks_hit_rate=99.8%
[INFO] Average auth latency: p50=0.6ms, p95=1.2ms, p99=8.5ms
[WARN] JWKS fetch failed for issuer=https://accounts.google.com (retrying with cache)
[INFO] User user_abc123 deleted, invalidated 15 cached tokens
```

### Health Check Endpoint

```rust
GET /api/health/auth-cache

Response:
{
  "token_cache": {
    "enabled": true,
    "size": 8234,
    "capacity": 50000,
    "hit_rate": 0.965,
    "memory_mb": 4.2
  },
  "user_cache": {
    "enabled": true,
    "size": 12456,
    "capacity": 100000,
    "hit_rate": 0.992,
    "memory_mb": 3.8
  },
  "jwks_cache": {
    "enabled": true,
    "size": 12,
    "capacity": 100,
    "hit_rate": 0.998,
    "last_fetch": "2025-10-27T14:23:45Z"
  }
}
```

---

## Implementation Phases

### Phase 1: JWKS Cache (Priority: High)
- ✅ Already planned in research.md
- Implement HashMap-based cache with 1-hour TTL
- Add background refresh logic
- **Expected Impact**: Eliminates 50-200ms OAuth provider latency

### Phase 2: User Cache (Priority: High)
- Implement moka-based user cache
- Add by-id and by-username indexes
- Implement invalidation on user changes
- **Expected Impact**: Saves 1-5ms RocksDB lookup per request

### Phase 3: Token Cache (Priority: Medium)
- Implement LRU token cache with reverse index
- Add proactive invalidation on user deletion
- Monitor hit rate and tune TTL
- **Expected Impact**: Saves 1-2ms JWT signature verification (95% hit rate)

### Phase 4: Monitoring (Priority: Low)
- Add Prometheus metrics
- Implement health check endpoint
- Add periodic stats logging
- **Expected Impact**: Operational visibility

---

## Security Considerations

### Cache Timing Attacks

**Risk**: Attacker measures response time to determine if user exists in cache

**Mitigation**: Add constant-time delay jitter
```rust
// Add 0-2ms random delay to all auth responses
let jitter = rand::thread_rng().gen_range(0..2);
tokio::time::sleep(Duration::from_millis(jitter)).await;
```

### Cache Poisoning

**Risk**: Attacker injects invalid data into cache

**Mitigation**:
1. Validate all data before caching (already done during JWT validation)
2. Sign cached entries with HMAC (overkill for in-memory cache)
3. Use separate cache instances per tenant (if multi-tenant)

### Memory Exhaustion

**Risk**: Attacker sends many requests with different tokens to fill cache

**Mitigation**:
1. Use LRU eviction (oldest entries removed)
2. Capacity limits enforced (10k-100k tokens)
3. Rate limiting at HTTP layer (separate concern)

### Stale Permissions

**Risk**: User role changed, but cached token still has old role

**Mitigation**:
1. Short TTL (5 minutes default)
2. Proactive invalidation on role change
3. `always_check_deleted` flag for paranoid mode
4. Include role version in JWT claims (future enhancement)

---

## Benchmarks (Projected)

### Before Caching
```
Benchmark: 1000 concurrent JWT auth requests

RocksDB lookup: 1-5ms
JWT validation: 1-2ms
JWKS fetch (miss): 50-200ms (5% of requests)

Average latency: ~8ms (with 95% JWKS hit rate)
p95 latency: ~15ms
p99 latency: ~180ms (JWKS cache miss)
Throughput: ~8000 req/sec
```

### After Full Caching
```
Benchmark: 1000 concurrent JWT auth requests

Token cache hit (95%): ~0.5ms
Token cache miss (5%): ~3ms
User cache hit (99%): ~0.1ms
User cache miss (1%): ~1-5ms

Average latency: ~0.8ms ⚡
p95 latency: ~1.5ms ⚡
p99 latency: ~8ms ⚡
Throughput: ~50000 req/sec 🚀
```

**Improvement**: **6x faster, 6x higher throughput** 🎉

---

## Recommendations Summary

### Must-Have (Include in MVP)
1. ✅ **JWKS Cache**: Already planned, critical for OAuth performance
2. ✅ **User Cache**: Simple moka implementation, huge performance win
3. ✅ **Proactive Invalidation**: Invalidate caches on user deletion/update

### Should-Have (Include in v1.1)
4. **Token Cache**: Add after user cache, measure impact first
5. **Background JWKS Refresh**: Prevent cache expiry during traffic spikes
6. **Monitoring**: Expose cache hit rates and latency metrics

### Nice-to-Have (Future)
7. **Cache Versioning**: For distributed deployments
8. **Per-Tenant Caches**: For multi-tenant environments
9. **Redis-Backed Caches**: For horizontal scaling (shared cache across instances)

---

## Updated Implementation Plan

Add to `specs/007-user-auth/plan.md`:

### Phase 1: Core Authentication (Update)
- [x] Implement JWKS cache with 1-hour TTL
- [NEW] Implement moka-based user cache (by-id, by-username)
- [NEW] Add cache invalidation on user CRUD operations

### Phase 2: Performance Optimization (New)
- [ ] Add JWT token cache with reverse index
- [ ] Implement background JWKS refresh
- [ ] Add cache hit rate metrics
- [ ] Create `/health/auth-cache` endpoint

### Testing (Update)
- [NEW] Load test: 10k concurrent requests, measure cache hit rates
- [NEW] Security test: Verify deleted user cannot authenticate after invalidation
- [NEW] Benchmark: Compare latency before/after caching (target: <1ms p95)

---

## Conclusion

**Recommended Approach**:

1. **Implement User Cache** (HIGH PRIORITY)
   - Saves 1-5ms per request
   - Simple to implement with moka
   - 99%+ hit rate expected

2. **Enhance JWKS Cache** (MEDIUM PRIORITY)
   - Add background refresh
   - Prevent cache expiry spikes

3. **Add Token Cache** (OPTIONAL)
   - Measure user cache impact first
   - Only add if still seeing >2ms p95 latency
   - More complex (reverse index needed)

**Performance Target**:
- ✅ **P50**: <1ms (achievable with user cache alone)
- ✅ **P95**: <2ms (requires user cache + JWKS background refresh)
- ✅ **P99**: <10ms (requires token cache or acceptable)

**Memory Budget**: ~20-100 MB (negligible on modern servers)

**Security**: Short TTLs (5-10 min) + proactive invalidation = acceptable risk

---

**Status**: ✅ Ready for review and integration into plan.md
