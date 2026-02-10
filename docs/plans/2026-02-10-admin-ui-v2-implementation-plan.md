# Admin UI V2 (Next.js + Redux) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a new standalone `admin-ui` app (Next.js + shadcn + Redux Toolkit) for embedded-first operations with SaaS-ready structure.

**Architecture:** Create a separate Next.js app at `admin-ui/` with App Router, feature-first modules, RTK Query for server state, Redux slices for UI/workspace state, and listener middleware for realtime notifications. Migrate high-value workflows first: auth shell, reliability dashboard, and analyst-first SQL Studio.

**Tech Stack:** Next.js App Router, React 19, TypeScript, Tailwind CSS, shadcn/ui, Redux Toolkit, RTK Query, Monaco Editor, kalam-link.

---

### Task 1: Scaffold New App

**Files:**
- Create: `admin-ui/` (new app root)
- Create: `admin-ui/package.json`
- Create: `admin-ui/next.config.ts`
- Create: `admin-ui/tsconfig.json`
- Create: `admin-ui/.eslintrc.cjs`
- Create: `admin-ui/.prettierrc`
- Create: `admin-ui/src/app/layout.tsx`
- Create: `admin-ui/src/app/globals.css`
- Create: `admin-ui/src/app/page.tsx`

**Step 1: Write failing smoke test for app boot**

Create `admin-ui/src/app/__tests__/boot.test.tsx` asserting root renders app shell placeholder.

**Step 2: Run test to verify it fails**

Run: `cd admin-ui && npm test -- --run src/app/__tests__/boot.test.tsx`
Expected: FAIL because app not scaffolded.

**Step 3: Scaffold Next.js app with strict TypeScript and test setup**

Use `create-next-app` non-interactive settings and wire test config.

**Step 4: Re-run test to verify pass**

Run: `cd admin-ui && npm test -- --run src/app/__tests__/boot.test.tsx`
Expected: PASS.

**Step 5: Commit**

```bash
git add admin-ui
git commit -m "feat(admin-ui): scaffold next app foundation"
```

### Task 2: Install UI and State Foundation

**Files:**
- Modify: `admin-ui/package.json`
- Create: `admin-ui/components.json`
- Create: `admin-ui/src/lib/utils.ts`
- Create: `admin-ui/src/components/ui/*` (shadcn primitives)

**Step 1: Write failing test for Redux provider mount**

Create `admin-ui/src/app/__tests__/providers.test.tsx` to assert children render with store provider.

**Step 2: Run failing test**

Run: `cd admin-ui && npm test -- --run src/app/__tests__/providers.test.tsx`
Expected: FAIL due missing provider.

**Step 3: Install deps and init shadcn**

Install: `@reduxjs/toolkit react-redux @radix-ui/* class-variance-authority clsx tailwind-merge lucide-react` and initialize shadcn with Tailwind tokens.

**Step 4: Re-run test and lint**

Run:
- `cd admin-ui && npm test -- --run src/app/__tests__/providers.test.tsx`
- `cd admin-ui && npm run lint`
Expected: PASS.

**Step 5: Commit**

```bash
git add admin-ui
git commit -m "feat(admin-ui): add shadcn and redux dependencies"
```

### Task 3: Build Store, Typed Hooks, and RTK Query Base

**Files:**
- Create: `admin-ui/src/store/index.ts`
- Create: `admin-ui/src/store/hooks.ts`
- Create: `admin-ui/src/store/listener.ts`
- Create: `admin-ui/src/store/rootReducer.ts`
- Create: `admin-ui/src/store/api/baseApi.ts`
- Create: `admin-ui/src/lib/env.ts`

**Step 1: Write failing reducer test**

Create `admin-ui/src/store/__tests__/store.test.ts` verifying store initializes with expected slices and middleware.

**Step 2: Run test (fail)**

Run: `cd admin-ui && npm test -- --run src/store/__tests__/store.test.ts`
Expected: FAIL.

**Step 3: Implement store + RTK Query baseApi**

- Configure middleware with `baseApi.middleware`
- Add serializable check exceptions only where needed
- Export typed `AppDispatch`, `RootState`, hooks

**Step 4: Re-run tests**

Run: `cd admin-ui && npm test -- --run src/store/__tests__/store.test.ts`
Expected: PASS.

**Step 5: Commit**

```bash
git add admin-ui/src/store admin-ui/src/lib/env.ts
git commit -m "feat(admin-ui): add redux store and rtk query base"
```

### Task 4: Add Auth Session Feature (Backend Passthrough)

**Files:**
- Create: `admin-ui/src/features/auth/authApi.ts`
- Create: `admin-ui/src/features/auth/authSlice.ts`
- Create: `admin-ui/src/features/auth/selectors.ts`
- Create: `admin-ui/src/features/auth/components/LoginForm.tsx`
- Create: `admin-ui/src/app/(auth)/login/page.tsx`
- Create: `admin-ui/src/app/(protected)/layout.tsx`
- Create: `admin-ui/src/middleware.ts`

**Step 1: Write failing tests**

- `authSlice.test.ts`: login/logout reducers
- `login-form.test.tsx`: submit calls login mutation and routes on success

**Step 2: Run tests (fail)**

Run: `cd admin-ui && npm test -- --run src/features/auth/__tests__/*`
Expected: FAIL.

**Step 3: Implement auth flow**

- Reuse legacy endpoint contracts from `ui/src/lib/api.ts`
- Persist access token in secure cookie/session strategy
- Protect `(protected)` routes

**Step 4: Re-run tests**

Run: `cd admin-ui && npm test -- --run src/features/auth/__tests__/*`
Expected: PASS.

**Step 5: Commit**

```bash
git add admin-ui/src/features/auth admin-ui/src/app/'(auth)' admin-ui/src/app/'(protected)' admin-ui/src/middleware.ts
git commit -m "feat(admin-ui): implement backend passthrough auth"
```

### Task 5: Implement App Shell and Navigation

**Files:**
- Create: `admin-ui/src/components/shared/app-shell/AppShell.tsx`
- Create: `admin-ui/src/components/shared/app-shell/SidebarNav.tsx`
- Create: `admin-ui/src/components/shared/app-shell/Header.tsx`
- Create: `admin-ui/src/components/shared/app-shell/CommandPalette.tsx`
- Create: `admin-ui/src/config/navigation.ts`

**Step 1: Write failing component tests**

- navigation renders expected links
- header renders search/profile placeholders

**Step 2: Run tests (fail)**

Run: `cd admin-ui && npm test -- --run src/components/shared/app-shell/__tests__/*`
Expected: FAIL.

**Step 3: Implement industrial clean shell**

- Desktop and mobile layouts
- Tokenized spacing/typography in `globals.css`
- Route-aware active nav states

**Step 4: Re-run tests**

Run: `cd admin-ui && npm test -- --run src/components/shared/app-shell/__tests__/*`
Expected: PASS.

**Step 5: Commit**

```bash
git add admin-ui/src/components/shared/app-shell admin-ui/src/config/navigation.ts admin-ui/src/app/globals.css
git commit -m "feat(admin-ui): add protected app shell and nav"
```

### Task 6: Add Notifications Feature with `kalam-link` Subscription

**Files:**
- Create: `admin-ui/src/features/notifications/notificationsApi.ts`
- Create: `admin-ui/src/features/notifications/notificationsSlice.ts`
- Create: `admin-ui/src/features/notifications/notificationsListener.ts`
- Create: `admin-ui/src/features/notifications/selectors.ts`
- Create: `admin-ui/src/features/notifications/components/NotificationsBell.tsx`

**Step 1: Write failing reducer/listener tests**

- unread count updates on event
- dedup by id
- reconnect state transition

**Step 2: Run tests (fail)**

Run: `cd admin-ui && npm test -- --run src/features/notifications/__tests__/*`
Expected: FAIL.

**Step 3: Implement realtime flow**

- Subscribe to `private.notification` scoped to authenticated user
- Listener lifecycle tied to auth session
- Catch-up fetch on reconnect

**Step 4: Re-run tests**

Run: `cd admin-ui && npm test -- --run src/features/notifications/__tests__/*`
Expected: PASS.

**Step 5: Commit**

```bash
git add admin-ui/src/features/notifications admin-ui/src/components/shared/app-shell/Header.tsx
git commit -m "feat(admin-ui): add realtime header notifications"
```

### Task 7: Build Reliability Dashboard v1

**Files:**
- Create: `admin-ui/src/features/dashboard/dashboardApi.ts`
- Create: `admin-ui/src/features/dashboard/components/HealthStrip.tsx`
- Create: `admin-ui/src/features/dashboard/components/CriticalQueuePanel.tsx`
- Create: `admin-ui/src/features/dashboard/components/PressureIndicators.tsx`
- Create: `admin-ui/src/features/dashboard/components/QuickActions.tsx`
- Create: `admin-ui/src/app/(protected)/dashboard/page.tsx`

**Step 1: Write failing page test**

Verify dashboard renders the four required sections and loading states.

**Step 2: Run test (fail)**

Run: `cd admin-ui && npm test -- --run src/features/dashboard/__tests__/dashboard-page.test.tsx`
Expected: FAIL.

**Step 3: Implement dashboard with RTK Query**

Use composable cards with explicit degraded/healthy states and actionable links.

**Step 4: Re-run tests**

Run: `cd admin-ui && npm test -- --run src/features/dashboard/__tests__/dashboard-page.test.tsx`
Expected: PASS.

**Step 5: Commit**

```bash
git add admin-ui/src/features/dashboard admin-ui/src/app/'(protected)'/dashboard/page.tsx
git commit -m "feat(admin-ui): add reliability-first dashboard"
```

### Task 8: Build SQL Studio State Slices

**Files:**
- Create: `admin-ui/src/features/sql-studio/state/sqlWorkspaceSlice.ts`
- Create: `admin-ui/src/features/sql-studio/state/sqlResultsSlice.ts`
- Create: `admin-ui/src/features/sql-studio/state/sqlUiSlice.ts`
- Create: `admin-ui/src/features/sql-studio/state/selectors.ts`
- Create: `admin-ui/src/features/sql-studio/state/persistence.ts`

**Step 1: Write failing state tests**

- tab create/close/restore
- active tab switching
- dirty-state and persistence behavior

**Step 2: Run tests (fail)**

Run: `cd admin-ui && npm test -- --run src/features/sql-studio/state/__tests__/*`
Expected: FAIL.

**Step 3: Implement slices and local persistence**

Persist per user workspace key with migrations for schema changes.

**Step 4: Re-run tests**

Run: `cd admin-ui && npm test -- --run src/features/sql-studio/state/__tests__/*`
Expected: PASS.

**Step 5: Commit**

```bash
git add admin-ui/src/features/sql-studio/state
git commit -m "feat(admin-ui): add sql studio workspace state slices"
```

### Task 9: Build SQL Studio Core Layout and Editor

**Files:**
- Create: `admin-ui/src/app/(protected)/sql-studio/page.tsx`
- Create: `admin-ui/src/features/sql-studio/components/SqlStudioLayout.tsx`
- Create: `admin-ui/src/features/sql-studio/components/QueryTabBar.tsx`
- Create: `admin-ui/src/features/sql-studio/components/ExplorerTree.tsx`
- Create: `admin-ui/src/features/sql-studio/components/FavoritesPanel.tsx`
- Create: `admin-ui/src/features/sql-studio/components/SqlEditorPanel.tsx`

**Step 1: Write failing component tests**

- explorer renders tree and filter input
- tab bar supports add/close/restore

**Step 2: Run tests (fail)**

Run: `cd admin-ui && npm test -- --run src/features/sql-studio/components/__tests__/*`
Expected: FAIL.

**Step 3: Implement analyst-first core UX**

- Left: Explorer + Favorites
- Center: tabs + monaco editor
- command shortcuts for run actions

**Step 4: Re-run tests**

Run: `cd admin-ui && npm test -- --run src/features/sql-studio/components/__tests__/*`
Expected: PASS.

**Step 5: Commit**

```bash
git add admin-ui/src/features/sql-studio/components admin-ui/src/app/'(protected)'/sql-studio/page.tsx
git commit -m "feat(admin-ui): add sql studio core layout"
```

### Task 10: Add SQL Execution and Results Grid

**Files:**
- Create: `admin-ui/src/features/sql-studio/api/sqlApi.ts`
- Create: `admin-ui/src/features/sql-studio/components/ResultsGrid.tsx`
- Create: `admin-ui/src/features/sql-studio/components/ExecutionStatusBar.tsx`
- Create: `admin-ui/src/features/sql-studio/components/CellContextMenu.tsx`

**Step 1: Write failing integration tests**

- run query updates result model
- results render datatype-aware headers and values

**Step 2: Run tests (fail)**

Run: `cd admin-ui && npm test -- --run src/features/sql-studio/__tests__/execution.test.tsx`
Expected: FAIL.

**Step 3: Implement execution pipeline**

- Reuse backend SQL contracts from legacy `ui/src/lib/kalam-client.ts` / `ui/src/lib/api.ts`
- Include row count, duration, execution identity

**Step 4: Re-run tests**

Run: `cd admin-ui && npm test -- --run src/features/sql-studio/__tests__/execution.test.tsx`
Expected: PASS.

**Step 5: Commit**

```bash
git add admin-ui/src/features/sql-studio/api admin-ui/src/features/sql-studio/components
git commit -m "feat(admin-ui): add sql execution and results grid"
```

### Task 11: Add Live Queries + Edit/Commit/Discard + Right Drawer DDL

**Files:**
- Create: `admin-ui/src/features/sql-studio/live/liveListener.ts`
- Create: `admin-ui/src/features/sql-studio/state/pendingChangesSlice.ts`
- Create: `admin-ui/src/features/sql-studio/components/PendingChangesToolbar.tsx`
- Create: `admin-ui/src/features/sql-studio/components/TableDesignerDrawer.tsx`
- Create: `admin-ui/src/features/sql-studio/components/SqlPreviewDialog.tsx`

**Step 1: Write failing tests**

- live status transitions and reconnect
- pending edits + discard
- commit preview confirmation flow
- conflict banner on live update touching edited row

**Step 2: Run tests (fail)**

Run: `cd admin-ui && npm test -- --run src/features/sql-studio/__tests__/live-and-editing.test.tsx`
Expected: FAIL.

**Step 3: Implement advanced interactions**

- per-tab live query toggle
- row/cell change indicators
- right drawer for create/alter table
- commit/discard with SQL preview

**Step 4: Re-run tests**

Run: `cd admin-ui && npm test -- --run src/features/sql-studio/__tests__/live-and-editing.test.tsx`
Expected: PASS.

**Step 5: Commit**

```bash
git add admin-ui/src/features/sql-studio
git commit -m "feat(admin-ui): add live mode and commit-discard editing"
```

### Task 12: Harden, Document, and Validate

**Files:**
- Create: `admin-ui/README.md`
- Create: `admin-ui/.env.example`
- Create: `admin-ui/src/app/(protected)/jobs/page.tsx`
- Create: `admin-ui/src/app/(protected)/settings/page.tsx`
- Modify: `docs/plans/2026-02-10-admin-ui-v2-design.md` (implementation status appendix)

**Step 1: Write failing smoke test for route availability**

Test protected routes load shell and key modules.

**Step 2: Run tests (fail)**

Run: `cd admin-ui && npm test -- --run src/app/__tests__/protected-routes.test.tsx`
Expected: FAIL.

**Step 3: Add route stubs, docs, and env configuration**

Define local run instructions, backend URL variables, and known caveats.

**Step 4: Full validation run**

Run:
- `cd admin-ui && npm run lint`
- `cd admin-ui && npm test`
- `cd admin-ui && npm run build`
Expected: all PASS.

**Step 5: Commit**

```bash
git add admin-ui docs/plans/2026-02-10-admin-ui-v2-design.md
git commit -m "chore(admin-ui): harden routes docs and build validation"
```

## Notes for Execution

- Prefer feature-by-feature migration without coupling to legacy `ui/` components.
- Reuse only API contract knowledge and proven interaction semantics.
- Keep design tokens centralized and avoid one-off styles.
- Preserve strict TypeScript and serializable Redux action patterns.
- Maintain keyboard-first ergonomics for SQL Studio.
