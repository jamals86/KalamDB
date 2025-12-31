## KalamDB HTTP & WebSocket API

Simple overview of how to talk to KalamDB from any language.

### Base URL

Default dev server:

```text
http://localhost:8080
```

### 1. Healthcheck

```http
GET /v1/api/healthcheck
```

**Response 200**

```json
{
	"status": "healthy",
	"api_version": "v1"
}
```

### 2. Execute SQL over HTTP

```http
POST /v1/api/sql
Content-Type: application/json
Authorization: Basic <credentials> | Bearer <token>

{
	"sql": "SELECT 1 AS value;"
}
```

**Example `curl`:**

```bash
# Local dev (localhost): root user supports empty password
curl -u root: -X POST http://localhost:8080/v1/api/sql \
	-H "Content-Type: application/json" \
	-d '{"sql": "SELECT 1 AS value;"}'
```

The response is JSON rows, e.g.:

```json
{
	"status": "success",
	"results": [
		{
			"schema": [{"name": "value", "data_type": "BigInt", "index": 0}],
			"rows": [[1]],
			"row_count": 1
		}
	],
	"took": 1.23
}
```

### 3. Admin UI auth (cookie-based)

The Admin UI uses cookie-based auth endpoints under `/v1/api/auth`:

```http
POST /v1/api/auth/login
POST /v1/api/auth/refresh
POST /v1/api/auth/logout
GET  /v1/api/auth/me
```

### 3. Auth headers

KalamDB supports both Basic auth and JWT.

- **Basic**: `Authorization: Basic base64(username:password)`
- **JWT**: `Authorization: Bearer <token>`

```bash
curl -X POST http://localhost:8080/v1/api/sql \
	-H "Content-Type: application/json" \
	-H "Authorization: Basic $(printf 'alice:Secret123!' | base64)" \
	-d '{"sql": "SELECT CURRENT_USER();"}'
```

### 4. WebSocket subscriptions

WebSocket endpoint:

```text
ws://localhost:8080/v1/ws
```

Basic flow:

1. Open a WebSocket connection to `/v1/ws`.
2. Send an `authenticate` message.
3. Send a `subscribe` message.
4. Receive batches and change notifications.

See [WebSocket Protocol](websocket-protocol.md) for the full message schema.

Example messages sent after connecting:

```json
{
	"type": "authenticate",
	"method": "jwt",
	"token": "<JWT_TOKEN>"
}
```

```json
{
	"type": "subscribe",
	"subscription": {
		"id": "messages_recent",
		"sql": "SELECT * FROM app.messages WHERE user_id = 'alice'",
		"options": {"last_rows": 10}
	}
}
```

Use the `kalam` CLI (see `../getting-started/cli.md`) for an easier way to work with subscriptions.
