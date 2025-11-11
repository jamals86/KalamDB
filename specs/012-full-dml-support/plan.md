# Implementation Plan: Full DML Support

**Branch**: `012-full-dml-support` | **Date**: 2025-11-11 | **Spec**: [/Users/jamal/git/KalamDB/specs/012-full-dml-support/spec.md](spec.md)
**Input**: Feature specification from `/specs/012-full-dml-support/spec.md`

**Note**: This document is produced via the `/speckit.plan` workflow.

## Summary

Deliver append-only UPDATE/DELETE support across fast and persisted storage, add manifest-driven query acceleration with Bloom filters and caching, formalize AS USER impersonation, centralize system column management, and introduce typed job executor parameters and unified configuration access within KalamDB's Rust backend.

## Technical Context

**Language/Version**: Rust 1.90 (edition 2021)  
**Primary Dependencies**: DataFusion 40, Apache Arrow 52, Apache Parquet 52, RocksDB 0.24, Actix-Web 4.4, serde 1.0, DashMap 5  
**Storage**: RocksDB fast path with Parquet batch files on filesystem/S3 for flushed segments  
**Testing**: `cargo test`, smoke tests under `backend/tests`, targeted performance harnesses (FR-102–FR-107)  
**Target Platform**: Linux/macOS servers hosting the KalamDB engine (Actix runtime)  
**Project Type**: Distributed database engine (backend services + CLI)  
**Performance Goals**: Maintain ≤2× baseline query latency with 10+ versions, manifest pruning decisions <5 ms, Bloom filter lookups <1 ms per file, UPDATE (<10 ms) and DELETE (<50 ms) on persisted records  
**Constraints**: Append-only mutation model, nanosecond `_updated` monotonicity, manifest/cache consistency, AS USER restricted to service/admin roles, configuration single source via AppContext  
**Scale/Scope**: Tables with 100+ Parquet batches, millions of record versions, manifest cache up to 50 000 entries, concurrent job execution with typed params

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- Gate 1 – Constitution Availability: `.specify/memory/constitution.md` contains placeholder sections with no ratified principles. **Status**: BLOCKED (NEEDS CLARIFICATION). **Action**: Request maintainers to supply governing principles or confirm that default KalamDB guidelines supersede constitution. Proceeding with feature planning under existing AGENTS.md guidance pending clarification.
- Post-Phase 1 Review: Constitution still unpopulated after design artifacts; mitigation remains escalation plus adherence to AGENTS.md standards until guidance is formalized.

## Project Structure

### Documentation (this feature)

```text
specs/012-full-dml-support/
├── plan.md              # Implementation plan (this file)
├── research.md          # Phase 0 research outcomes
├── data-model.md        # Phase 1 entity design
├── quickstart.md        # Phase 1 onboarding steps
├── contracts/           # Phase 1 API/interaction contracts
└── tasks.md             # Phase 2 task breakdown (created by /speckit.tasks)
```

### Source Code (repository root)

```text
backend/
├── crates/
│   ├── kalamdb-core/            # AppContext, schema registry, tables, jobs, DML handlers
│   ├── kalamdb-commons/         # Shared models (system tables, manifests, IDs)
│   ├── kalamdb-store/           # RocksDB stores & storage backends
│   ├── kalamdb-auth/            # Auth & impersonation policies
│   └── kalamdb-sql/             # SQL parsing / AST extensions
├── src/
│   ├── lifecycle.rs             # Bootstrap, flush scheduling, AppContext init
│   ├── config.rs                # Configuration DTO exposed via AppContext
│   └── routes.rs                # HTTP + WebSocket API entrypoints
└── tests/                       # Smoke, integration, performance harnesses

cli/
└── src/                         # CLI client invoking SQL endpoints

data/
├── rocksdb/                     # Local RocksDB instance (fast storage, CFs)
└── storage/                     # Parquet batches, manifests (S3/local abstractions)
```

**Structure Decision**: Feature impacts the Rust backend core (`kalamdb-core`, `kalamdb-commons`, `kalamdb-store`, `kalamdb-sql`) with supporting updates to backend configuration and job execution; no frontend/mobile assets involved.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| Proceeding without explicit constitution principles | Constitution file is a placeholder; progress needed for planning deliverables | Waiting would block scheduling; rely on AGENTS.md guidelines until constitution is clarified |

**Constitution Authority Resolution**: KalamDB uses AGENTS.md as the authoritative governance document. The SpecKit constitution template (`.specify/memory/constitution.md`) is not populated because project-specific guidelines in AGENTS.md supersede generic templates. All architecture decisions, coding principles, and quality gates are defined in AGENTS.md (see sections: Core Coding Principles, Module Organization, Dependency Management, Authentication Patterns, Build & Check Cadence).
