# Tasks: Docker Container, WASM Compilation, and TypeScript Examples

**Input**: Design documents from `/specs/006-docker-wasm-examples/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `- [ ] [ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US0, US1, US2, US3)
- Include exact file paths in descriptions

## Path Conventions

This is a multi-component project:
- `backend/crates/` - Rust backend crates
- `link/kalam-link/` - WASM-compiled client library (dual-purpose: CLI dependency + standalone WASM)
- `cli/kalam-cli/` - CLI tool (links to kalam-link)
- `examples/simple-typescript/` - React example app
- `docker/backend/` - Docker deployment files

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project initialization and verification of existing structure

- [x] T001 Verify Rust 1.75+ toolchain and wasm32-unknown-unknown target installed
- [x] T002 Verify Docker 20+ and Docker Compose 2.0+ are installed
- [x] T003 [P] Verify wasm-pack 0.12+ is installed
- [x] T004 [P] Verify Node.js 18+ and npm are installed
- [x] T005 [P] Create examples/simple-typescript directory structure
- [x] T006 [P] Create docker/backend directory structure (already exists from plan phase)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core infrastructure that MUST be complete before ANY user story can be implemented

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [x] T007 Add uuid crate dependency to backend/crates/kalamdb-sql/Cargo.toml for API key generation
- [x] T008 Update User model in backend/crates/kalamdb-sql/src/models.rs to add apikey and role fields
- [x] T009 Create RocksDB secondary index for apikey in backend/crates/kalamdb-sql/src/adapter.rs
- [x] T010 Add UserTableRow deleted field to backend/crates/kalamdb-store/src/user_table_store.rs
- [x] T011 Implement soft delete query rewriting in backend/crates/kalamdb-core/src/tables/user_table_provider.rs

**Checkpoint**: Foundation ready - user story implementation can now begin in parallel

---

## Phase 2.5: Core Bug Fixes and Refactoring (Critical Path)

**Purpose**: Fix critical bugs in UPDATE/DELETE row count behavior and reorganize project structure

**⚠️ IMPORTANT**: These fixes should be completed before widespread WASM usage to ensure correct behavior

### PostgreSQL-Compatible Row Count Behavior

- [x] T011A Research PostgreSQL UPDATE behavior: Does it return count when no changes? Or only when values actually change?
- [x] T011B Research PostgreSQL DELETE behavior with soft delete: How does it report affected rows?
- [x] T011C Fix UPDATE handler in backend/crates/kalamdb-core/src/sql/executor.rs to return actual affected row count (currently returns 0)
- [x] T011D Fix DELETE handler in backend/crates/kalamdb-core/src/sql/executor.rs to return actual affected row count for soft deletes
- [x] T011E Add integration test in backend/tests/ to verify UPDATE returns correct count (e.g., "Updated 1 row(s)" when updating existing row)
- [x] T011F Add integration test in backend/tests/ to verify DELETE returns correct count (e.g., "Deleted 1 row(s)" when soft-deleting existing row)
- [x] T011G Verify UPDATE with no actual changes (same values) follows PostgreSQL behavior (document in test)
- [x] T011H Add support for DELETE with LIKE pattern or document limitation in error message

### Project Structure Refactoring: Move kalam-link to /link

**Rationale**: `kalam-link` is used both as a CLI dependency AND as a standalone WASM library for TypeScript/other languages. Moving to `/link` makes this dual-purpose clear and improves project organization.

**SOLUTION: Created unified root-level workspace at repository root**

Cargo requires workspace members to be hierarchically below the workspace root. Solution was to create `/Cargo.toml` at repository root that includes all backend crates, link/, and cli/kalam-cli, and remove the separate backend/ and cli/ workspaces for unified builds.

**IMPROVEMENT: Simplified to cli/ and moved kalamdb-server to backend/**

Following Rust best practices, moved main binaries to their directory roots:
- `cli/kalam-cli/` → `cli/` (matches `link/` pattern)
- `backend/crates/kalamdb-server/` → `backend/` (main server binary at workspace root, libraries in `crates/`)

This matches industry patterns (e.g., cargo binary + cargo-* libraries) and makes entry points immediately discoverable.

- [x] T011I Create /link directory at repository root
- [x] T011J Move cli/kalam-link/ to /link/ (renamed directory structure)
- [x] T011K Update cli/kalam-cli/Cargo.toml to reference ../../link as dependency path
- [x] T011L Create root Cargo.toml workspace including backend/, link/, and cli/
- [x] T011M Remove backend/Cargo.toml and cli/Cargo.toml workspaces (backed up as *.backup)
- [x] T011N Verified unified workspace: `cargo clean` now cleans all projects (removed 3790 files, 2.2GiB)
- [x] T011O Verified kalam-cli builds successfully: cargo build -p kalam-cli
- [x] T011P Verified kalam-link builds successfully: cargo build -p kalam-link  
- [x] T011Q Verified single target directory at root (no backend/target or cli/target)
- [x] T011R All builds use unified /target directory for efficient caching
- [x] T011S Move cli/kalam-cli/ to cli/ for consistency with link/
- [x] T011T Move backend/crates/kalamdb-server/ to backend/ (main binary pattern)
- [x] T011U Update backend/Cargo.toml paths to crates/* (from ../*) 
- [x] T011V Update cli/Cargo.toml kalam-link path to ../link (from ../../link)
- [x] T011W Verified complete workspace builds: cargo check --workspace (1m 20s)

**Final Structure**:
```
/
├── Cargo.toml (workspace root)
├── backend/
│   ├── Cargo.toml (kalamdb-server binary)
│   ├── src/main.rs (server entry point)
│   └── crates/ (supporting libraries)
│       ├── kalamdb-commons/
│       ├── kalamdb-core/
│       ├── kalamdb-sql/
│       ├── kalamdb-store/
│       ├── kalamdb-api/
│       └── kalamdb-live/
├── cli/
│   ├── Cargo.toml (kalam-cli binary)
│   ├── src/main.rs (CLI entry point)
│   └── tests/
└── link/
    ├── Cargo.toml (kalam-link library)
    ├── src/lib.rs (Rust library)
    └── pkg/ (WASM output)
```

**Benefits**:
- ✅ Single `cargo clean` cleans entire project
- ✅ Single `cargo build --workspace` builds all components
- ✅ Unified dependency resolution and caching
- ✅ No duplicate target directories (saves disk space)
- ✅ Clear project structure: /backend, /link, /cli
- ✅ **Discoverable entry points**: backend/src/main.rs and cli/src/main.rs
- ✅ **Industry standard pattern**: Main binaries at workspace root, libraries in subdirectories
- ✅ **Consistency**: All three top-level directories follow same pattern

**Checkpoint**: ✅ Project structure refactored successfully, unified workspace verified, PostgreSQL-compatible behavior confirmed

---

## Phase 3: User Story 0 - API Key Authentication and Soft Delete (Priority: P0) 🎯 Prerequisite

**Goal**: Enable API key authentication via X-API-KEY header and soft delete for user tables, making them available for WASM client and examples

**Independent Test**: Create user with create-user command, get API key, make SQL API request with X-API-KEY header, delete a row and verify it's soft-deleted (hidden from SELECT but recoverable)

### Implementation for User Story 0

- [x] T012 [P] [US0] Implement create-user command in backend/crates/kalamdb-server/src/commands/create_user.rs with auto-generated UUID API key
- [x] T013 [P] [US0] Add role validation function in backend/crates/kalamdb-core/src/auth/roles.rs (validate against: admin, user, readonly)
- [x] T014 [US0] Update insert_user in backend/crates/kalamdb-sql/src/adapter.rs to store apikey and role fields
- [x] T015 [US0] Implement get_user_by_apikey in backend/crates/kalamdb-sql/src/adapter.rs using apikey secondary index
- [x] T016 [P] [US0] Create ApiKeyAuth middleware in backend/crates/kalamdb-api/src/middleware/api_key_auth.rs to extract X-API-KEY header
- [x] T017 [US0] Implement localhost exception in ApiKeyAuth middleware (allow 127.0.0.1 without API key)
- [x] T018 [US0] Add ApiKeyAuth middleware to /sql route in backend/crates/kalamdb-api/src/routes/sql.rs
- [x] T019 [US0] Return 401 Unauthorized when X-API-KEY is invalid or missing in backend/crates/kalamdb-api/src/middleware/api_key_auth.rs
- [x] T020 [P] [US0] Implement soft delete in DELETE handler in backend/crates/kalamdb-core/src/handlers/delete_handler.rs (set deleted=true instead of physical removal)
- [x] T021 [P] [US0] Add deleted field filtering in SELECT query execution in backend/crates/kalamdb-core/src/tables/user_table_provider.rs (WHERE deleted = false)
- [x] T022 [US0] Update existing user table tests in backend/tests/test_stream_tables.rs to work with soft delete behavior
- [x] T023 [US0] Create integration test for API key auth in backend/tests/test_api_key_auth.rs (create user, test with/without key)
- [x] T024 [US0] Create integration test for soft delete in backend/tests/test_soft_delete.rs (delete row, verify hidden but exists)
- [x] T025 [US0] Add create-user command to main.rs in backend/crates/kalamdb-server/src/main.rs with CLI argument parsing
- [x] T026 [US0] Update kalam-cli to support user create command in cli/kalam-cli/src/commands/user.rs

**Checkpoint**: At this point, User Story 0 should be fully functional - API key auth works, soft delete works, tests pass

---

## Phase 4: User Story 1 - Docker Deployment with Configuration (Priority: P1)

**Goal**: Provide production-ready Docker deployment with environment variable configuration and data persistence

**Independent Test**: Run build-backend.sh, start with docker-compose up, create user with API key inside container, verify data persists after restart, test environment variable overrides

### Implementation for User Story 1

- [x] T027 [P] [US1] Verify Dockerfile exists in docker/backend/Dockerfile (already created in plan phase)
- [x] T028 [P] [US1] Verify docker-compose.yml exists in docker/backend/docker-compose.yml (already created in plan phase)
- [x] T029 [P] [US1] Verify build-backend.sh exists in docker/backend/build-backend.sh (already created in plan phase)
- [x] T030 [US1] Add environment variable parsing in backend/crates/kalamdb-server/src/config.rs (KALAMDB_SERVER_PORT, KALAMDB_LOG_LEVEL, etc.)
- [x] T031 [US1] Update config loading to prioritize env vars over config.toml in backend/crates/kalamdb-server/src/config.rs
- [ ] T032 [US1] Build Docker image using docker/backend/build-backend.sh and verify kalamdb-server binary exists
- [ ] T033 [US1] Run docker-compose up and verify container starts successfully with default env vars
- [ ] T032 [US1] Test Docker build with ./build-backend.sh and verify both kalamdb-server and kalam-cli are included
- [ ] T033 [US1] Test docker-compose up and verify server starts with default environment variables
- [ ] T034 [US1] Test creating user inside container with docker exec -it kalamdb kalam-cli user create
- [ ] T035 [US1] Test data persistence by stopping container, restarting, and verifying data still exists
- [ ] T036 [US1] Test environment variable overrides in docker-compose.yml (change port, log level) and verify server respects them
- [ ] T037 [P] [US1] Create .env.example in docker/backend/.env.example (already created in plan phase)
- [ ] T038 [P] [US1] Update docker/README.md with complete deployment instructions (already created in plan phase)

**Checkpoint**: At this point, User Stories 0 AND 1 should both work independently - Docker deployment is production-ready

---

## Phase 5: User Story 2 - WASM Compilation for Browser/Node.js Use (Priority: P2)

**Goal**: Compile kalam-link to WebAssembly for JavaScript/TypeScript usage with proper type definitions

**Independent Test**: Run wasm-pack build, import WASM module in Node.js test script, initialize with URL and API key, execute basic query

### Implementation for User Story 2

- [x] T039 [P] [US2] Add wasm-bindgen dependencies to cli/kalam-link/Cargo.toml
- [x] T040 [P] [US2] Add wasm-bindgen-futures dependency to cli/kalam-link/Cargo.toml for async support
- [x] T041 [P] [US2] Create WASM bindings module in cli/kalam-link/src/wasm.rs
- [x] T042 [US2] Implement KalamClient struct in cli/kalam-link/src/wasm.rs with url and apikey fields (both required)
- [x] T043 [US2] Add #[wasm_bindgen] constructor for KalamClient::new(url: String, api_key: String) in cli/kalam-link/src/wasm.rs
- [x] T044 [US2] Implement validation for required url and apikey parameters with clear error messages in cli/kalam-link/src/wasm.rs
- [x] T045 [P] [US2] Implement connect() method in cli/kalam-link/src/wasm.rs for WebSocket connection
- [x] T046 [P] [US2] Implement disconnect() method in cli/kalam-link/src/wasm.rs
- [x] T047 [P] [US2] Implement isConnected() method in cli/kalam-link/src/wasm.rs
- [x] T048 [US2] Implement insert() method in cli/kalam-link/src/wasm.rs accepting table name and JSON data
- [x] T049 [US2] Implement delete() method in cli/kalam-link/src/wasm.rs accepting table name and row ID
- [x] T050 [US2] Implement query() method in cli/kalam-link/src/wasm.rs accepting SQL string and returning QueryResult
- [x] T051 [US2] Implement subscribe() method in cli/kalam-link/src/wasm.rs accepting table name and callback
- [x] T052 [US2] Implement unsubscribe() method in cli/kalam-link/src/wasm.rs accepting subscription ID
- [x] T053 [US2] Add X-API-KEY header to all WebSocket and HTTP requests in cli/kalam-link/src/wasm.rs
- [X] T054 [US2] Build WASM module with wasm-pack build --target web --out-dir pkg from link/ (created build.sh script)
- [X] T055 [US2] Verify WASM output includes .wasm file, .js bindings, .d.ts types, and package.json in link/pkg/
- [X] T056 [US2] Create TypeScript SDK structure in link/sdks/typescript with package.json, README.md, and test files
- [X] T057 [US2] Create comprehensive test suite: basic.test.mjs, connection.test.mjs, query.test.mjs, subscription.test.mjs
- [X] T058 [US2] Build script automatically copies WASM artifacts to TypeScript SDK directory
- [X] T059 [US2] Create detailed README.md for TypeScript SDK with API reference, examples, and troubleshooting

**Checkpoint**: All user stories 0, 1, and 2 should now be independently functional - WASM client is ready for JavaScript use

---

## Phase 6: User Story 3 - TypeScript TODO App Example (Priority: P3)

**Goal**: Provide a complete React example demonstrating real-time subscriptions with localStorage caching and offline-first capabilities

**Independent Test**: Run setup.sh to create tables, start React app, add/delete TODOs through UI, verify real-time sync across tabs, test localStorage persistence and reconnection sync

### Implementation for User Story 3

- [ ] T060 [P] [US3] Create package.json in examples/simple-typescript/ with React, TypeScript, Vite dependencies
- [ ] T061 [P] [US3] Create tsconfig.json in examples/simple-typescript/ with ES2020+ target
- [ ] T062 [P] [US3] Create vite.config.ts in examples/simple-typescript/ for build configuration
- [ ] T063 [P] [US3] Create todo-app.sql in examples/simple-typescript/ with CREATE TABLE IF NOT EXISTS for todos table
- [ ] T064 [US3] Create setup.sh in examples/simple-typescript/ that validates KalamDB accessibility and loads todo-app.sql via kalam-cli
- [ ] T065 [US3] Make setup.sh executable with proper error handling and idempotent behavior
- [ ] T066 [P] [US3] Create .env.example in examples/simple-typescript/ with VITE_KALAMDB_URL and VITE_KALAMDB_API_KEY
- [ ] T067 [P] [US3] Create src/types/todo.ts with TODO type definition (id, title, completed, created_at)
- [ ] T068 [P] [US3] Create src/services/kalamClient.ts wrapping WASM client with initialization
- [ ] T069 [US3] Initialize KalamClient in src/services/kalamClient.ts with URL and API key from .env
- [ ] T070 [P] [US3] Create src/services/localStorage.ts for reading/writing TODOs and last sync ID
- [ ] T071 [P] [US3] Implement loadTodosFromCache() in src/services/localStorage.ts
- [ ] T072 [P] [US3] Implement saveTodosToCache() in src/services/localStorage.ts
- [ ] T073 [P] [US3] Implement getLastSyncId() and setLastSyncId() in src/services/localStorage.ts
- [ ] T074 [US3] Create src/hooks/useTodos.ts custom hook managing TODO state with useState
- [ ] T075 [US3] Implement initial load from localStorage in src/hooks/useTodos.ts
- [ ] T076 [US3] Implement subscription to TODO changes in src/hooks/useTodos.ts (subscribe from last sync ID)
- [ ] T077 [US3] Implement connection status tracking in src/hooks/useTodos.ts with useState
- [ ] T078 [US3] Handle insert events from subscription and update both state and localStorage in src/hooks/useTodos.ts
- [ ] T079 [US3] Handle delete events from subscription and update both state and localStorage in src/hooks/useTodos.ts
- [ ] T080 [US3] Implement addTodo() function using WASM client insert in src/hooks/useTodos.ts
- [ ] T081 [US3] Implement deleteTodo() function using WASM client delete in src/hooks/useTodos.ts
- [ ] T082 [P] [US3] Create src/components/ConnectionStatus.tsx showing "Connected"/"Disconnected" badge with color
- [ ] T083 [P] [US3] Create src/components/TodoList.tsx displaying todos from state
- [ ] T084 [P] [US3] Create src/components/TodoItem.tsx with delete button
- [ ] T085 [P] [US3] Create src/components/AddTodoForm.tsx with input and add button (disabled when disconnected)
- [ ] T086 [US3] Create src/App.tsx composing all components and using useTodos hook
- [ ] T087 [US3] Create src/main.tsx as React entry point with StrictMode
- [ ] T088 [P] [US3] Create index.html in examples/simple-typescript/
- [ ] T089 [P] [US3] Create src/styles/App.css with basic styling
- [ ] T090 [US3] Test setup.sh creates todos table successfully
- [ ] T091 [US3] Test React app starts with npm run dev
- [ ] T092 [US3] Test adding TODO through UI persists to KalamDB and localStorage
- [ ] T093 [US3] Test deleting TODO through UI removes from KalamDB and localStorage
- [ ] T094 [US3] Test real-time sync by opening two browser tabs and verifying changes propagate
- [ ] T095 [US3] Test localStorage persistence by closing app and reopening (TODOs load instantly)
- [ ] T096 [US3] Test reconnection sync by disconnecting, adding TODO elsewhere, reconnecting (app syncs missed changes)
- [ ] T097 [US3] Test connection status badge shows correct state (Connected/Disconnected)
- [ ] T098 [US3] Test add button is disabled when WebSocket is disconnected
- [ ] T099 [P] [US3] Create README.md in examples/simple-typescript/ with setup and usage instructions
- [ ] T100 [P] [US3] Add package.json scripts for dev, build, and test in examples/simple-typescript/

**Checkpoint**: All user stories should now be independently functional - Complete TODO app example is ready for developers

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Improvements that affect multiple user stories and final validation

- [ ] T101 [P] Update main project README.md in repository root with links to new Docker and WASM features
- [ ] T102 [P] Update backend/README.md with create-user command documentation
- [ ] T103 [P] Update cli/kalam-link/README.md with WASM compilation instructions
- [ ] T104 Verify all quickstart.md examples work end-to-end from specs/006-docker-wasm-examples/quickstart.md
- [ ] T105 Run full test suite (cargo test) and verify all tests pass including new API key and soft delete tests
- [ ] T106 [P] Add logging for API key authentication events in backend/crates/kalamdb-api/src/middleware/api_key_auth.rs
- [ ] T107 [P] Add error handling documentation for common WASM client errors in examples/simple-typescript/README.md
- [ ] T108 Code review and cleanup across all modified files
- [ ] T109 Performance validation: API key auth overhead <10ms, soft delete queries same as hard delete
- [ ] T110 Security review: Verify API keys are logged safely, localhost exception is secure

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS all user stories
- **Core Bug Fixes & Refactoring (Phase 2.5)**: Depends on Foundational - RECOMMENDED before User Stories (ensures correct behavior)
  - PostgreSQL compatibility: Should complete before WASM widespread usage
  - Structure refactoring: Should complete before WASM packaging (affects import paths)
- **User Story 0 (Phase 3)**: Depends on Foundational - BLOCKS User Stories 2 and 3 (they need API key auth)
- **User Story 1 (Phase 4)**: Depends on Foundational and User Story 0 - Independent otherwise
- **User Story 2 (Phase 5)**: Depends on Foundational, User Story 0, and Phase 2.5 structure refactoring - Independent otherwise
- **User Story 3 (Phase 6)**: Depends on Foundational, User Story 0, Phase 2.5, and User Story 2 (needs WASM client)
- **Polish (Phase 7)**: Depends on all desired user stories being complete

### User Story Dependencies

- **Phase 2.5 (Bug Fixes)**: RECOMMENDED before user stories - ensures PostgreSQL compatibility and clean structure
  - Row count fixes prevent incorrect behavior in production
  - Structure refactoring prevents path confusion in WASM usage
- **User Story 0 (P0)**: BLOCKING - Must complete before US2 and US3
  - US2 needs API key authentication working
  - US3 needs both API key auth and soft delete working
- **User Story 1 (P1)**: Can start after Foundational + US0 - Independent of US2/US3
- **User Story 2 (P2)**: Can start after Foundational + US0 + Phase 2.5 structure - BLOCKING for US3 (US3 needs WASM client)
- **User Story 3 (P3)**: Depends on US0, Phase 2.5, and US2 being complete

### Critical Path

```
Setup → Foundational → Phase 2.5 (Bug Fixes + Refactor) → US0 (API Key + Soft Delete) → US2 (WASM) → US3 (React Example)
                                      ↓
                                     US1 (Docker) [can proceed in parallel after US0]
```

### Parallel Opportunities

**Within Setup (Phase 1)**:
- T003, T004, T005, T006 can all run in parallel

**Within Foundational (Phase 2)**:
- T007 and T008 can run in parallel with T010 and T011

**Within Core Bug Fixes & Refactoring (Phase 2.5)**:
- T011A, T011B can run in parallel (PostgreSQL research)
- T011C, T011D can run in parallel (UPDATE/DELETE handler fixes - different code sections)
- T011E, T011F, T011G can run in parallel (different test files)
- T011I-T011R: Structure refactoring tasks should run sequentially (file moves are order-dependent)

**Within User Story 0**:
- T012, T013 can run in parallel
- T016, T020, T021 can run in parallel (different files)
- T023, T024 can run in parallel (different test files)

**Within User Story 1**:
- T027, T028, T029, T037, T038 can run in parallel (verification/documentation tasks)

**Within User Story 2**:
- T039, T040, T041 can run in parallel (dependency additions)
- T045, T046, T047 can run in parallel (connection methods)

**Within User Story 3**:
- T060, T061, T062, T063, T066, T067 can all run in parallel (config and type files)
- T070, T071, T072, T073 can run in parallel (localStorage utilities)
- T082, T083, T084, T085 can run in parallel (React components)
- T088, T089, T099, T100 can run in parallel (static files and docs)

**Cross-Story Parallelism**:
- Once US0 is complete, US1 and US2 can proceed in parallel
- US1 (Docker) has no dependencies on US2 or US3

---

## Parallel Example: User Story 0

```bash
# Launch parallelizable tasks together:
Task T012: "Implement create-user command in backend/crates/kalamdb-server/src/commands/create_user.rs"
Task T013: "Add role validation in backend/crates/kalamdb-core/src/auth/roles.rs"

# Then in next batch:
Task T016: "Create ApiKeyAuth middleware in backend/crates/kalamdb-api/src/middleware/api_key_auth.rs"
Task T020: "Implement soft delete in DELETE handler in backend/crates/kalamdb-core/src/handlers/delete_handler.rs"
Task T021: "Add deleted field filtering in SELECT in backend/crates/kalamdb-core/src/tables/user_table_provider.rs"

# Finally:
Task T023: "Create test_api_key_auth.rs"
Task T024: "Create test_soft_delete.rs"
```

---

## Implementation Strategy

### MVP First (User Story 0 + User Story 1)

1. Complete Phase 1: Setup
2. Complete Phase 2: Foundational (CRITICAL - blocks all stories)
3. Complete Phase 3: User Story 0 (API Key + Soft Delete) - PREREQUISITE
4. Complete Phase 4: User Story 1 (Docker) - PRODUCTION DEPLOYMENT
5. **STOP and VALIDATE**: Test US0 + US1 independently
6. Deploy/demo Docker deployment with API key auth

This delivers a production-ready KalamDB deployment with authentication.

### Full Feature Delivery

1. Complete Setup + Foundational → Foundation ready
2. Add User Story 0 → Test independently → CRITICAL MILESTONE
3. Add User Story 1 → Test independently → Deploy/Demo (Production Ready!)
4. Add User Story 2 → Test independently → WASM client available
5. Add User Story 3 → Test independently → Deploy/Demo (Complete Example!)
6. Each story adds value without breaking previous stories

### Parallel Team Strategy

With multiple developers:

1. Team completes Setup + Foundational together (T001-T011)
2. Team completes Core Bug Fixes & Refactoring (Phase 2.5):
   - **Developer A**: PostgreSQL research and row count fixes (T011A-T011H)
   - **Developer B**: Project structure refactoring - move kalam-link (T011I-T011R)
3. Team completes User Story 0 together (T012-T026) - CRITICAL
4. Once US0 is done:
   - **Developer A**: User Story 1 (Docker - T027-T038)
   - **Developer B**: User Story 2 (WASM - T039-T059)
5. Once US0 and US2 are done:
   - **Developer C**: User Story 3 (React Example - T060-T100)
6. Team completes Polish together (T101-T110)

---

## Task Count Summary

- **Total Tasks**: 128 (18 new tasks added)
- **Setup (Phase 1)**: 6 tasks
- **Foundational (Phase 2)**: 5 tasks (BLOCKING)
- **Core Bug Fixes & Refactoring (Phase 2.5)**: 18 tasks (CRITICAL - PostgreSQL compatibility + structure cleanup)
  - PostgreSQL-compatible row counts: 8 tasks (T011A-T011H)
  - Project structure refactoring: 10 tasks (T011I-T011R)
- **User Story 0 (Phase 3)**: 15 tasks (API Key + Soft Delete)
- **User Story 1 (Phase 4)**: 12 tasks (Docker)
- **User Story 2 (Phase 5)**: 21 tasks (WASM)
- **User Story 3 (Phase 6)**: 41 tasks (React Example)
- **Polish (Phase 7)**: 10 tasks

**Parallel Opportunities**: 45 tasks marked [P] can run in parallel within their phase

**Independent Test Criteria**:
- **US0**: Create user → Get API key → Make authenticated SQL request → Soft delete row → Verify hidden
- **US1**: Build Docker image → Start container → Create user inside → Verify data persists → Test env var override
- **US2**: Compile WASM → Import in Node.js → Initialize with URL+API key → Execute query
- **US3**: Run setup.sh → Start React app → Add TODO → Delete TODO → Verify multi-tab sync → Test offline/reconnect

**Suggested MVP Scope**: User Story 0 + User Story 1 (API key authentication + Docker deployment)

---

## Notes

- [P] tasks = different files, no dependencies within phase
- [Story] label maps task to specific user story for traceability
- User Story 0 is BLOCKING for US2 and US3 (they need API key auth)
- User Story 2 is BLOCKING for US3 (needs WASM client)
- User Story 1 can proceed in parallel with US2 after US0
- Each user story should be independently completable and testable
- Commit after each task or logical group
- Stop at any checkpoint to validate story independently
- Docker files already created in plan phase (T027-T029, T037-T038 are verification tasks)
