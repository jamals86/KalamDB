# Admin UI V2 Design (Embedded-First, SaaS-Ready)

Date: 2026-02-10
Status: Approved (brainstorming validation complete)
Scope: New standalone admin UI app replacing legacy UI over time

## 1. Goals and Non-Goals

### Goals
- Build a completely new admin UI with modern UX/UI using shadcn-based primitives.
- Prioritize embedded/on-prem usage first, while keeping architecture extensible for cloud SaaS.
- Use React best practices with predictable state management through Redux Toolkit and RTK Query.
- Deliver an informative operator dashboard as the default landing page.
- Deliver an analyst-first SQL Studio inspired by data.world + Supabase + Neon.
- Support realtime header notifications through `kalam-link` subscription to `private.notification`.

### Non-Goals (MVP)
- Full parity with every legacy page in the first release.
- Billing/multi-org SaaS product modules in MVP.
- Rewriting backend APIs purely for UI preferences.

## 2. Product Decisions (Validated)

- Delivery model: Embedded-first now, SaaS extensibility by design.
- Rewrite strategy: Full new admin UI app (not incremental refactor of current `ui`).
- Stack: Next.js (App Router) + shadcn/ui + Redux Toolkit + RTK Query.
- State observability: Redux DevTools support for Chrome extension workflows.
- Auth model: Backend-auth passthrough (KalamDB auth endpoints directly).
- MVP scope: Login, Dashboard, SQL Studio, Namespaces/Tables explorer, Jobs, Settings.
- Visual direction: Industrial clean (dense, high signal, operator-friendly).
- Dashboard focus: Reliability-first telemetry and operational actionability.
- SQL Studio default mode: Analyst-first.

## 3. App Architecture

## 3.1 Repository Layout

Create a new standalone app at:
- `/Users/jamal/git/KalamDB/admin-ui`

Proposed structure:

- `admin-ui/src/app/`
  - App Router segments (`(auth)`, `(protected)`, `sql-studio`, `jobs`, `settings`, etc.)
  - Route layouts and route-level loading/error boundaries
- `admin-ui/src/features/`
  - Feature-first modules with co-located slice, RTK Query endpoints, selectors, components, tests
- `admin-ui/src/components/ui/`
  - shadcn primitive components only
- `admin-ui/src/components/shared/`
  - Composed cross-feature blocks (shell, headers, stat panels, status banners)
- `admin-ui/src/store/`
  - `configureStore`, typed hooks, middleware, listener setup
- `admin-ui/src/lib/`
  - API clients, auth token utilities, shared formatters, constants
- `admin-ui/src/styles/`
  - design tokens, theme variables, global CSS

## 3.2 Layering Rules

- `features/*` may depend on `components/ui`, `components/shared`, `lib`, `store`.
- `components/ui` remains domain-agnostic.
- Avoid global mutable modules; all app state flows via Redux/RTK Query.
- Realtime subscriptions are managed in listener middleware, not in random UI components.

## 4. State Management Strategy

## 4.1 Server State

Use RTK Query for:
- Dashboard metrics and health feeds
- Jobs lists and actions
- Namespace/table metadata
- SQL execution endpoints and schema introspection
- Notifications initial list and read/update actions

## 4.2 Client/UI State

Use Redux slices for:
- SQL workspace tabs and active editor state
- SQL panel layout and preferences
- Sidebar filter/search values
- Favorites and query-history UI interactions
- Notification stream status and unread badge

Use local component state only for strictly ephemeral concerns (hover, open popover anchor refs, uncontrolled inputs before submit).

## 4.3 DevTools and Diagnostics

- Enable Redux DevTools in non-production builds.
- Keep actions serializable and informative (`sql/runStarted`, `notifications/eventReceived`, etc.).
- Include lightweight event metadata for debugging subscription behavior.

## 5. UX and Information Architecture

## 5.1 App Shell

Protected shell includes:
- Left primary navigation
- Top header (global search, notifications, profile/session)
- Main content panel with page-level actions

Default post-login route:
- Informative reliability dashboard

## 5.2 Dashboard (Reliability-First)

Top layer:
- Service health strip (API, query engine, storage, websocket/subscriptions)
- Critical work queue (failed/retrying jobs, urgent operational items)
- Pressure indicators (live query load, ingest pressure, storage pressure)
- Quick actions (open SQL template, retry failed category, jump to filtered logs)

Second layer:
- Error/retry trend charts
- Queue latency and job throughput
- Incident timeline / recent alerts
- Top noisy entities

Principles:
- High signal density
- Clear status semantics
- Actionable modules, not passive cards

## 6. Header Notifications (Realtime)

## 6.1 Data Source

Use `kalam-link` subscription to:
- `private.notification` user table

## 6.2 Notification Feature Module

`src/features/notifications/`:
- RTK Query for initial fetch + mark read operations
- Slice for in-memory notification entities and unread count
- Listener middleware for stream lifecycle tied to authenticated session

## 6.3 Behavior

- Connect on session ready
- Reconnect with exponential backoff
- Deduplicate by notification id
- Bounded in-memory list (e.g., latest 200)
- Catch-up fetch on reconnect using `lastEventAt`
- Visible state in header (`Live`, `Reconnecting`, `Offline`)

Fallback:
- Optional polling mode if subscription is unavailable

## 7. SQL Studio Detailed Design (Analyst-First)

## 7.1 Layout

Three-pane workspace:
- Left aside:
  - Explorer tree: `database -> namespaces -> tables -> columns`
  - Tree filter/search
  - Favorites section for saved queries
- Center:
  - Persistent SQL tabs (workspace model)
  - Monaco editor with schema-aware autocomplete
  - Results grid and execution status/log
- Right slide panel:
  - Create table and alter table workflows
  - Column add/edit/remove operations
  - Table metadata and DDL helpers

## 7.2 Tabs and Favorites

- Persistent tab restore
- Dirty state indicators
- Duplicate/close/reopen behavior
- Favorites + recents + pinned templates

## 7.3 Live Query Support

Per-tab live toggle with explicit status:
- `connecting`, `live`, `reconnecting`, `offline`

Realtime behaviors:
- Row-level update highlights
- Activity indicators in result panel

## 7.4 Results Grid and Editing

Required interactions:
- Datatype-aware columns and cell rendering
- Multi-row selection
- Right-click cell context menu: view, copy, edit, delete row
- Pending change indicators per row/cell
- Global pending toolbar with Commit/Discard
- Commit opens SQL preview then applies mutations

Conflict rule (safety-first):
- If a live update targets rows with local uncommitted edits, freeze row edits and surface conflict banner.

## 8. Data Flow Overview

## 8.1 Query Execution Flow

1. User executes SQL from active tab.
2. `sqlWorkspaceSlice` updates run state.
3. RTK Query mutation calls backend SQL endpoint.
4. Response normalized into per-tab result model.
5. UI updates grid, status bar, execution metrics.

## 8.2 Live Query Flow

1. User enables live mode per tab.
2. Listener opens subscription via `kalam-link`.
3. Events map to tab result dataset with stable row identity.
4. UI highlights changes and updates counts/status.
5. Disconnect/reconnect handled by middleware with catch-up sync.

## 8.3 Table Edit Flow

1. User edits cells/deletes rows in grid.
2. Pending changes tracked in Redux slice.
3. Commit opens SQL preview drawer/modal.
4. User confirms; batch SQL applies.
5. On success, pending state cleared and grid refreshed.

## 9. Error Handling and Resilience

- Route-level error boundaries in Next.js app router.
- Feature-level error states with recover actions.
- Distinguish transport errors vs SQL execution errors vs permission errors.
- Preserve editor/tab state after recoverable failures.
- Realtime disconnect should degrade gracefully with explicit status and retry.
- Avoid destructive silent state resets; preserve user work-in-progress.

## 10. Testing Strategy

## 10.1 Unit Tests

- Reducer/action tests for each slice
- Selector tests for computed state
- Utility tests for SQL workspace and formatting helpers

## 10.2 Integration Tests

- RTK Query endpoint integration (mock service worker or API mocks)
- Notification listener lifecycle tests (connect/reconnect/dedup)
- SQL Studio state transitions across run/live/edit/commit

## 10.3 E2E Tests

- Login and protected route access
- Dashboard informative widgets and navigation actions
- SQL Studio tab persistence and execution
- Realtime notification badge updates
- Commit/discard table edits and conflict banner behavior

## 10.4 Quality Gates

- TypeScript strict mode
- ESLint + formatting checks
- CI with unit + integration + critical e2e path coverage

## 11. Phased Delivery

Phase 1:
- New app scaffold, auth shell, design tokens, nav, header
- Dashboard v1 + notifications stream

Phase 2:
- SQL Studio core (tabs, explorer tree, execution, results)

Phase 3:
- SQL Studio advanced (favorites, live mode, right-drawer DDL, edit/commit/discard)

Phase 4:
- Jobs, settings, namespace/table management pages
- hardening and migration plan from legacy UI

## 12. Migration Approach

- Run legacy `ui` and new `admin-ui` in parallel during rollout.
- Validate API compatibility and required backend gaps.
- Cut over embedded entrypoint to new UI when MVP quality bar is met.
- Keep legacy UI only as fallback until parity threshold is achieved.

## 13. Open Items (Post-MVP)

- SaaS multi-tenant workspace switcher and org-level RBAC surfaces
- Shared query collections and collaboration model
- Advanced observability dashboards and custom saved views
- Billing and usage controls for cloud productization
