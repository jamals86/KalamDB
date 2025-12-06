# Research: Admin UI with Token-Based Authentication

**Feature**: 015-admin-ui  
**Date**: December 3, 2025  
**Status**: Complete

## Research Topics

### 1. JWT Token Generation & Refresh Strategy

**Decision**: Use short-lived access tokens (24h default) with HttpOnly cookie storage and silent refresh

**Rationale**: 
- HttpOnly cookies prevent XSS attacks (tokens not accessible via JavaScript)
- Silent refresh before expiration provides seamless UX
- Existing `kalamdb-auth` already has JWT validation; needs token generation endpoint
- jsonwebtoken crate already in use supports HS256 for generation

**Alternatives Considered**:
- localStorage: Rejected due to XSS vulnerability
- Long-lived tokens (7 days): Rejected due to security risk
- OAuth2 refresh tokens: Overkill for single-server deployment

**Implementation Notes**:
- Add `POST /v1/api/auth/login` → Returns HttpOnly cookie with JWT
- Add `POST /v1/api/auth/refresh` → Refreshes token before expiration
- Add `POST /v1/api/auth/logout` → Clears HttpOnly cookie
- Cookie attributes: `HttpOnly`, `Secure` (production), `SameSite=Strict`, `Path=/`

### 2. SQL Editor with Autocomplete

**Decision**: Use Monaco Editor with custom SQL language provider

**Rationale**:
- Monaco is the VS Code editor engine, battle-tested for code editing
- Native SQL language support with syntax highlighting
- Custom CompletionItemProvider for table/column autocomplete
- React integration via `@monaco-editor/react`

**Alternatives Considered**:
- CodeMirror 6: Good option but Monaco has better TypeScript types and SQL support
- Custom textarea: Too much work for proper syntax highlighting
- Ace Editor: Older, less active development

**Implementation Notes**:
- Fetch schema metadata from `/v1/api/admin/schema/tables`
- Cache schema in React Query for 60s
- Provide completions for: keywords, tables, columns (context-aware)

### 3. Data Grid for Query Results

**Decision**: Use TanStack Table with virtualization for large datasets

**Rationale**:
- Headless UI - works with shadcn/ui styling
- Built-in sorting, filtering, pagination
- @tanstack/react-virtual for 10,000 row performance
- TypeScript-first API

**Alternatives Considered**:
- AG Grid: Excellent but heavy (100KB+), overkill for admin UI
- react-table v7: Deprecated in favor of TanStack Table
- Custom implementation: Too much work

**Implementation Notes**:
- Virtual scrolling for rows beyond visible viewport
- Column resizing enabled
- Export to CSV/JSON for results

### 4. React + shadcn/ui Stack

**Decision**: Vite + React 18 + shadcn/ui + Tailwind CSS + TanStack Query

**Rationale**:
- Vite: Fast dev server, optimized production builds
- shadcn/ui: Copy-paste components, full control, Radix primitives
- Tailwind CSS: Utility-first, consistent with shadcn
- TanStack Query: Data fetching, caching, auto-refresh

**Alternatives Considered**:
- Next.js: SSR not needed, adds complexity
- Material UI: Heavier, opinionated styling
- Redux: TanStack Query handles server state better

**Implementation Notes**:
- Build outputs to `ui/dist/` 
- Actix-Web serves static files from `/ui` route
- Use shadcn `sidebar` layout component for navigation

### 5. Backend Static File Serving

**Decision**: Serve UI as static files from Actix-Web with SPA fallback

**Rationale**:
- Single deployment (no separate frontend server)
- Actix-web `Files` service handles static assets
- Fallback to `index.html` for client-side routing

**Alternatives Considered**:
- Nginx reverse proxy: Adds deployment complexity
- Separate frontend server: More infrastructure to manage
- Embedded assets in binary: Larger binary, harder to update

**Implementation Notes**:
- Build UI → `ui/dist/`
- Copy to `backend/static/ui/` during build or configure Actix to serve from `ui/dist/`
- Route: `GET /ui/*` → static files, fallback to `index.html`

### 6. Admin Data Operations

**Decision**: Use existing SQL API for ALL data operations including schema metadata

**Rationale**:
- SQL API already exists and handles auth, permissions, and data operations
- No need for any dedicated admin REST endpoints beyond auth
- Simpler implementation - only auth endpoints are new
- UI executes SQL queries directly for everything
- Schema for autocomplete: `SELECT * FROM information_schema.tables/columns`

**Alternatives Considered**:
- Dedicated REST endpoints for each entity: Rejected - duplicates SQL functionality
- Dedicated schema endpoint: Rejected - information_schema provides same data via SQL

**Implementation Notes**:
- User management: `SELECT/INSERT/UPDATE/DELETE FROM system.users`
- Namespace management: `CREATE NAMESPACE` / `DROP NAMESPACE` / `SELECT FROM system.namespaces`
- Storage viewing: `SELECT FROM system.storages`
- Settings viewing: `SELECT FROM system.settings` or config query
- Schema autocomplete: `SELECT * FROM information_schema.tables WHERE table_schema = ?`
- Column autocomplete: `SELECT * FROM information_schema.columns WHERE table_name = ?`

## Summary

All research topics resolved. No NEEDS CLARIFICATION items remain.

**Key Technology Choices**:
- JWT in HttpOnly cookies (Actix-Web + cookie crate)
- Monaco Editor for SQL editing
- TanStack Table with virtualization for results
- Vite + React 18 + shadcn/ui + Tailwind CSS
- Static file serving from Actix-Web
- **SQL API for ALL data operations** - no new REST endpoints except auth
- Only new endpoints: auth (login/logout/refresh/me)
- Schema autocomplete via SQL: `SELECT * FROM information_schema.tables/columns`
