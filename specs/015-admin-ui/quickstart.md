# Quickstart: Admin UI with Token-Based Authentication

**Feature**: 015-admin-ui  
**Date**: December 3, 2025

## Prerequisites

- Rust 1.90+ (backend)
- Node.js 20+ (frontend build)
- pnpm (frontend package manager)
- KalamDB backend running

## Development Setup

### 1. Backend Setup

The backend already exists. Add new auth and admin API endpoints:

```bash
cd backend
cargo build
```

### 2. Frontend Setup

```bash
# Create the UI directory
cd ui

# Install dependencies
pnpm install

# Start dev server (proxies API to localhost:3000)
pnpm dev
```

The dev server runs at `http://localhost:5173` with API proxy to the backend.

### 3. Running Together

Terminal 1 - Backend:
```bash
cd backend
cargo run
# Runs on http://localhost:3000
```

Terminal 2 - Frontend:
```bash
cd ui
pnpm dev
# Runs on http://localhost:5173, proxies /v1/* to backend
```

## Quick Verification

### 1. Login Flow

```bash
# Login (creates HttpOnly cookie)
curl -X POST http://localhost:3000/v1/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username": "admin", "password": "admin123"}' \
  -c cookies.txt

# Use cookie for authenticated request
curl http://localhost:3000/v1/api/auth/me \
  -b cookies.txt
```

### 2. Admin API

```bash
# List users
curl http://localhost:3000/v1/api/admin/users \
  -b cookies.txt

# List namespaces
curl http://localhost:3000/v1/api/admin/namespaces \
  -b cookies.txt

# Get schema for autocomplete
curl http://localhost:3000/v1/api/admin/schema/tables \
  -b cookies.txt
```

### 3. SQL Execution

```bash
# Execute a query
curl -X POST http://localhost:3000/v1/api/sql \
  -H "Content-Type: application/json" \
  -d '{"sql": "SELECT * FROM system.users LIMIT 10"}' \
  -b cookies.txt
```

## Building for Production

```bash
# Build frontend
cd ui
pnpm build
# Output: ui/dist/

# Build backend (serves UI from /ui route)
cd backend
cargo build --release
```

## Environment Variables

### Backend

| Variable | Default | Description |
|----------|---------|-------------|
| `KALAMDB_JWT_SECRET` | (required) | Secret for JWT signing |
| `KALAMDB_JWT_EXPIRY_HOURS` | 24 | Token expiration in hours |
| `KALAMDB_COOKIE_SECURE` | false | Set to true in production (HTTPS) |

### Frontend (Build-time)

| Variable | Default | Description |
|----------|---------|-------------|
| `VITE_API_URL` | /v1 | API base URL (relative in production) |

## Default Admin User

For development, ensure a system or dba user exists:

```sql
-- Create via SQL if needed
INSERT INTO system.users (username, password_hash, role)
VALUES ('admin', '$2b$12$...', 'system');
```

Or use the CLI:
```bash
kalamdb user create --username admin --password admin123 --role system
```

## Accessing the UI

- Development: `http://localhost:5173`
- Production: `http://localhost:3000/ui`

Login with a user that has `dba` or `system` role.
