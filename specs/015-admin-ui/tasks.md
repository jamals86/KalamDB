# Tasks: Admin UI with Token-Based Authentication

**Input**: Design documents from `/specs/015-admin-ui/`  
**Prerequisites**: plan.md ✅, spec.md ✅, research.md ✅, data-model.md ✅, contracts/ ✅

**Organization**: Tasks are grouped by user story to enable independent implementation and testing.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

## Path Conventions

- **Backend**: `backend/crates/kalamdb-api/src/`, `backend/crates/kalamdb-auth/src/`
- **Frontend**: `ui/src/`
- **Tests**: `backend/tests/`, `ui/tests/`

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project initialization and basic structure for both backend and frontend

- [x] T001 Create `ui/` directory structure per implementation plan
- [x] T002 Initialize React project with Vite in `ui/package.json`
- [x] T003 [P] Configure TypeScript in `ui/tsconfig.json`
- [x] T004 [P] Configure Tailwind CSS in `ui/tailwind.config.ts`
- [x] T005 [P] Initialize shadcn/ui with `ui/components.json`
- [x] T006 [P] Configure Vite with API proxy in `ui/vite.config.ts`
- [x] T007 [P] Add workspace dependency `cookie` to root `Cargo.toml`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core infrastructure that MUST be complete before ANY user story can be implemented

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

### Backend Auth Infrastructure

- [x] T008 Create HttpOnly cookie handling module in `backend/crates/kalamdb-auth/src/cookie.rs`
- [x] T009 Add JWT token generation function to `backend/crates/kalamdb-auth/src/jwt_auth.rs`
- [x] T010 Add JWT token refresh function to `backend/crates/kalamdb-auth/src/jwt_auth.rs`
- [x] T011 Create auth handlers module in `backend/crates/kalamdb-api/src/handlers/auth.rs`
- [x] T012 Implement POST `/v1/api/auth/login` handler in `backend/crates/kalamdb-api/src/handlers/auth.rs`
- [x] T013 Implement POST `/v1/api/auth/refresh` handler in `backend/crates/kalamdb-api/src/handlers/auth.rs`
- [x] T014 Implement POST `/v1/api/auth/logout` handler in `backend/crates/kalamdb-api/src/handlers/auth.rs`
- [x] T015 Implement GET `/v1/api/auth/me` handler in `backend/crates/kalamdb-api/src/handlers/auth.rs`
- [x] T016 Add auth routes to `backend/crates/kalamdb-api/src/routes.rs`
- [x] T017 Add cookie-based auth middleware/extractor in `backend/crates/kalamdb-auth/src/extractor.rs`
- [x] T018 Configure static file serving for `/ui` route in `backend/crates/kalamdb-api/src/routes.rs`

### Frontend Core Infrastructure

- [x] T019 [P] Create API client with cookie auth in `ui/src/lib/api.ts`
- [x] T020 [P] Create auth context and hooks in `ui/src/lib/auth.tsx`
- [x] T021 [P] Install and configure shadcn Button component in `ui/src/components/ui/button.tsx`
- [x] T022 [P] Install and configure shadcn Input component in `ui/src/components/ui/input.tsx`
- [x] T023 [P] Install and configure shadcn Card component in `ui/src/components/ui/card.tsx`
- [x] T024 Create app entry point with router in `ui/src/App.tsx`
- [x] T025 Create main layout with sidebar in `ui/src/components/layout/Layout.tsx`
- [x] T026 [P] Create sidebar navigation in `ui/src/components/layout/Sidebar.tsx`
- [x] T027 [P] Create header with user menu in `ui/src/components/layout/Header.tsx`

**Checkpoint**: Foundation ready - user story implementation can now begin

---

## Phase 3: User Story 1 - Token-Based Authentication Login (Priority: P1) 🎯 MVP

**Goal**: Administrators can log in once with username/password and receive an access token stored in HttpOnly cookie for all subsequent requests.

**Independent Test**: Log in through UI → receive cookie → make authenticated SQL request → logout → verify cookie cleared

### Implementation for User Story 1

- [x] T028 [US1] Create Login page component in `ui/src/pages/Login.tsx`
- [x] T029 [US1] Create login form with validation in `ui/src/components/auth/LoginForm.tsx`
- [x] T030 [US1] Implement login API call in `ui/src/lib/auth.tsx`
- [x] T031 [US1] Implement token refresh logic (silent refresh before expiry) in `ui/src/lib/auth.tsx`
- [x] T032 [US1] Implement logout functionality in `ui/src/lib/auth.tsx`
- [x] T033 [US1] Create protected route wrapper in `ui/src/components/auth/ProtectedRoute.tsx`
- [x] T034 [US1] Create Dashboard page (landing after login) in `ui/src/pages/Dashboard.tsx`
- [x] T035 [US1] Add role check to restrict UI to dba/system roles in `ui/src/components/auth/ProtectedRoute.tsx`
- [x] T036 [US1] Handle 401 responses globally and redirect to login in `ui/src/lib/api.ts`

**Checkpoint**: User Story 1 complete - login/logout flow fully functional

---

## Phase 4: User Story 2 - SQL Studio with Query Execution (Priority: P2)

**Goal**: Administrators can write and execute SQL queries with syntax highlighting and autocomplete, viewing results in a data grid.

**Independent Test**: Open SQL Studio → type query with autocomplete → execute → see results in grid → sort/filter results

### Implementation for User Story 2

- [x] T037 [P] [US2] Install Monaco Editor package (`@monaco-editor/react`) in `ui/package.json`
- [x] T038 [P] [US2] Install TanStack Table package (`@tanstack/react-table`) in `ui/package.json`
- [x] T039 [US2] Create SQL Studio page in `ui/src/pages/SqlStudio.tsx`
- [x] T040 [US2] Create Monaco Editor wrapper component in `ui/src/components/sql-studio/Editor.tsx`
- [x] T041 [US2] Implement SQL syntax highlighting config in `ui/src/components/sql-studio/sql-language.ts`
- [x] T042 [US2] Create autocomplete provider (fetches schema via SQL) in `ui/src/components/sql-studio/Autocomplete.ts`
- [x] T043 [US2] Create query execution hook in `ui/src/hooks/useQueryExecution.ts`
- [x] T044 [US2] Create results data grid component in `ui/src/components/sql-studio/Results.tsx`
- [x] T045 [US2] Implement sorting and filtering in results grid in `ui/src/components/sql-studio/Results.tsx`
- [x] T046 [US2] Implement pagination for large result sets in `ui/src/components/sql-studio/Results.tsx`
- [x] T047 [US2] Add 10,000 row limit warning display in `ui/src/components/sql-studio/Results.tsx`
- [x] T048 [US2] Add query execution timer and cancel button in `ui/src/components/sql-studio/Editor.tsx`
- [x] T049 [US2] Display query errors clearly in `ui/src/components/sql-studio/ErrorDisplay.tsx`

**Checkpoint**: User Story 2 complete - SQL Studio fully functional

---

## Phase 5: User Story 3 - User Management (Priority: P2)

**Goal**: Administrators can view, create, edit, and delete users through the admin UI using SQL queries.

**Independent Test**: Navigate to Users → see user list → create new user → edit role → delete user

### Implementation for User Story 3

- [x] T050 [P] [US3] Install shadcn Table component in `ui/src/components/ui/table.tsx`
- [x] T051 [P] [US3] Install shadcn Dialog component in `ui/src/components/ui/dialog.tsx`
- [x] T052 [P] [US3] Install shadcn Form component in `ui/src/components/ui/form.tsx`
- [x] T053 [US3] Create Users page in `ui/src/pages/Users.tsx`
- [x] T054 [US3] Create users list component with data fetching in `ui/src/components/users/UsersList.tsx`
- [x] T055 [US3] Create user form (create/edit) dialog in `ui/src/components/users/UserForm.tsx`
- [x] T056 [US3] Implement user CRUD via SQL queries in `ui/src/hooks/useUsers.ts`
- [x] T057 [US3] Add delete confirmation dialog in `ui/src/components/users/DeleteUserDialog.tsx`
- [x] T058 [US3] Add self-deletion prevention check in `ui/src/hooks/useUsers.ts`
- [x] T059 [US3] Add search and filter functionality in `ui/src/components/users/UsersList.tsx`

**Checkpoint**: User Story 3 complete - User management fully functional

---

## Phase 6: User Story 4 - Storage Management (Priority: P3)

**Goal**: Administrators can view storage configurations and usage statistics.

**Independent Test**: Navigate to Storages → see storage list → click storage → see details

### Implementation for User Story 4

- [x] T060 [US4] Create Storages page in `ui/src/pages/Storages.tsx`
- [x] T061 [US4] Create storage list component in `ui/src/components/storages/StorageList.tsx`
- [x] T062 [US4] Create storage detail view in `ui/src/components/storages/StorageDetail.tsx`
- [x] T063 [US4] Implement storage data fetching via SQL in `ui/src/hooks/useStorages.ts`

**Checkpoint**: User Story 4 complete - Storage viewing fully functional

---

## Phase 7: User Story 5 - Namespace Management (Priority: P3)

**Goal**: Administrators can view namespaces, see tables within them, and create new namespaces.

**Independent Test**: Navigate to Namespaces → see list → click namespace → see tables → create new namespace

### Implementation for User Story 5

- [x] T064 [US5] Create Namespaces page in `ui/src/pages/Namespaces.tsx`
- [x] T065 [US5] Create namespace list component in `ui/src/components/namespaces/NamespaceList.tsx`
- [x] T066 [US5] Create namespace detail view with tables in `ui/src/components/namespaces/NamespaceDetail.tsx`
- [x] T067 [US5] Create namespace form dialog in `ui/src/components/namespaces/NamespaceForm.tsx`
- [x] T068 [US5] Implement namespace operations via SQL in `ui/src/hooks/useNamespaces.ts`

**Checkpoint**: User Story 5 complete - Namespace management fully functional

---

## Phase 8: User Story 6 - Storage Browser (Priority: P3)

**Goal**: Administrators can browse files and folders within storages to inspect data files.

**Independent Test**: Select storage → navigate folders → view file metadata → use breadcrumbs

### Implementation for User Story 6

- [x] T069 [US6] Create storage browser component in `ui/src/components/storages/StorageBrowser.tsx`
- [x] T070 [US6] Create folder/file list view in `ui/src/components/storages/FileList.tsx`
- [x] T071 [US6] Create breadcrumb navigation in `ui/src/components/storages/Breadcrumbs.tsx`
- [x] T072 [US6] Implement file browsing via SQL (if system.storage_files available) in `ui/src/hooks/useStorageBrowser.ts`
- [x] T073 [US6] Add file metadata display (size, date) in `ui/src/components/storages/FileList.tsx`

**Checkpoint**: User Story 6 complete - Storage browsing fully functional

---

## Phase 9: User Story 7 - Settings View (Priority: P4)

**Goal**: Administrators can view database configuration settings organized by category.

**Independent Test**: Navigate to Settings → see settings grouped by category → see values and descriptions

### Implementation for User Story 7

- [x] T074 [US7] Create Settings page in `ui/src/pages/Settings.tsx`
- [x] T075 [US7] Create settings display component in `ui/src/components/settings/SettingsView.tsx`
- [x] T076 [US7] Implement settings fetching via SQL in `ui/src/hooks/useSettings.ts`
- [x] T077 [US7] Group settings by category in display in `ui/src/components/settings/SettingsView.tsx`

**Checkpoint**: User Story 7 complete - Settings viewing fully functional

---

## Phase 10: Polish & Cross-Cutting Concerns

**Purpose**: Improvements that affect multiple user stories

- [x] T078 [P] Add loading states and skeletons across all pages
- [x] T079 [P] Add error boundary component in `ui/src/components/ErrorBoundary.tsx`
- [x] T080 [P] Add toast notifications for success/error in `ui/src/components/ui/toast.tsx`
- [x] T081 Build production frontend bundle with `pnpm build` in `ui/`
- [x] T082 Configure backend to serve built UI from `ui/dist/` in `backend/crates/kalamdb-api/src/routes.rs`
- [x] T083 [P] Update `ui/README.md` with development instructions
- [x] T084 Run quickstart.md validation - verify login flow works end-to-end
- [x] T085 Create backend integration test for auth endpoints in `backend/tests/test_auth_api.rs`

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS all user stories
- **User Stories (Phase 3-9)**: All depend on Foundational phase completion
  - User stories can proceed in parallel (if staffed) or sequentially in priority order
- **Polish (Phase 10)**: Depends on all desired user stories being complete

### User Story Dependencies

| Story | Priority | Dependencies | Can Start After |
|-------|----------|--------------|-----------------|
| US1: Auth | P1 | None | Phase 2 |
| US2: SQL Studio | P2 | US1 (login required) | Phase 3 |
| US3: Users | P2 | US1 (login required) | Phase 3 |
| US4: Storages | P3 | US1 (login required) | Phase 3 |
| US5: Namespaces | P3 | US1 (login required) | Phase 3 |
| US6: Storage Browser | P3 | US4 (builds on storage view) | Phase 6 |
| US7: Settings | P4 | US1 (login required) | Phase 3 |

### Within Each User Story

- Components before integration
- Hooks before UI that uses them
- Core functionality before polish (sorting, filtering, etc.)

### Parallel Opportunities

- All Phase 1 Setup tasks marked [P] can run in parallel
- All Phase 2 Backend auth tasks run sequentially; Frontend tasks marked [P] can run in parallel
- Once Phase 2 completes: US2, US3, US4, US5, US7 can all start in parallel
- Within each story: tasks marked [P] can run in parallel

---

## Parallel Example: Phase 2 Frontend

```bash
# These can run in parallel (different files):
T019: Create API client in ui/src/lib/api.ts
T020: Create auth context in ui/src/lib/auth.tsx  
T021: Install shadcn Button in ui/src/components/ui/button.tsx
T022: Install shadcn Input in ui/src/components/ui/input.tsx
T023: Install shadcn Card in ui/src/components/ui/card.tsx
```

## Parallel Example: User Stories After US1

```bash
# These can start in parallel once US1 is complete:
Developer A: Phase 4 - SQL Studio (US2)
Developer B: Phase 5 - User Management (US3)
Developer C: Phase 6 - Storage Management (US4)
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup
2. Complete Phase 2: Foundational (CRITICAL - blocks all stories)
3. Complete Phase 3: User Story 1 - Auth
4. **STOP and VALIDATE**: Test login/logout flow independently
5. Deploy/demo if ready - basic auth is functional

### Incremental Delivery

1. Setup + Foundational → Foundation ready
2. Add US1 (Auth) → Test independently → **MVP Ready!**
3. Add US2 (SQL Studio) → Primary admin functionality
4. Add US3 (Users) → User management capability
5. Add US4-7 (Storages, Namespaces, Browser, Settings) → Full admin suite

### Suggested MVP Scope

**MVP = Phase 1 + Phase 2 + Phase 3 (User Story 1)**

This delivers:
- ✅ Login with username/password
- ✅ JWT token in HttpOnly cookie
- ✅ Token refresh before expiration
- ✅ Logout functionality
- ✅ Role-based access (dba/system only)
- ✅ Basic dashboard after login

---

## Summary

| Metric | Count |
|--------|-------|
| Total Tasks | 85 |
| Phase 1 (Setup) | 7 |
| Phase 2 (Foundational) | 20 |
| User Story 1 (Auth) | 9 |
| User Story 2 (SQL Studio) | 13 |
| User Story 3 (Users) | 10 |
| User Story 4 (Storages) | 4 |
| User Story 5 (Namespaces) | 5 |
| User Story 6 (Browser) | 5 |
| User Story 7 (Settings) | 4 |
| Phase 10 (Polish) | 8 |
| Parallel Opportunities | 25+ tasks marked [P] |

---

## Notes

- All data operations (users, namespaces, storages, settings) use SQL API - no dedicated REST endpoints
- Only new backend endpoints: auth (login/logout/refresh/me)
- Frontend uses TanStack Query for data fetching and caching
- Monaco Editor provides SQL editing with syntax highlighting
- HttpOnly cookies for XSS-resistant token storage
