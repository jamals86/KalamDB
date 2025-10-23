# ADR-010: API Versioning Strategy

**Date**: 2025-10-23  
**Status**: Accepted  
**Context**: Phase 2a - User Story 14 (004-system-improvements-and)

## Context

As KalamDB evolves, the REST API and WebSocket protocol will need to change to support new features, improve performance, and fix design issues. Without a versioning strategy, these changes could break existing clients and integrations, making it impossible to deploy updates without coordinating with all API consumers.

### Problem Statement

1. **Breaking Changes**: How do we introduce breaking changes (e.g., changing response format, renaming fields) without disrupting existing clients?
2. **Backward Compatibility**: How long should we support old API versions?
3. **Migration Path**: How do clients migrate from one version to another?
4. **Version Discovery**: How do clients know which API version they're using?

### Requirements

- **Explicit versioning**: API version must be clear in the URL
- **Multiple version support**: Server must support multiple API versions simultaneously
- **Graceful deprecation**: Old versions should be deprecated with advance notice
- **No implicit upgrades**: Clients should explicitly opt into new versions

## Decision

We will use **URL path-based API versioning** with the following structure:

```
/v{major}/api/{endpoint}
/v{major}/ws
```

### Versioning Rules

1. **Major Version Increments** (`v1` → `v2`) indicate **breaking changes**:
   - Changed response structure
   - Removed or renamed fields
   - Changed authentication mechanism
   - Changed WebSocket protocol format
   - Changed error response format

2. **Non-Breaking Changes** can be added to existing versions:
   - New optional request fields
   - New response fields (additive only)
   - New endpoints
   - Performance improvements
   - Bug fixes

3. **Version Support Policy**:
   - **Current version**: Fully supported with new features
   - **Previous version**: Supported for **6 months** after new version release
   - **Deprecated versions**: 3-month deprecation warning before removal
   - **Emergency security fixes**: Applied to all supported versions

### Endpoint Structure

#### Current Version (v1)

```
POST /v1/api/sql           # Execute SQL commands
GET  /v1/api/healthcheck   # Server health status
WS   /v1/ws                # WebSocket subscriptions
```

#### Future Version (v2) - Example

```
POST /v2/api/sql           # Execute SQL with new response format
GET  /v2/api/healthcheck   # Enhanced health metrics
WS   /v2/ws                # WebSocket with improved protocol
```

### Version Discovery

Clients can discover supported API versions through:

1. **Healthcheck Response**:
```json
{
  "status": "healthy",
  "version": "0.1.0",
  "api_versions": ["v1"],
  "deprecated_versions": [],
  "uptime_seconds": 3600
}
```

2. **Documentation**: API_REFERENCE.md documents all supported versions

3. **Error Response** for invalid version:
```json
{
  "status": "error",
  "error": "API version 'v3' not supported. Supported versions: v1, v2",
  "error_type": "UnsupportedApiVersion"
}
```

### Migration Example

When migrating from v1 to v2:

**Phase 1: Announcement** (6 months before v2 release)
- Document upcoming breaking changes
- Provide migration guide
- Add deprecation warnings to v1 responses (optional header)

**Phase 2: v2 Release**
- Deploy v2 endpoints alongside v1
- Update documentation
- Provide example code for both versions

**Phase 3: v1 Deprecation** (6 months after v2 release)
- Add `X-Api-Deprecated: true` header to v1 responses
- Log deprecation warnings
- Send email notifications to known API consumers

**Phase 4: v1 Removal** (9 months after v2 release)
- Remove v1 endpoints
- Return 410 Gone for v1 requests

## Consequences

### Positive

1. **Clear Contract**: Clients know exactly what version they're using
2. **Safe Evolution**: Breaking changes don't affect existing clients
3. **Gradual Migration**: Clients can migrate at their own pace
4. **Testing**: Multiple versions can be tested simultaneously
5. **Monitoring**: Version-specific metrics and error rates

### Negative

1. **Code Duplication**: Multiple endpoint handlers for different versions
2. **Maintenance Burden**: Supporting multiple versions increases testing surface
3. **Documentation Overhead**: Each version needs separate documentation
4. **Database Schema**: Version-specific database changes require careful planning

### Mitigation Strategies

1. **Shared Business Logic**: Version-specific adapters wrap shared core logic
   ```rust
   // Shared core
   fn execute_sql_core(sql: &str, user_id: &str) -> Result<CoreResponse>;
   
   // Version-specific adapters
   fn v1_sql_handler(req: V1Request) -> V1Response {
       let core = execute_sql_core(&req.sql, &req.user_id)?;
       core.to_v1_response()
   }
   
   fn v2_sql_handler(req: V2Request) -> V2Response {
       let core = execute_sql_core(&req.sql, &req.user_id)?;
       core.to_v2_response()
   }
   ```

2. **Feature Flags**: Use feature flags to gradually roll out v2 features
3. **Automated Testing**: Integration tests for all supported versions
4. **Version Metrics**: Monitor adoption rates to inform deprecation decisions

## Implementation

### Phase 1: v1 Baseline (COMPLETE)

- [X] All endpoints moved to `/v1` prefix
- [X] Update kalam-link client to use `/v1/api/sql`
- [X] Update kalam-cli to use `/v1/ws` for WebSocket
- [X] Update API documentation with versioning strategy
- [X] Update integration tests to use versioned endpoints

### Phase 2: Version Infrastructure (Future)

- [ ] Add API version middleware for routing
- [ ] Implement version detection from request path
- [ ] Add `api_versions` field to healthcheck response
- [ ] Log metrics per API version (Prometheus labels)

### Phase 3: Deprecation Tooling (Future)

- [ ] Add deprecation warning header injection
- [ ] Implement deprecation logging
- [ ] Create migration guide template
- [ ] Build version compatibility matrix

## Alternatives Considered

### 1. Header-Based Versioning

```http
Accept: application/vnd.kalamdb.v1+json
```

**Rejected because**:
- Less visible to developers (hidden in headers)
- Harder to test with browser/curl
- More complex to implement routing
- Doesn't work well with WebSocket upgrades

### 2. Query Parameter Versioning

```
POST /api/sql?version=v1
```

**Rejected because**:
- Not RESTful (version is not a resource parameter)
- Query parameters can be accidentally omitted
- Doesn't work with WebSocket URLs (though `?version=v1` could work)
- Less clear in logs and monitoring

### 3. Subdomain Versioning

```
https://v1.kalamdb.example.com/api/sql
```

**Rejected because**:
- Requires DNS/certificate management per version
- Complicates local development
- Harder to route in proxies/load balancers
- Overkill for single-service API

### 4. No Versioning (Breaking Changes Allowed)

**Rejected because**:
- Breaks all existing clients on updates
- Requires coordinated deployments
- Prevents gradual migration
- Unsuitable for production use

## References

- [REST API Versioning Best Practices](https://restfulapi.net/versioning/)
- [Stripe API Versioning](https://stripe.com/docs/api/versioning)
- [GitHub API Versioning](https://docs.github.com/en/rest/overview/api-versions)
- [Semantic Versioning 2.0.0](https://semver.org/)

## Related ADRs

- ADR-009: Three-Layer Architecture (establishes API layer separation)
- Future: ADR-013: WebSocket Protocol Versioning (v1 vs v2 changes)
