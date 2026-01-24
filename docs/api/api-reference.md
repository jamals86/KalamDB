# KalamDB REST API Reference

**Version**: 0.1.3  
**Base URL**: `http://localhost:8080`  
**API Version**: v1

## Overview

KalamDB provides a versioned REST API with a single SQL endpoint that accepts SQL commands for all operations. This unified interface simplifies client integration and enables full SQL expressiveness.

### API Versioning

All API endpoints are versioned with a `/v1` prefix to ensure backward compatibility as the API evolves.

**Current Version**: v1  
**Endpoint Prefix**: `/v1`

**Versioned Endpoints**:
- `/v1/api/sql` - Execute SQL commands (JSON or multipart)
- `/v1/files/{namespace}/{table}/{subfolder}/{file_id}` - Download files stored in FILE columns
- `/v1/ws` - WebSocket for live query subscriptions
- `/v1/api/healthcheck` - Server health status
- `/v1/api/auth/login` - Admin UI login (cookie-based)
- `/v1/api/auth/refresh` - Refresh auth cookie
- `/v1/api/auth/logout` - Clear auth cookie
- `/v1/api/auth/me` - Current user info (from cookie)

**Version Strategy**:
- **Breaking changes** require a new version (e.g., `/v2/api/sql`)
- **Non-breaking changes** (new optional fields, new endpoints) can be added to existing versions
- **Deprecation policy**: Old versions supported for at least 6 months after new version release

---

## Authentication

`POST /v1/api/sql` requires an `Authorization` header (HTTP Basic or JWT bearer).

For local development (localhost / `127.0.0.1`), the default `root` user supports an empty password, so you can use Basic auth with `root:`.

### HTTP Basic Auth
```http
Authorization: Basic <base64(username:password)>
```

### JWT Token Authentication
```http
Authorization: Bearer <JWT_TOKEN>
```

---

## Endpoints

### POST /v1/api/sql

Execute a SQL command and receive results.

**Version**: v1  
**Path**: `/v1/api/sql`

#### Request

**Headers**:
- `Content-Type: application/json` or `multipart/form-data`
- `Authorization: Basic <credentials>` or `Authorization: Bearer <token>` (required)

**Body (JSON)**:
```json
{
  "sql": "SELECT * FROM app.messages LIMIT 10",
  "params": ["optional", 123],
  "namespace_id": "optional"
}
```

**Body (multipart)**:

Fields:
- `sql` (text) — SQL statement (single statement only)
- `params` (optional text) — JSON array of positional params
- `namespace_id` (optional text)
- `file:<placeholder>` (file) — file parts mapped to `FILE("placeholder")`

Example:
```
POST /v1/api/sql
Content-Type: multipart/form-data; boundary=...

--...
Content-Disposition: form-data; name="sql"

INSERT INTO app.documents (id, name, attachment)
VALUES ('doc1', 'My Document', FILE("myfile.txt"));
--...
Content-Disposition: form-data; name="file:myfile.txt"; filename="test-attachment.txt"
Content-Type: text/plain

<file bytes>
--...--
```

Notes:
- `params` are positional and use `$1`, `$2`, … placeholders.
- If `params` is provided, the request must contain exactly one SQL statement (parameterized multi-statement batches are rejected).
- `namespace_id` is used to resolve unqualified table names.
- Multipart uploads require **one** SQL statement and are only accepted on the leader node in cluster mode.

#### Response

**Success (200 OK)**:
```json
{
  "status": "success",
  "results": [
    {
      "schema": [
        {"name": "id", "data_type": "BigInt", "index": 0},
        {"name": "content", "data_type": "Text", "index": 1}
      ],
      "rows": [[1, "Hello World"]],
      "row_count": 1
    }
  ],
  "took": 42.0
}
```

**DDL Operations (200 OK)**:
```json
{
  "status": "success",
  "results": [
    {"row_count": 0, "message": "Table created successfully"}
  ],
  "took": 15.0
}
```

**Error (400 Bad Request)**:
```json
{
  "status": "error",
  "results": [],
  "took": 5.0,
  "error": {
    "code": "INVALID_SQL",
    "message": "...",
    "details": null
  }
}
```

**Error (429 Too Many Requests)** (rate limiting):
```json
{
  "status": "error",
  "results": [],
  "took": 1.0,
  "error": {
    "code": "RATE_LIMIT_EXCEEDED",
    "message": "Too many queries per second. Please slow down.",
    "details": null
  }
}
```

---

### GET /v1/files/{namespace}/{table_name}/{subfolder}/{file_id}

Download a file stored in a `FILE` column.

**Headers**:
- `Authorization: Basic <credentials>` or `Authorization: Bearer <token>` (required)

**Path Parameters**:
- `namespace` — Namespace name
- `table_name` — Table name
- `subfolder` — FileRef.sub (e.g., `f0001`)
- `file_id` — Stored filename (server-generated)

**Success (200 OK)**:
- `Content-Type` inferred from file extension
- `Content-Disposition: inline; filename="<file_id>"`
- Body: file bytes

**Error (401 Unauthorized)**: missing/invalid auth

**Error (404 Not Found)**: file not found

**Notes**:
- The stored filename is derived from `FileRef`.
- Stream/System tables do not support downloads.

---

### GET /v1/api/healthcheck

Check server health.

```bash
curl http://localhost:8080/v1/api/healthcheck
```

**Success (200 OK)**:
```json
{
  "status": "healthy",
  "api_version": "v1"
}
```

---

### WebSocket /v1/ws

Real-time live query subscriptions via WebSocket.

- **URL**: `ws://localhost:8080/v1/ws`
- **Auth**: send an `authenticate` message after connecting (server enforces a ~3s auth timeout)

See [WebSocket Protocol](websocket-protocol.md) for the full message schema.

---

### Admin UI Auth (cookie-based)

These endpoints are used by the Admin UI and rely on an HttpOnly cookie containing a JWT.

#### POST /v1/api/auth/login

Request body:
```json
{"username":"alice","password":"Secret123!"}
```

Success (200 OK): sets an auth cookie and returns:
```json
{
  "user": {"id":"...","username":"alice","role":"dba","email":null,"created_at":"...","updated_at":"..."},
  "expires_at": "2025-12-31T00:00:00Z",
  "access_token": "<JWT_TOKEN>"
}
```

#### POST /v1/api/auth/refresh

Uses the existing auth cookie, sets a refreshed auth cookie, and returns the same shape as login.

#### POST /v1/api/auth/logout

Clears the auth cookie.

#### GET /v1/api/auth/me

Returns the current user info from the auth cookie.

---

## SQL Syntax

This document describes the HTTP endpoints and wire formats. For the actual SQL syntax and examples, see:

- [SQL Syntax Reference](../reference/sql.md)

---

## See Also

- [WebSocket Protocol](websocket-protocol.md) - Real-time subscriptions
- [Quick Start Guide](../getting-started/quick-start.md) - Getting started tutorial
