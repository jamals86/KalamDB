# Implementation Plan: Docker Container, WASM Compilation, and TypeScript Examples

**Branch**: `006-docker-wasm-examples` | **Date**: 2025-10-25 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/006-docker-wasm-examples/spec.md`

**Note**: This template is filled in by the `/speckit.plan` command. See `.specify/templates/commands/plan.md` for the execution workflow.

## Summary

This feature adds API key authentication with soft delete for user tables, Docker containerization with environment variable configuration, WASM compilation of kalam-link for browser/Node.js use, and a React TODO app example demonstrating real-time subscriptions with localStorage caching.

**Technical Approach**:
- Story 0 (P0): Extend KalamDB backend with API key auth and soft delete
- Story 1 (P1): Create Docker container with config override support
- Story 2 (P2): Compile kalam-link to WASM using wasm-pack
- Story 3 (P3): Build React app using WASM client with real-time sync

## Technical Context

**Language/Version**: Rust 1.75+ (backend), TypeScript/JavaScript ES2020+ (frontend), Bash (scripts)
**Primary Dependencies**: 
  - Backend: RocksDB 0.21, Actix-Web 4.4, existing KalamDB crates
  - Docker: Docker Engine 20+, Docker Compose 2.0+
  - WASM: wasm-pack 0.12+, wasm-bindgen
  - Frontend: React 18+, TypeScript 5+, React Testing Library
**Storage**: RocksDB (existing), browser localStorage (new)
**Testing**: cargo test (Rust), Jest/React Testing Library (TypeScript), integration tests (bash)
**Target Platform**: 
  - Backend: Linux/macOS server (Docker container)
  - WASM: wasm32-unknown-unknown target
  - Frontend: Modern browsers (Chrome, Firefox, Safari latest)
**Project Type**: Multi-component (backend modification + Docker + WASM library + web example)
**Performance Goals**: 
  - API key auth: <10ms overhead
  - Soft delete queries: same perf as hard delete
  - WASM module load: <500ms
  - React app initial render: <50ms from localStorage
  - WebSocket sync latency: <100ms
**Constraints**: 
  - API key stored in user table (no separate auth service)
  - Soft delete transparent to existing queries
  - WASM requires server URL + API key (no defaults)
  - React app read-only when WebSocket disconnected
**Scale/Scope**: 
  - 4 independent user stories
  - Backend: 2 new fields (apikey, deleted), 1 new middleware
  - Docker: 1 Dockerfile, 1 docker-compose.yml
  - WASM: Multi-language SDK architecture in link/sdks/
  - TypeScript SDK: Complete package with build system, tests, docs
  - Frontend: 1 example app (~500 LOC), 1 setup script

**SDK Architecture Philosophy**:
  - Each language SDK in `link/sdks/{language}/` is a complete, self-contained, npm-publishable package
  - Every SDK includes: build system, package config, tests, documentation, .gitignore
  - TypeScript SDK compiles from `link/` (Rust source) to `link/sdks/typescript/` (output)
  - Future SDKs (Python, Go, etc.) follow same pattern for consistency
  - **Examples MUST use SDKs as local dependencies** (e.g., `"@kalamdb/client": "file:../../link/sdks/typescript"`)
  - **Examples MUST NOT implement their own clients** - all functionality comes from SDK
  - If examples need additional functions, those should be added to the SDK for all users
  - SDKs are designed to be published to npm/PyPI/etc. without modification

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

**Status**: ✅ PASS (Re-validated after Phase 1 design)

This feature aligns with KalamDB principles:
- ✅ Extends existing libraries (kalamdb-core, kalamdb-api, kalam-link)
- ✅ CLI tools remain functional (kalam-cli works on localhost)
- ✅ Test-first approach required (FR-009: update all tests)
- ✅ No new projects/services (modifies existing backend, adds examples)
- ✅ Clear integration points (API key header, WASM client interface)
- ✅ Observability maintained (text-based logs, structured errors)
- ✅ Design artifacts complete (data-model.md, contracts/, quickstart.md)

**No violations requiring justification.**

## Project Structure

### Documentation (this feature)

```text
specs/[###-feature]/
├── plan.md              # This file (/speckit.plan command output)
├── research.md          # Phase 0 output (/speckit.plan command)
├── data-model.md        # Phase 1 output (/speckit.plan command)
├── quickstart.md        # Phase 1 output (/speckit.plan command)
├── contracts/           # Phase 1 output (/speckit.plan command)
└── tasks.md             # Phase 2 output (/speckit.tasks command - NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
backend/
├── crates/
│   ├── kalamdb-core/
│   │   └── src/
│   │       ├── auth/          # NEW: API key authentication
│   │       └── storage/        # MODIFIED: Soft delete support
│   ├── kalamdb-api/
│   │   └── src/
│   │       └── middleware/     # NEW: X-API-KEY header auth
│   └── kalamdb-server/
│       └── src/
│           ├── config.rs       # MODIFIED: Env var override support
│           └── commands/       # NEW: CLI commands (create-user)
└── tests/
    ├── test_api_key_auth.rs     # NEW: API key authentication tests
    └── test_soft_delete.rs      # NEW: Soft delete behavior tests

link/                            # Rust library for KalamDB client
├── src/
│   ├── wasm.rs                  # NEW: WASM-specific bindings
│   ├── client.rs                # MODIFIED: Add API key + URL params
│   └── lib.rs                   # Core library interface
├── Cargo.toml                   # Dependencies for both CLI and WASM
├── sdks/                        # NEW: Multi-language SDK directory
│   └── typescript/              # TypeScript/JavaScript SDK
│       ├── build.sh             # Build script (Rust → WASM)
│       ├── package.json         # npm package config
│       ├── README.md            # API documentation
│       ├── .gitignore           # Excludes generated WASM files
│       ├── src/                 # TypeScript wrapper code (if needed)
│       ├── tests/               # SDK test suite (Node.js)
│       │   └── basic.test.mjs   # Core functionality tests
│       └── [generated files]    # kalam_link_bg.wasm, kalam_link.js, kalam_link.d.ts
└── tests/                       # Rust unit tests

cli/
└── kalam-cli/                   # CLI tool (depends on link/ crate)
    ├── src/
    │   └── main.rs              # Uses kalam-link as dependency
    └── Cargo.toml               # Depends on link crate

examples/
└── simple-typescript/
    ├── src/
    │   ├── components/          # NEW: React components
    │   ├── services/            # NEW: localStorage utilities (NOT client wrapper)
    │   ├── hooks/               # NEW: React hooks (useTodos)
    │   └── index.tsx            # NEW: App entry point
    ├── setup.sh                 # NEW: Table setup script
    ├── todo-app.sql             # NEW: SQL schema definitions
    ├── package.json             # NEW: Dependencies + local SDK reference
    ├── tsconfig.json            # NEW: TypeScript config
    └── README.md                # NEW: Setup instructions
    
    **NOTE**: This example imports KalamDB client from ../../link/sdks/typescript/
              It does NOT implement its own client wrapper.

docker/
├── backend/
│   ├── Dockerfile              # NEW: Docker image definition
│   ├── docker-compose.yml      # NEW: Compose configuration with volumes
│   └── build-backend.sh        # NEW: Build script for backend + CLI image
└── README.md                    # NEW: Docker deployment guide
```

**Structure Decision**: Multi-component extension of existing KalamDB structure. Modifications concentrated in backend/crates (auth + soft delete), new WASM build in link/kalam-link (moved from cli/), new example in examples/, and new Docker configs at backend root.

---

### SDK Integration Principles

**Philosophy**: SDKs are first-class publishable packages, examples are SDK consumers

**Architecture**:
1. **SDK Location**: `link/sdks/{language}/` (e.g., `link/sdks/typescript/`)
2. **SDK Structure**: Complete, self-contained package ready for npm/PyPI/etc.
   - package.json / setup.py / Cargo.toml (depending on language)
   - Build scripts (e.g., build.sh for WASM compilation)
   - Tests (e.g., tests/basic.test.mjs)
   - Documentation (README.md with API reference)
   - .gitignore (excludes build artifacts if needed)
3. **Example Structure**: `examples/{example-name}/`
4. **Example Dependencies**: Import SDK as local dependency
   - TypeScript: `"@kalamdb/client": "file:../../link/sdks/typescript"`
   - Python: `pip install -e ../../link/sdks/python`
5. **No Duplication**: Examples MUST NOT implement their own clients
6. **SDK Extension**: If examples need functionality, add it to the SDK for all users

**Benefits**:
- ✅ SDKs are production-ready, tested, and documented
- ✅ Examples validate SDK usability
- ✅ No code duplication between examples
- ✅ SDK improvements benefit all examples immediately
- ✅ Easy to publish SDKs to package registries

**Example Workflow**:
```typescript
// ❌ WRONG: examples/simple-typescript/src/services/kalamClient.ts
export class KalamClient { ... } // Don't implement your own!

// ✅ CORRECT: examples/simple-typescript/src/App.tsx
import { KalamClient } from '@kalamdb/client'; // Use SDK

const client = new KalamClient(config);
```

---

### Issue 1: UPDATE/DELETE Row Count Incorrect (Critical)

**Problem**: UPDATE and DELETE operations return "Updated 0 row(s)" / "Deleted 0 row(s)" even when successful.

**Root Cause**: Row count tracking in `backend/crates/kalamdb-core/src/sql/executor.rs` not properly incrementing counters.

**Fix Plan (Phase 2.5)**:
1. Research PostgreSQL behavior for row counts (T011A-T011B)
2. Fix UPDATE handler counter (T011C)
3. Fix DELETE handler counter for soft deletes (T011D)
4. Add integration tests (T011E-T011G)
5. Document LIKE pattern limitation (T011H)

**Impact**: User confusion, non-standard SQL behavior, testing difficulties

---

### Issue 2: Project Structure - kalam-link Location

**Problem**: `cli/kalam-link/` path implies CLI-only use, but it's actually dual-purpose (CLI dependency + standalone WASM library).

**Fix Plan (Phase 2.5)**:
- Move `cli/kalam-link/` → `/link/kalam-link/` (T011I-T011J)
- Update all references in CLI, docs, and build scripts (T011K-T011R)

**Benefits**: Clearer separation, better organization, explicit multi-purpose indication

---

````
