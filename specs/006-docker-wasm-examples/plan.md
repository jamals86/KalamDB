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
  - WASM: 1 compiled module + TypeScript bindings
  - Frontend: 1 example app (~500 LOC), 1 setup script

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

cli/
└── kalam-link/
    ├── src/
    │   ├── wasm.rs              # NEW: WASM-specific bindings
    │   └── client.rs            # MODIFIED: Add API key + URL params
    ├── Cargo.toml               # MODIFIED: Add wasm-bindgen deps
    └── pkg/                     # NEW: wasm-pack output directory

examples/
└── simple-typescript/
    ├── src/
    │   ├── components/          # NEW: React components
    │   ├── services/            # NEW: WASM client wrapper
    │   └── index.tsx            # NEW: App entry point
    ├── setup.sh                 # NEW: Table setup script
    ├── todo-app.sql             # NEW: SQL schema definitions
    ├── package.json             # NEW: Dependencies + scripts
    ├── tsconfig.json            # NEW: TypeScript config
    └── README.md                # NEW: Setup instructions

docker/
├── backend/
│   ├── Dockerfile              # NEW: Docker image definition
│   ├── docker-compose.yml      # NEW: Compose configuration with volumes
│   └── build-backend.sh        # NEW: Build script for backend + CLI image
└── README.md                    # NEW: Docker deployment guide
```

**Structure Decision**: Multi-component extension of existing KalamDB structure. Modifications concentrated in backend/crates (auth + soft delete), new WASM build in cli/kalam-link, new example in examples/, and new Docker configs at backend root.

````
