# KalamDB Development Guidelines

Auto-generated from all feature plans. Last updated: 2025-10-27

## ⚠️ CRITICAL: System Table Models Architecture

**SINGLE SOURCE OF TRUTH**: All system table models are defined in `kalamdb-commons/src/models/system.rs`

**DO NOT create duplicate model definitions**. Always import from:
```rust
use kalamdb_commons::system::{User, Job, LiveQuery, Namespace, SystemTable, InformationSchemaTable, UserTableCounter};
```

**Consolidation Status** (as of 2025-10-27):
- ✅ Models moved to `kalamdb-commons/src/models/system.rs`
- 🚧 IN PROGRESS: Removing duplicates from `jobs_provider.rs`, `live_queries_provider.rs`, `kalamdb-sql/src/models.rs`
- See `docs/architecture/SYSTEM_MODEL_CONSOLIDATION.md` for full migration plan

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
- 2025-10-26: SDK architecture clarified - examples MUST use link/sdks/ as dependencies (006-docker-wasm-examples)
- 006-docker-wasm-examples: Added TypeScript SDK, React example, Docker deployment, API key auth
- 004-system-improvements-and: Added Rust 1.75+ (stable toolchain, edition 2021)
- 2025-10-15: Tasks.md synchronized with plan.md (229 tasks, 100% aligned)

<!-- MANUAL ADDITIONS START -->
<!-- MANUAL ADDITIONS END -->
