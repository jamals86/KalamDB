# Implementation Plan: Live Queries over WebSocket Connections

**Branch**: `014-live-queries-websocket` | **Date**: November 24, 2025 | **Spec**: `specs/014-live-queries-websocket/spec.md`
**Input**: Feature specification from `specs/014-live-queries-websocket/spec.md`

**Note**: This implementation plan is derived from the approved feature specification and will guide design and development work for live queries over multiplexed WebSocket connections and the Kalam-link client SDK.

## Summary

This feature enables each authenticated user to maintain multiple WebSocket connections, each hosting multiple live query subscriptions, with one row per subscription in `system.live_queries` keyed by `(connection_id, subscription_id)`. Users can subscribe and unsubscribe individual queries on a shared connection; administrators (or auth expiry) can kill a connection via SQL commands (no REST) and thereby terminate all its subscriptions within a 5-second SLA; and the Kalam-link shared library (not just the TypeScript SDK) will own connection lifecycle, authentication, subscriptions, auto-reconnect, and SeqId-based resume semantics with O(1) handler lookup while the TypeScript SDK exposes thin wrappers to those shared capabilities.

## Technical Context

**Language/Version**: Rust 1.90 (edition 2021) for backend; TypeScript (ES2020+) for Kalam-link SDK  
**Primary Dependencies**: Actix-Web 4 for HTTP/WebSocket handling; existing KalamDB live query infrastructure in `kalamdb-core`; Kalam-link TypeScript client stack; RocksDB and Parquet via existing storage abstractions  
**Storage**: Existing RocksDB-backed write path and Parquet segment storage; `system.live_queries` system table backed by current metadata mechanisms  
**Testing**: `cargo test` (unit + integration) for backend crates, existing smoke tests in `cli` (`cargo test --test smoke`), and TypeScript tests for the link SDK (e.g., Jest/ts-node as already configured)  
**Target Platform**: Linux/macOS server environments for backend; browser and Node.js environments for Kalam-link clients  
**Project Type**: Multi-crate Rust backend plus separate TypeScript SDK package  
**Performance Goals**: Support dozens of concurrent subscriptions per connection per client without materially increasing connection count; keep admin kill-connection operations reflected in `system.live_queries` within a few seconds under normal load  
**Constraints**: Must preserve existing KalamDB performance and memory characteristics; avoid introducing per-subscription heavy state on the server; adhere to existing authentication and authorization patterns; maintain backwards compatibility for existing clients where possible  
**Scale/Scope**: Designed for production multi-tenant deployments with many users, each potentially holding several WebSocket connections and multiple subscriptions per connection

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- Library-first: Feature will extend existing crates (`kalamdb-core`, `kalamdb-api`, `link` TypeScript SDK) rather than creating ad-hoc binaries; live connection and subscription management will be encapsulated behind clear interfaces.
- CLI/interface orientation: Existing CLI and HTTP APIs will be respected; any new administrative kill-connection controls will be exposed via SQL commands (no new REST endpoints).
- Test-first: New behavior (multi-subscription connections, admin kill, SeqId resume) will be guarded by unit/integration tests and extended smoke tests for critical flows.
- Observability: Changes will ensure that `system.live_queries` remains the primary observability surface for active subscriptions, with additional logging/metrics added only where justified.

## Project Structure

### Documentation (this feature)

```text
specs/[###-feature]/
├── plan.md              # This file (/speckit.plan command output)
├── research.md          # Phase 0 output (/speckit.plan command)
├── data-model.md        # Phase 1 output (/speckit.plan command)
├── quickstart.md        # Phase 1 output (/speckit.plan command)
└── tasks.md             # Phase 2 output (/speckit.tasks command - NOT created by /speckit.plan)
```

### Source Code (repository root)
<!--
  ACTION REQUIRED: Replace the placeholder tree below with the concrete layout
  for this feature. Delete unused options and expand the chosen structure with
  real paths (e.g., apps/admin, packages/something). The delivered plan must
  not include Option labels.
-->

```text
specs/014-live-queries-websocket/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
└── tasks.md (created later by /speckit.tasks)

backend/
├── crates/
│   ├── kalamdb-core/          # Core live query and connection management
│   ├── kalamdb-api/           # HTTP/WebSocket API surface
│   ├── kalamdb-commons/       # Shared models including system tables
│   └── kalamdb-store/         # Storage abstraction (unchanged by this feature)
└── tests/                     # Existing integration and smoke tests

link/
├── sdks/
│   └── typescript/            # Kalam-link TypeScript SDK (WebSocket client)
└── tests/                     # SDK-level tests
```

**Structure Decision**: Use the existing multi-crate Rust backend layout and the `link/sdks/typescript` SDK package. All new feature-specific design docs live under `specs/014-live-queries-websocket/`, backend work primarily touches `kalamdb-core` (live management) and `kalamdb-api` (WebSocket/admin endpoints), and client changes are isolated to the TypeScript SDK.

## Complexity Tracking

No constitution violations are currently anticipated for this feature. If implementation reveals additional structural complexity (e.g., new crates or major cross-cutting abstractions), this section will be revisited and updated.
