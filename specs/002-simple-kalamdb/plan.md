# Implementation Plan: [FEATURE]

**Branch**: `[###-feature-name]` | **Date**: [DATE] | **Spec**: [link]
**Input**: Feature specification from `/specs/[###-feature-name]/spec.md`

**Note**: This template is filled in by the `/speckit.plan` command. See `.specify/templates/commands/plan.md` for the execution workflow.

## Summary

[Extract from feature spec: primary requirement + technical approach from research]

## Technical Context

<!--
  ACTION REQUIRED: Replace the content in this section with the technical details
  for the project. The structure here is presented in advisory capacity to guide
  the iteration process.
-->

**Language/Version**: Rust 1.75+ (stable toolchain)
**Primary Dependencies**: RocksDB (writes), Apache Arrow (zero-copy data), Apache Parquet (storage), DataFusion (SQL queries), tokio (async runtime), serde (serialization)
**Storage**: RocksDB for write path (<1ms), Parquet for analytics-ready compressed storage
**Testing**: cargo test, criterion.rs (benchmarks), property-based tests for data transformations
**Target Platform**: Linux server, embeddable library
**Project Type**: single (database server with library interface)
**Performance Goals**: Writes <1ms (p99 RocksDB path), SQL query performance via DataFusion
**Constraints**: Zero-copy efficiency via Arrow IPC, tenant isolation enforcement, JWT authentication required
**Scale/Scope**: Real-time chat/AI conversation storage, user-partitioned data, streaming + durable persistence

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

[Gates determined based on constitution file]

**Documentation Requirements (Principle VIII - Self-Documenting Code)**:
- [ ] Module-level rustdoc comments planned for all new modules
- [ ] Public API documentation strategy defined (structs, enums, traits, functions)
- [ ] Inline comment strategy for complex algorithms and architectural patterns identified
- [ ] Architecture Decision Records (ADRs) planned for key design choices
- [ ] Code examples planned for non-trivial public APIs

## Project Structure

### Documentation (this feature)

```
specs/[###-feature]/
├── plan.md              # This file (/speckit.plan command output)
├── research.md          # Phase 0 output (/speckit.plan command)
├── data-model.md        # Phase 1 output (/speckit.plan command)
├── quickstart.md        # Phase 1 output (/speckit.plan command)
├── contracts/           # Phase 1 output (/speckit.plan command)
└── tasks.md             # Phase 2 output (/speckit.tasks command - NOT created by /speckit.plan)
```

### Source Code (repository root)
<!--
  ACTION REQUIRED: Replace the placeholder tree below with the concrete layout
  for this feature. Delete unused options and expand the chosen structure with
  real paths (e.g., apps/admin, packages/something). The delivered plan must
  not include Option labels.
-->

```
# [REMOVE IF UNUSED] Option 1: Single project (DEFAULT)
src/
├── models/
├── services/
├── cli/
└── lib/

tests/
├── contract/
├── integration/
└── unit/

# [REMOVE IF UNUSED] Option 2: Web application (when "frontend" + "backend" detected)
backend/
├── src/
│   ├── models/
│   ├── services/
│   └── api/
└── tests/

frontend/
├── src/
│   ├── components/
│   ├── pages/
│   └── services/
└── tests/

# [REMOVE IF UNUSED] Option 3: Mobile + API (when "iOS/Android" detected)
api/
└── [same as backend above]

ios/ or android/
└── [platform-specific structure: feature modules, UI flows, platform tests]
```

**Structure Decision**: [Document the selected structure and reference the real
directories captured above]

## Complexity Tracking

*Fill ONLY if Constitution Check has violations that must be justified*

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| [e.g., 4th project] | [current need] | [why 3 projects insufficient] |
| [e.g., Repository pattern] | [specific problem] | [why direct DB access insufficient] |
