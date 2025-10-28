# KalamDB Development Guidelines

Auto-generated from all feature plans. Last updated: 2025-10-27

## ⚠️ CRITICAL: System Table Models Architecture

**SINGLE SOURCE OF TRUTH**: All system table models are defined in `kalamdb-commons/src/models/system.rs`

**Authentication Constants**: System user constants defined in `kalamdb-commons/src/constants.rs`
```rust
// Use AuthConstants::DEFAULT_SYSTEM_USERNAME instead of hardcoded "root"
use kalamdb_commons::constants::AuthConstants;
let username = AuthConstants::DEFAULT_SYSTEM_USERNAME;  // "root"
let user_id = AuthConstants::DEFAULT_SYSTEM_USER_ID;    // "sys_root"
```

**DO NOT create duplicate model definitions**. Always import from:
```rust
use kalamdb_commons::system::{User, Job, LiveQuery, Namespace, SystemTable, InformationSchemaTable, UserTableCounter};
```

**Consolidation Status** (as of 2025-10-28):
- ✅ Models defined in `kalamdb-commons/src/models/system.rs` (COMPLETE)
- ✅ All duplicates removed from `kalamdb-sql/src/models.rs` (COMPLETE)
- ✅ Fixed `users_provider.rs` to use consolidated models (COMPLETE)
- ✅ Fixed `live_queries_provider.rs` to use consolidated LiveQuery model (COMPLETE)
- ✅ Storage abstraction (Sub-Phase 0.5.1) complete with StorageBackend trait
- ✅ Domain models (Sub-Phase 0.5.2) complete with strongly-typed entities
- ✅ **Phase 0 Complete** (October 28, 2025) - All system table providers verified to use kalamdb_commons::system models
- ✅ **Phase 0.5 Complete** - Storage refactoring complete, ready for authentication implementation
- ✅ Build verification: cargo build succeeds with 0 errors
- See `docs/architecture/SYSTEM_MODEL_CONSOLIDATION_STATUS.md` for details

## Active Technologies
- (001-build-a-rust)
- Rust 1.75+ (stable toolchain, edition 2021) (002-simple-kalamdb)
- RocksDB 0.21, Apache Arrow 50.0, Apache Parquet 50.0, DataFusion 35.0, Actix-Web 4.4 (002-simple-kalamdb)
- RocksDB for write path (<1ms), Parquet for flushed storage (compressed columnar format) (004-system-improvements-and)
- Rust 1.75+ (backend), TypeScript/JavaScript ES2020+ (frontend), Bash (scripts) (006-docker-wasm-examples)
- RocksDB (existing), browser localStorage (new) (006-docker-wasm-examples)

## Project Structure
```
backend/                         # Server binary and core crates
├── src/main.rs                  # kalamdb-server entry point
└── crates/                      # Supporting libraries
    ├── kalamdb-core/            # Core library (embeddable)
    ├── kalamdb-api/             # REST API and WebSocket
    ├── kalamdb-sql/             # SQL execution and DataFusion
    ├── kalamdb-store/           # RocksDB storage layer
    ├── kalamdb-live/            # Real-time subscriptions
    └── kalamdb-commons/         # Shared utilities

cli/                             # CLI tool binary
├── src/main.rs                  # kalam-cli entry point
└── tests/                       # CLI integration tests

link/                            # WASM-compiled client library
├── src/lib.rs                   # Rust library
├── src/wasm.rs                  # WASM bindings
└── sdks/                        # Multi-language SDKs
    └── typescript/              # TypeScript/JavaScript SDK
        ├── package.json         # npm package (@kalamdb/client)
        ├── build.sh             # Rust→WASM compilation
        ├── tests/               # SDK tests (14 passing)
        └── README.md            # API documentation

examples/                        # Example applications
└── simple-typescript/           # React TODO app
    ├── package.json             # Uses link/sdks/typescript/ as dependency
    └── src/                     # Imports from '@kalamdb/client'

specs/                           # Feature specifications
└── 006-docker-wasm-examples/    # Current feature
    ├── plan.md                  # Implementation plan
    ├── tasks.md                 # Task breakdown
    └── SDK_INTEGRATION.md       # SDK architecture guide
```

## SDK Architecture Principles (006-docker-wasm-examples)

**CRITICAL**: Examples MUST use SDKs as dependencies, NOT implement their own clients

- SDKs at `link/sdks/{language}/` are complete, publishable npm/PyPI packages
- Examples import SDKs as local dependencies: `"@kalamdb/client": "file:../../link/sdks/typescript"`
- Examples MUST NOT create mock client implementations (e.g., `kalamClient.ts`)
- If examples need functionality, add it to the SDK for all users
- SDKs include: build system, tests, docs, package config, .gitignore

**Example Usage**:
```typescript
// ✅ CORRECT: examples/simple-typescript/src/App.tsx
import { KalamClient } from '@kalamdb/client'; // From SDK

// ❌ WRONG: examples/simple-typescript/src/services/kalamClient.ts
export class KalamClient { ... } // Don't implement your own!
```

## Commands
# Build: cargo build
# Test: cargo test
# Run: cargo run --bin kalamdb-server

## Code Style
Rust 2021 edition: Follow standard conventions, use type-safe wrappers (NamespaceId, TableName, UserId, TableType enum)

## Recent Changes
- 2025-10-28: **Phase 0 COMPLETE** (007-user-auth) - System model consolidation verified, live_queries_provider migrated, cargo build passes
- 2025-10-27: **Phase 0.5 COMPLETE** - System model consolidation finished, storage abstraction implemented
- 2025-10-27: All system models consolidated to kalamdb-commons (User, Job, Namespace, LiveQuery, SystemTable)
- 2025-10-26: SDK architecture clarified - examples MUST use link/sdks/ as dependencies (006-docker-wasm-examples)
- 006-docker-wasm-examples: Added TypeScript SDK, React example, Docker deployment, API key auth
- 004-system-improvements-and: Added Rust 1.75+ (stable toolchain, edition 2021)
- 2025-10-15: Tasks.md synchronized with plan.md (229 tasks, 100% aligned)

<!-- MANUAL ADDITIONS START -->
<!-- MANUAL ADDITIONS END -->
