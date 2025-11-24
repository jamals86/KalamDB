# Tasks: Live Queries over WebSocket Connections

**Input**: Design documents from `/specs/014-live-queries-websocket/`
**Prerequisites**: `plan.md`, `spec.md`, `research.md`, `data-model.md`, `quickstart.md`

**Tests**: Each user story includes at least one test-oriented task to keep flows independently verifiable.

**Organization**: Tasks are grouped by user story (P1–P4) so each slice can be implemented and tested on its own.

## Format: `[ID] [P?] [Story] Description`

- **\[P]**: Task can run in parallel (different files, no blocking dependency)
- **\[Story]**: Label user story ownership (US1..US4); omit for Setup/Foundational/Polish phases
- Include exact file paths so each task is self-contained

## Path Conventions

- Backend code lives under `backend/crates/` (e.g., `kalamdb-core`, `kalamdb-api`, `kalamdb-commons`)
- CLI/tests live under `cli/tests/`
- SDK work happens in `link/sdks/typescript/`
- Feature docs live under `specs/014-live-queries-websocket/`

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Align documentation/configuration before touching runtime code.

- [ ] T001 Document SQL-only admin controls (kill + live query inspection) in `specs/014-live-queries-websocket/plan.md` so downstream tasks avoid adding REST endpoints.
- [ ] T002 Update developer quickstart with SQL kill/query workflows and SeqId resume notes in `specs/014-live-queries-websocket/quickstart.md`.
- [ ] T003 \[P] Record Kalam-link shared lifecycle ownership (connect/disconnect/subscribe/unsubscribe/resume/list) in `specs/014-live-queries-websocket/research.md` to guide SDK work.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core data structures shared by every user story.

- [ ] T004 Update system table schema/types to include `connection_id`, `subscription_id`, and `last_seq_id` in `backend/crates/kalamdb-commons/src/system_tables.rs`.
- [ ] T005 \[P] Build an O(1) connection handler index keyed by `(user_id, connection_id)` inside `backend/crates/kalamdb-core/src/live/connection_registry.rs` so lookups avoid scanning users.
- [ ] T006 \[P] Implement immediate RocksDB deletion helpers for unsubscribe and socket-close flows in `backend/crates/kalamdb-core/src/live/connection_registry.rs`.
- [ ] T007 \[P] Ensure the `system.live_queries` table provider exposes the new fields and can refresh on every mutation in `backend/crates/kalamdb-core/src/tables/system/live_queries.rs`.
- [ ] T008 Define the Kalam-link shared lifecycle interface (connect/disconnect/subscribe/unsubscribe/resume/list) and exports in `link/src/client.rs` + `link/src/subscription.rs`.
- [ ] T009 Add shared TypeScript types/bindings aligned with the Kalam-link interface in `link/sdks/typescript/src/live/types.ts`.
- [ ] T010 \[P] Propagate auth-expiry and token-revocation events into the live manager so affected WebSocket connections close and run the shared teardown path within 5 seconds in `backend/crates/kalamdb-core/src/live/manager.rs` and supporting auth middleware.
- [ ] T011 Add a backend regression test that simulates auth expiry to ensure the WebSocket closes and `system.live_queries` rows disappear within the SLA in `backend/tests/test_live_queries_auth_expiry.rs`.

**Checkpoint**: Once Phase 2 completes, user stories can proceed independently.

---

## Phase 3: User Story 1 – Manage multiple subscriptions on one WebSocket (Priority: P1) 🎯 MVP

**Goal**: Allow a single WebSocket connection to host multiple live query subscriptions with independent unsubscribe.

**Independent Test**: Spin up one connection, subscribe to two queries, unsubscribe one, and confirm the second continues receiving updates.

### Implementation & Tests

- [ ] T012 \[P] \[US1] Implement per-connection subscription map and routing (leveraging the O(1) index) in `backend/crates/kalamdb-core/src/live/subscription_manager.rs`.
- [ ] T013 \[P] \[US1] Update WebSocket command handling to accept multiple `subscribe` messages and targeted `unsubscribe` in `backend/crates/kalamdb-api/src/websocket/live_queries.rs`.
- [ ] T014 \[US1] Persist unique `(connection_id, subscription_id)` rows and trigger immediate RocksDB removal on unsubscribe in `backend/crates/kalamdb-core/src/live/connection_registry.rs`.
- [ ] T015 \[US1] Add regression test covering multi-subscribe/unsubscribe behavior in `backend/crates/kalamdb-core/tests/live_multi_subscription.rs`.

**Checkpoint**: US1 delivers the MVP (multi-subscription WebSocket + unsubscribe).

---

## Phase 4: User Story 2 – Track live queries in `system.live_queries` (Priority: P2)

**Goal**: Ensure observability surfaces accurately list every active subscription with correct identifiers.

**Independent Test**: With multiple subscriptions active, `SELECT * FROM system.live_queries` returns one row per subscription; rows disappear when unsubscribed.

### Implementation & Tests

- [ ] T016 \[P] \[US2] Wire subscription lifecycle hooks (create/update/delete) to the `system.live_queries` provider in `backend/crates/kalamdb-core/src/tables/system/live_queries.rs`.
- [ ] T017 \[US2] Ensure socket-close paths remove all rows for a connection immediately in `backend/crates/kalamdb-core/src/live/connection_registry.rs`.
- [ ] T018 \[US2] Write a SQL integration test exercising the metadata view in `backend/tests/test_live_queries_metadata.rs`.

---

## Phase 5: User Story 3 – Admin terminates a live connection via SQL (Priority: P3)

**Goal**: Admin-issued SQL kill commands close a WebSocket connection and drop all its subscriptions.

**Independent Test**: Issue the existing SQL kill statement targeting a connection; verify socket closes and `system.live_queries` shows no rows for that connection.

### Implementation & Tests

- [ ] T019 \[P] \[US3] Extend the kill-command SQL handler in `backend/crates/kalamdb-core/src/sql/executor/handlers/kill.rs` to accept WebSocket connection identifiers.
- [ ] T020 \[US3] Invoke live manager shutdown logic (including immediate RocksDB cleanup) when killing a connection in `backend/crates/kalamdb-core/src/live/manager.rs`.
- [ ] T021 \[US3] Add CLI integration test covering admin kill in `cli/tests/test_kill_connection.rs`.

---

## Phase 6: User Story 4 – SDK manages connection lifecycle (Priority: P4)

**Goal**: Kalam-link SDK initializes/authenticates, reuses a single WebSocket, manages multiple subscriptions, and auto-resumes with SeqIds.

**Independent Test**: Using the SDK alone, init/auth, subscribe twice, call `listSubscriptions`, unsubscribe one, and observe automatic reconnect + SeqId resume after a forced disconnect.

### Implementation & Tests

- [ ] T022 \[P] \[US4] Implement the shared Kalam-link connection lifecycle manager (connect/disconnect/subscribe/unsubscribe/resume/list) in `link/src/client.rs` and `link/src/subscription.rs`.
- [ ] T023 \[P] \[US4] Expose WASM/FFI bindings so the TypeScript SDK can call the Kalam-link lifecycle helpers via `link/src/wasm.rs`.
- [ ] T024 \[P] \[US4] Wire the TypeScript SDK to call the shared library helpers (view, connect/disconnect, subscribe/unsubscribe, resume) in `link/sdks/typescript/src/live/client.ts` and `link/sdks/typescript/src/index.ts`.
- [ ] T025 \[US4] Add SDK unit tests covering multi-subscribe, resume, and list operations via the shared library in `link/sdks/typescript/tests/liveClient.spec.ts`.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Final hardening across backend and SDK.

- [ ] T026 \[P] Add logging + metrics for connection/subscription lifecycle events in `backend/crates/kalamdb-core/src/live/manager.rs`.
- [ ] T027 \[P] Update user-facing docs with SDK usage examples (including shared link library usage) in `docs/SDK.md` and `docs/SQL.md`.
- [ ] T028 Add an end-to-end integration test that covers multi-subscribe, unsubscribe, resume, view subscriptions, admin kill, and auth-expiry teardown flows in `backend/tests/test_live_queries_full_flow.rs`.
- [ ] T029 Run `cargo test --test smoke` from `cli/` and `pnpm test` from `link/sdks/typescript/` (ensuring the new integration and auth-expiry tests are included) to confirm regressions are absent.

---

## Dependencies & Execution Order

1. **Phase 1 → Phase 2**: Setup must finish before foundational data structures change.
2. **Phase 2 → User Stories**: Foundational library changes block every user story.
3. **User Story Order**: Prioritize US1 (MVP), then US2 → US3 → US4. Stories can run in parallel after Phase 2 if coordination exists, but each stays independently testable.
4. **Polish** depends on completion of whichever user stories are in scope for the release.

### Story Dependency Graph

- US1 has no story-level dependencies once Phase 2 is done.
- US2 depends on US1 data structures but not on US1 features being fully shipped (shared registry code already exists post Phase 2).
- US3 depends on US1/US2 cleanup semantics so kill operations can remove rows cleanly.
- US4 depends on US1 behavior (multi-subscribe support) and benefits from US2 metadata for debugging.

### Parallel Execution Examples

- After Phase 2, T012 (backend multi-subscribe) and T022 (shared Kalam-link lifecycle manager) can run concurrently because they live in different repo paths.
- Within US1, T012 and T013 are marked \[P] and can proceed in parallel since one touches core manager logic and the other touches API wiring.
- Test tasks (T015, T018, T021, T025) can run while corresponding implementation tasks are underway, provided mocks/stubs exist.

---

## Implementation Strategy

### MVP Scope

Deliver Phase 1–3 (through User Story 1). This enables multiplexed subscriptions with independent unsubscribe on a single WebSocket, meeting the minimum user promise.

### Incremental Delivery

1. Ship MVP (US1) once tests pass.
2. Add US2 to expose observability data.
3. Layer in US3 to let admins kill rogue connections.
4. Finish with US4 to provide a production-ready SDK experience.
5. Reserve Phase 7 polish tasks for the stabilization sprint before release.

---

## Task Counts

- **Total tasks**: 29
- **By user story**:
  - US1: 4 tasks (T012–T015)
  - US2: 3 tasks (T016–T018)
  - US3: 3 tasks (T019–T021)
  - US4: 4 tasks (T022–T025)
- **Setup/Foundational/Polish**: 15 tasks (T001–T011, T026–T029)

All tasks follow the required checklist format.
