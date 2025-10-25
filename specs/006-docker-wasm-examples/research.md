# Research: Docker Container, WASM Compilation, and TypeScript Examples

**Feature**: 006-docker-wasm-examples  
**Date**: 2025-10-25  
**Purpose**: Technical research and decision documentation for API key auth, soft delete, Docker deployment, WASM compilation, and React example

## Overview

This document consolidates research findings for four independent user stories requiring different technical approaches:
- Story 0 (P0): Backend modifications for API key authentication and soft delete
- Story 1 (P1): Docker containerization with configuration override
- Story 2 (P2): WASM compilation of Rust kalam-link client
- Story 3 (P3): React TypeScript example application

---

## Story 0: API Key Authentication and Soft Delete

### Decision 1: API Key Storage and Generation

**Decision**: Store auto-generated API key directly in user table as mandatory `apikey` field, auto-generated on user creation

**Rationale**:
- Simplifies implementation - no new tables or services required
- Aligns with "simple authentication for examples" goal
- User table already exists and is indexed
- Direct lookup by API key using RocksDB key-value semantics
- Every user automatically gets an API key (secure by default)
- Sufficient for demo/development use case

**Alternatives Considered**:
- Optional API key field
  - ❌ Requires handling NULL cases in authentication
  - ❌ Creates users without API access (confusing UX)
  - ✅ Would allow disabling API access per user (not required)
- Separate `api_keys` table with foreign key to users
  - ❌ Adds complexity for minimal benefit in demo context
  - ❌ Requires joins/lookups across tables
  - ✅ Would enable multiple keys per user (not required)
  - ✅ Would enable key rotation (future enhancement)
- Hash-based storage (bcrypt/argon2)
  - ❌ Overkill for API keys (not passwords)
  - ❌ Cannot query by API key directly
  - ✅ Would provide security against DB leak (not in scope)

**Implementation Notes**:
- Add `apikey: String` field to User struct (NOT NULL)
- Create index on apikey for O(1) lookups
- Auto-generate UUID v4 on user creation using `uuid::Uuid::new_v4().to_string()`
- CLI command displays generated API key to user
- Backend server command: `kalamdb-server create-user --name <name> --role <role>`

### Decision 2: Soft Delete Implementation Strategy

**Decision**: Add `deleted: bool` field to user table rows, filter in SELECT queries

**Rationale**:
- Simple boolean flag is performant and clear
- RocksDB supports filtering during iteration
- Transparent to existing code (add default filter)
- Enables future "undelete" functionality
- No schema migration complexity (default to false)

**Alternatives Considered**:
- `deleted_at: Option<Timestamp>` instead of boolean
  - ✅ Provides audit trail (when deleted)
  - ❌ More complex filtering logic
  - ✅ Enables time-based queries
  - **Verdict**: Use boolean for simplicity; can add timestamp later if needed
- Separate deleted_rows table
  - ❌ Requires moving data between tables
  - ❌ Complicates restore logic
  - ❌ Breaks row ID semantics
- Tombstone markers in RocksDB
  - ✅ Native RocksDB concept
  - ❌ Harder to query "show deleted"
  - ❌ Permanent (can't undelete)

**Implementation Notes**:
- Add `deleted: bool` field with default false
- Modify SELECT query builder to add `WHERE deleted = false` clause
- Support optional `INCLUDE DELETED` query hint for admin use
- Update DELETE handler to execute `UPDATE SET deleted = true`
- All existing tests should pass with minimal changes (filter applied automatically)

### Decision 3: User Role Field

**Decision**: Add mandatory `role` field to User table for future authorization

**Rationale**:
- Enables role-based access control (RBAC) in future iterations
- Simple string field allows for flexible role definitions
- Can start with basic roles: "admin", "user", "readonly"
- No complex permissions table needed initially
- Easy to extend later with role-permission mappings

**Alternatives Considered**:
- No role field (add later when needed)
  - ❌ Requires schema migration later
  - ❌ All users would have same permissions initially
  - ✅ Simpler initial implementation
- Separate roles table with permissions
  - ❌ Overengineered for initial use case
  - ❌ Adds complexity with joins
  - ✅ More flexible for complex RBAC
  - ✅ Future enhancement path

**Implementation Notes**:
- Add `role: String` field to User struct (NOT NULL)
- Default role for create-user command: "user"
- CLI flag: `--role admin|user|readonly`
- No authorization enforcement initially (placeholder for future)
- Validation: role must be one of predefined values

### Decision 4: API Key Authentication Middleware

**Decision**: Actix-Web middleware extracting X-API-KEY header, resolving to User context

**Rationale**:
- Actix-Web provides robust middleware framework
- Middleware runs before route handlers (consistent auth)
- Can inject User context into request extensions
- Easy to bypass for localhost (check connection source)
- Standard HTTP header pattern

**Alternatives Considered**:
- Query parameter `?apikey=...`
  - ❌ Appears in logs and URLs (security risk)
  - ❌ Not RESTful
- Bearer token in Authorization header
  - ✅ More standard OAuth-style
  - ❌ Overcomplicates for simple API key
  - ❌ Implies JWT/OAuth which is future enhancement
- Custom auth system
  - ❌ Reinventing wheel
  - ❌ Not extensible to OAuth/JWT later

**Implementation Notes**:
- Create `ApiKeyAuth` middleware in kalamdb-api
- Extract `X-API-KEY` header value
- Query user table by apikey field
- Return 401 if not found
- Inject `User` struct into request extensions
- Route handlers access via `req.extensions().get::<User>()`
- Localhost exception: skip auth if connection is 127.0.0.1

---

## Story 1: Docker Deployment with Configuration

### Decision 5: Base Docker Image

**Decision**: Use `rust:1.75-slim` as base for builder, `debian:bookworm-slim` for runtime

**Rationale**:
- Multi-stage build reduces final image size (builder vs runtime)
- rust:1.75-slim matches project Rust version
- debian:bookworm-slim is minimal, secure, well-maintained
- Debian allows easy apt install of dependencies (if needed)
- Familiar to most developers

**Alternatives Considered**:
- Alpine Linux
  - ✅ Smaller image size
  - ❌ musl libc compatibility issues with RocksDB
  - ❌ Potential performance degradation
- Ubuntu
  - ✅ Very familiar
  - ❌ Larger base image
  - ❌ More attack surface
- Distroless
  - ✅ Minimal attack surface
  - ❌ Harder to debug
  - ❌ No shell for troubleshooting

**Implementation Notes**:
```dockerfile
FROM rust:1.75-slim as builder
# Build kalamdb-server binary

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/kalamdb-server /usr/local/bin/
COPY --from=builder /app/target/release/kalam-cli /usr/local/bin/
```

### Decision 6: Environment Variable Override Mechanism

**Decision**: Use envsubst or Rust env::var() to override config.toml values at runtime

**Rationale**:
- Rust std::env::var() is idiomatic and no external dependencies
- Config precedence: env vars > config.toml > defaults
- Docker-compose .env file support built-in
- Clear naming convention: `KALAMDB_PORT`, `KALAMDB_DATA_DIR`, etc.

**Alternatives Considered**:
- envsubst template substitution
  - ✅ Works at container start
  - ❌ Requires shell script wrapper
  - ❌ Less type-safe
- Config file generation from env vars
  - ❌ More complex
  - ❌ Requires file writes
- CLI flags passed to binary
  - ✅ Explicit
  - ❌ Verbose docker-compose files
  - ❌ Doesn't support nested config

**Implementation Notes**:
- Modify config loading in kalamdb-server to check env vars first
- Naming: `KALAMDB_` prefix + uppercase nested keys with `_` separator
- Example: `KALAMDB_SERVER_PORT`, `KALAMDB_LOG_LEVEL`, `KALAMDB_DATA_DIR`
- Document all overridable values in docker/README.md

### Decision 7: Volume Mount Strategy

**Decision**: Single named volume for `/data` directory containing RocksDB files

**Rationale**:
- Named volumes are managed by Docker (better than bind mounts for production)
- Single mount point simplifies configuration
- Survives container recreation
- Easy to backup/restore

**Alternatives Considered**:
- Bind mount to host directory
  - ✅ Direct file access from host
  - ❌ Permissions issues across OSes
  - ❌ Less portable
- Multiple volumes (data, logs, config)
  - ❌ Over-complicated for single-server setup
  - ✅ Better separation of concerns
  - **Verdict**: Single volume sufficient for now

**Implementation Notes**:
```yaml
volumes:
  kalamdb_data:

services:
  kalamdb:
    volumes:
      - kalamdb_data:/data
```

---

## Story 2: WASM Compilation

### Decision 8: WASM Build Tool

**Decision**: Use wasm-pack for building and packaging kalam-link

**Rationale**:
- Industry standard for Rust → WASM compilation
- Generates TypeScript type definitions automatically
- Handles wasm-bindgen integration
- Produces npm-compatible package structure
- Well-documented, actively maintained

**Alternatives Considered**:
- cargo build --target wasm32-unknown-unknown
  - ✅ More direct control
  - ❌ Requires manual wasm-bindgen invocation
  - ❌ No TypeScript bindings generation
  - ❌ More manual packaging
- wasm-tools
  - ✅ Lower-level control
  - ❌ More complex workflow
  - ❌ No JS/TS binding generation

**Implementation Notes**:
- Add wasm-pack to dev dependencies
- Build command: `wasm-pack build --target web --out-dir pkg`
- Output: pkg/ directory with .wasm file + JS/TS bindings
- Can publish to npm or use locally

### Decision 9: WASM Module Initialization API

**Decision**: Require `KalamClient::new(url: String, api_key: String)` constructor

**Rationale**:
- Explicit initialization prevents misconfiguration
- No implicit defaults (security best practice)
- Clear error messages if missing
- Aligns with spec requirement (FR-021, FR-022, FR-023)
- TypeScript types enforce required parameters

**Alternatives Considered**:
- Builder pattern with optional chaining
  - ✅ More flexible
  - ❌ Allows forgetting required params
  - ❌ More complex API
- Global configuration object
  - ❌ Not idiomatic for library
  - ❌ Shared state across instances
- Environment variables
  - ❌ Not available in browser context
  - ❌ Insecure (API key in env)

**Implementation Notes**:
```rust
#[wasm_bindgen]
pub struct KalamClient {
    url: String,
    api_key: String,
    // ... websocket connection
}

#[wasm_bindgen]
impl KalamClient {
    #[wasm_bindgen(constructor)]
    pub fn new(url: String, api_key: String) -> Result<KalamClient, JsValue> {
        if url.is_empty() || api_key.is_empty() {
            return Err(JsValue::from_str("URL and API key are required"));
        }
        // Initialize WebSocket connection
        Ok(KalamClient { url, api_key })
    }
}
```

### Decision 10: WebSocket Protocol for WASM

**Decision**: Reuse existing KalamDB WebSocket subscription protocol, send X-API-KEY in initial handshake

**Rationale**:
- Already implemented and tested
- Supports insert/update/delete events
- Efficient binary protocol
- API key passed in WebSocket headers or first message

**Alternatives Considered**:
- HTTP polling
  - ❌ Higher latency
  - ❌ More server load
  - ❌ Doesn't support push notifications
- Server-Sent Events (SSE)
  - ✅ Simpler than WebSocket
  - ❌ Unidirectional only
  - ❌ No binary support
- gRPC-Web
  - ❌ More complex setup
  - ❌ Not needed for simple use case

**Implementation Notes**:
- WebSocket URL: `ws://host:port/subscribe`
- Send API key in Sec-WebSocket-Protocol header or first frame
- Server validates API key, opens user-scoped subscription
- Events: `{"type": "insert|update|delete", "table": "...", "id": ..., "data": {...}}`

---

## Story 3: React TypeScript Example

### Decision 11: React State Management

**Decision**: Use React hooks (useState, useEffect, useReducer) without external state library

**Rationale**:
- Keeps example simple and dependency-free
- useState sufficient for TODO list state
- useEffect handles WebSocket lifecycle
- Easy for beginners to understand
- Aligns with "under 500 LOC" constraint (SC-008)

**Alternatives Considered**:
- Redux
  - ❌ Overkill for simple example
  - ❌ Adds boilerplate
  - ✅ Better for large apps (not applicable)
- Zustand
  - ✅ Simpler than Redux
  - ❌ Still external dependency
  - ❌ Unnecessary for this scope
- MobX
  - ❌ Different paradigm (may confuse)
  - ❌ External dependency

**Implementation Notes**:
```typescript
const [todos, setTodos] = useState<Todo[]>([]);
const [connected, setConnected] = useState(false);
const [client, setClient] = useState<KalamClient | null>(null);
```

### Decision 12: localStorage Sync Strategy

**Decision**: Store full TODO list + highest ID, sync on connect by subscribing from last ID

**Rationale**:
- localStorage provides instant initial render
- Subscription from last ID minimizes data transfer
- Server is source of truth (handles conflicts)
- Simple implementation (JSON.stringify/parse)

**Alternatives Considered**:
- IndexedDB
  - ✅ Better for large datasets
  - ❌ More complex API
  - ❌ Out of scope (spec: localStorage sufficient)
- No local cache
  - ❌ Slow initial load
  - ❌ No offline display
- Full sync every connect
  - ❌ Inefficient for large lists
  - ❌ More bandwidth

**Implementation Notes**:
```typescript
// On load
const cached = localStorage.getItem('todos');
const lastId = localStorage.getItem('lastTodoId');
if (cached) {
  setTodos(JSON.parse(cached));
  // Subscribe from lastId onwards
  client.subscribe('todos', { fromId: lastId });
}

// On updates
localStorage.setItem('todos', JSON.stringify(todos));
localStorage.setItem('lastTodoId', Math.max(...todos.map(t => t.id)));
```

### Decision 13: React Testing Approach

**Decision**: Use React Testing Library with user-centric tests

**Rationale**:
- Industry standard for React testing
- Encourages testing user behavior over implementation
- Good documentation and community support
- Integrates with Jest

**Alternatives Considered**:
- Enzyme
  - ❌ Less maintained
  - ❌ Implementation-focused
- Cypress
  - ✅ Great for E2E
  - ❌ Overkill for unit tests
  - ❌ Requires separate server
- Plain Jest
  - ❌ Requires manual DOM setup
  - ❌ Less React-specific utilities

**Implementation Notes**:
- Test connection status display
- Test TODO add/delete operations
- Test localStorage persistence
- Mock WASM client for isolation

### Decision 14: Build Tool for React Example

**Decision**: Vite for development and bundling

**Rationale**:
- Fast dev server with HMR
- Native ES modules (no bundling in dev)
- Simple configuration
- Good TypeScript support
- Handles WASM imports well

**Alternatives Considered**:
- Create React App (CRA)
  - ❌ Deprecated/less maintained
  - ❌ Slower than Vite
  - ✅ More batteries-included
- Next.js
  - ❌ Overkill (SSR not needed)
  - ❌ More complex
- Webpack manual setup
  - ❌ More configuration
  - ❌ Slower dev experience

**Implementation Notes**:
```json
{
  "scripts": {
    "dev": "vite",
    "build": "vite build",
    "test": "vitest"
  }
}
```

---

## Summary of Key Decisions

| Decision | Choice | Story |
|----------|--------|-------|
| API Key Storage & Generation | User table `apikey` field (auto-generated UUID) | P0 |
| User Role Field | String field with predefined roles | P0 |
| Soft Delete | Boolean `deleted` field | P0 |
| Auth Middleware | Actix-Web X-API-KEY middleware | P0 |
| Docker Base Image | rust:1.75-slim + debian:bookworm-slim | P1 |
| Config Override | Rust env::var() with KALAMDB_ prefix | P1 |
| Volume Strategy | Single named volume /data | P1 |
| WASM Build Tool | wasm-pack | P2 |
| WASM Init API | Required url + api_key constructor | P2 |
| WebSocket Protocol | Existing KalamDB subscription | P2 |
| React State | useState/useEffect hooks | P3 |
| localStorage Sync | Full list + last ID, subscribe from ID | P3 |
| React Testing | React Testing Library | P3 |
| Build Tool | Vite | P3 |

---

## Next Steps

All technical unknowns from Phase 0 have been resolved. Ready to proceed to Phase 1:
- **data-model.md**: Document User table schema changes (apikey, role fields), TODO table schema
- **contracts/**: Define API contract for X-API-KEY header, WASM client interface
- **quickstart.md**: Setup instructions for Docker, WASM build, React example
