# Implementation Plan: Admin UI with Token-Based Authentication

**Branch**: `015-admin-ui` | **Date**: December 3, 2025 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/015-admin-ui/spec.md`

## Summary

Build a React-based Admin UI served at `/ui` with JWT token-based authentication (HttpOnly cookies), SQL Studio with autocomplete, and management interfaces for users, storages, namespaces, and settings. The UI targets dba/system roles only and integrates with existing KalamDB backend APIs.

## Technical Context

**Language/Version**: Rust 1.90+ (backend) + TypeScript 5.x / React 18 (frontend)  
**Primary Dependencies**: 
- Backend: Actix-Web 4, jsonwebtoken 9.2, kalamdb-auth (existing JWT/bcrypt)
- Frontend: React 18, shadcn/ui, Tailwind CSS, Monaco Editor (SQL), TanStack Query
**Storage**: Existing RocksDB + Parquet (no new storage needed)  
**Testing**: cargo test (backend), Vitest + React Testing Library (frontend)  
**Target Platform**: Web browser (served from KalamDB backend on `/ui`)
**Project Type**: Web application (backend + frontend)  
**Performance Goals**: <500ms autocomplete, <2s query results for 1000 rows, <3s login  
**Constraints**: 10,000 row max per query, 30s query timeout, HttpOnly cookies for XSS protection  
**Scale/Scope**: Admin users only (dba, system roles), single-tenant UI

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| Library-First | ✅ PASS | Frontend is standalone SPA; backend auth extensions in kalamdb-auth |
| Test-First | ✅ PASS | Contract tests for API, component tests for UI |
| Integration Testing | ✅ PASS | Auth flow, SQL execution, management CRUD |
| Observability | ✅ PASS | Existing logging/metrics infrastructure |
| Simplicity | ✅ PASS | Leverages existing auth, schema registry, SQL execution |

**Gate Result**: PASS - No violations requiring justification

## Project Structure

### Documentation (this feature)

```text
specs/015-admin-ui/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output (OpenAPI specs)
└── tasks.md             # Phase 2 output (/speckit.tasks command)
```

### Source Code (repository root)

```text
backend/
├── crates/
│   ├── kalamdb-api/src/
│   │   ├── handlers/
│   │   │   └── auth.rs          # NEW: Login/logout/refresh endpoints
│   │   └── routes.rs            # MODIFY: Add /ui and auth routes
│   └── kalamdb-auth/src/
│       ├── jwt_auth.rs          # MODIFY: Add token generation, refresh
│       └── cookie.rs            # NEW: HttpOnly cookie handling
└── tests/
    └── test_auth_api.rs         # NEW: Auth API integration tests

# NOTE: ALL data operations use existing SQL API
# - Users: SELECT/INSERT/UPDATE/DELETE FROM system.users
# - Namespaces: CREATE/DROP NAMESPACE, SELECT FROM system.namespaces  
# - Storages: SELECT FROM system.storages
# - Schema: SELECT FROM information_schema.tables/columns

ui/                              # NEW: React frontend
├── package.json
├── tsconfig.json
├── vite.config.ts
├── tailwind.config.ts
├── components.json              # shadcn/ui config
├── src/
│   ├── main.tsx
│   ├── App.tsx
│   ├── lib/
│   │   ├── api.ts              # API client with auth
│   │   └── auth.ts             # Auth context/hooks
│   ├── components/
│   │   ├── ui/                 # shadcn components
│   │   ├── layout/
│   │   │   ├── Sidebar.tsx
│   │   │   ├── Header.tsx
│   │   │   └── Layout.tsx
│   │   ├── sql-studio/
│   │   │   ├── Editor.tsx      # Monaco editor
│   │   │   ├── Results.tsx     # Data grid
│   │   │   └── Autocomplete.ts # Autocomplete provider
│   │   ├── users/
│   │   ├── storages/
│   │   ├── namespaces/
│   │   └── settings/
│   └── pages/
│       ├── Login.tsx
│       ├── Dashboard.tsx
│       ├── SqlStudio.tsx
│       ├── Users.tsx
│       ├── Storages.tsx
│       ├── Namespaces.tsx
│       └── Settings.tsx
└── tests/
    ├── setup.ts
    └── components/

```

**Structure Decision**: Web application structure with `ui/` directory at repo root for the React SPA, backend extensions in existing `backend/crates/`. Frontend builds to static files served by Actix-Web.

## Complexity Tracking

> No constitution violations - section not needed.
