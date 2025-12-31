# KalamDB Security Vulnerability Audit Report

**Audit Date:** December 31, 2025  
**Status:** In Progress

## Summary

| Severity | Count | Status |
|----------|-------|--------|
| 🔴 **High** | 5 | Fixing in progress |
| 🟡 **Medium** | 9 | Pending |
| 🟢 **Low** | 10 | Pending |

---

## 🔴 High Severity Issues

### 1. Hardcoded JWT Secrets (HIGH) - FIXING
**Location:** 
- `backend/crates/kalamdb-commons/src/config/defaults.rs`
- `backend/crates/kalamdb-auth/src/unified.rs`
- `backend/src/config.rs`

**Issue:** Multiple hardcoded default JWT secrets exist (`"kalamdb-dev-secret-key-change-in-production"`, `"CHANGE_ME_IN_PRODUCTION"`). While startup validation exists, inconsistent defaults create risk.

**Fix:** Consolidate to single source of truth, ensure startup validation catches all cases.

---

### 2. Storage Credentials Not Encrypted (HIGH) - FIXING
**Location:** `backend/crates/kalamdb-system/src/storages.rs`

**Issue:** Storage credentials are documented as "Encrypted credentials JSON" but no encryption implementation exists - credentials stored in plaintext.

**Fix:** Either implement actual encryption or update documentation to reflect plaintext storage with appropriate warnings.

---

### 3. Passwords Logged in SQL (HIGH) - FIXING
**Location:** 
- `backend/crates/kalamdb-api/src/handlers/sql_handler.rs`
- `backend/crates/kalamdb-core/src/tables/user_tables/table_provider.rs`

**Issue:** Raw SQL statements including `ALTER USER ... SET PASSWORD` are logged in debug mode, exposing passwords in log files.

**Fix:** Implement `redact_sensitive_sql()` function to sanitize password-containing SQL before logging.

---

### 4. UserId Without Validation (HIGH) - FIXING
**Location:** `backend/crates/kalamdb-commons/src/models/id.rs`

**Issue:** `UserId::new()` accepts any string without validation. UserId is substituted into storage paths via `{userId}` placeholder - could allow `../../../etc/passwd` injection.

**Fix:** Add validation to reject path traversal characters (`..`, `/`, `\`, `\0`).

---

### 5. Backup Path Not Sanitized (HIGH) - FIXING
**Location:** `backend/crates/kalamdb-core/src/sql/executor/handlers/backup.rs`

**Issue:** `BACKUP DATABASE ... TO` path is extracted from SQL without validation - allows writing to arbitrary filesystem locations.

**Fix:** Validate paths to block `..` sequences and optionally restrict to allowed directories.

---

## 🟡 Medium Severity Issues

### 6. No Token Revocation (MEDIUM)
**Location:** N/A (missing feature)

**Issue:** No JWT token blacklist/revocation mechanism exists. Compromised tokens remain valid until expiration (default 24 hours).

**Recommendation:** Implement token blacklist or use short-lived tokens with refresh tokens.

---

### 7. Cookie Secure=false by Default (MEDIUM)
**Location:** 
- `backend/crates/kalamdb-commons/src/config/cookie.rs`
- `backend/src/config.rs`

**Issue:** Auth cookies default to `Secure: false`, allowing transmission over HTTP and potential session hijacking.

**Recommendation:** Default to `true` and require explicit opt-out for development.

---

### 8. CORS Allows Any Origin (MEDIUM)
**Location:** 
- `backend/crates/kalamdb-commons/src/config/cors.rs`
- `backend/src/middleware.rs`

**Issue:** Default CORS configuration allows any origin (`allowed_origins: []` = wildcard), enabling cross-origin attacks on authenticated endpoints.

**Recommendation:** Require explicit origin configuration.

---

### 9. CREATE STORAGE Path Not Validated (MEDIUM)
**Location:** `backend/crates/kalamdb-core/src/sql/executor/handlers/create_storage.rs`

**Issue:** Base directory from `CREATE STORAGE` is not validated for path traversal or sensitive directories.

**Recommendation:** Validate paths, block `..` and sensitive directories like `/etc`, `/root`.

---

### 10. Unbounded Login Credentials (MEDIUM)
**Location:** `backend/crates/kalamdb-api/src/handlers/auth_handlers.rs`

**Issue:** `LoginRequest` struct accepts unbounded username/password strings. Attackers could send extremely long passwords (up to 10MB body limit).

**Recommendation:** Add `#[serde(deserialize_with = "...")]` validators for length limits.

---

### 11. UserId Interpolation in WHERE Clause (MEDIUM)
**Location:** `backend/crates/kalamdb-core/src/live/filter.rs`

**Issue:** `resolve_where_clause_placeholders()` interpolates user ID into SQL strings without escaping single quotes.

**Recommendation:** Add quote escaping: `user_id.as_str().replace("'", "''")`

---

### 12. Internal Errors Exposed (MEDIUM)
**Location:** 
- `backend/crates/kalamdb-api/src/handlers/sql_handler.rs`
- `backend/crates/kalamdb-core/src/error.rs`

**Issue:** SQL execution errors include full internal error messages (DataFusion, Arrow, RocksDB details) in HTTP responses.

**Recommendation:** Sanitize error messages, use generic messages for internal errors.

---

### 13. Unverified Claims for Audit Logging (MEDIUM)
**Location:** `backend/crates/kalamdb-auth/src/jwt.rs`

**Issue:** `extract_claims_unverified()` with `insecure_disable_signature_validation()` is used for audit logging - allows log poisoning with forged usernames.

**Recommendation:** Mark audit logs from unverified tokens as "unverified" or validate first.

---

### 14. Bincode Without Size Limits (MEDIUM)
**Location:** 
- `backend/crates/kalamdb-core/src/cache/query_cache.rs`
- Entity store modules

**Issue:** Bincode deserialization does not set size limits, potentially allowing memory exhaustion from corrupted cache entries.

**Recommendation:** Use `bincode::config::standard().with_limit::<{16 * 1024 * 1024}>()`.

---

## 🟢 Low Severity Issues

### 15. Limited Common Password List (LOW)
**Location:** `backend/crates/kalamdb-auth/src/password.rs`

**Issue:** Only 24 common passwords blocked. Industry standard lists contain 10,000+ passwords.

**Recommendation:** Load comprehensive password list from file.

---

### 16. Password Complexity Off by Default (LOW)
**Location:** `backend/crates/kalamdb-commons/src/config/defaults.rs`

**Issue:** `enforce_complexity: false` by default - users can set weak passwords.

**Recommendation:** Consider enabling by default or per-role policies.

---

### 17. Timing Leak for Empty Passwords (LOW)
**Location:** `backend/crates/kalamdb-auth/src/password.rs`

**Issue:** Short-circuit `||` evaluation means empty password check has different timing than bcrypt verification.

**Recommendation:** Add fake bcrypt verification for empty passwords.

---

### 18. No Content-Type Enforcement (LOW)
**Location:** `backend/crates/kalamdb-api/src/handlers/sql_handler.rs`

**Issue:** JSON endpoints don't strictly enforce `application/json` Content-Type header.

**Recommendation:** Add Actix-Web's `JsonConfig` middleware.

---

### 19. namespace_id No API Validation (LOW)
**Location:** `backend/crates/kalamdb-api/src/models/sql_request.rs`

**Issue:** `namespace_id` passed directly without upfront validation at API boundary.

**Recommendation:** Validate at deserialization time.

---

### 20. Missing redact_sensitive_data Function (LOW)
**Location:** `backend/src/logging.rs`

**Issue:** Tests reference `redact_sensitive_data` function that doesn't exist.

**Recommendation:** Implement the function or fix tests.

---

### 21. Silent JSON Parse Failure (LOW)
**Location:** `backend/crates/kalamdb-core/src/namespace/alter.rs`

**Issue:** `unwrap_or(json!({}))` silently swallows parsing errors.

**Recommendation:** Log parsing failures.

---

### 22. WebSocket Conversion Errors Exposed (LOW)
**Location:** `backend/crates/kalamdb-api/src/ws/ws_handler.rs`

**Issue:** WebSocket error messages include internal details from row conversion failures.

**Recommendation:** Sanitize error messages.

---

### 23. No TLS Configuration (LOW)
**Location:** `backend/src/lifecycle.rs`

**Issue:** Server binds to plain HTTP. TLS must be handled by reverse proxy.

**Recommendation:** Document reverse proxy requirement or add native TLS.

---

### 24. No JWKS/RS256 Support (LOW)
**Location:** `backend/crates/kalamdb-auth/src/jwt.rs`

**Issue:** OAuth providers using RS256 cannot be integrated.

**Recommendation:** Implement JWKS support for OAuth integration.

---

## Implementation Progress

- [ ] #1 - JWT Secret Consolidation
- [ ] #2 - Storage Credentials Encryption
- [ ] #3 - SQL Password Logging
- [ ] #4 - UserId Validation
- [ ] #5 - Backup Path Sanitization
- [ ] #6-24 - Pending future implementation

---

## References

- OWASP Top 10: https://owasp.org/Top10/
- CWE Path Traversal: https://cwe.mitre.org/data/definitions/22.html
- CWE SQL Injection: https://cwe.mitre.org/data/definitions/89.html
