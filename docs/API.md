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
	"status": "ok"
}
```

### 2. Execute SQL over HTTP

```http
POST /v1/api/sql
Content-Type: application/json

{
	"sql": "SELECT 1 AS value;"
}
```

**Example `curl`:**

```bash
curl -X POST http://localhost:8080/v1/api/sql \
	-H "Content-Type: application/json" \
	-d '{"sql": "SELECT 1 AS value;"}'
```

The response is JSON rows, e.g.:

```json
{
	"columns": ["value"],
	"rows": [[1]]
}
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
2. Send a subscription request with a SQL filter.
3. Receive change events for matching rows.

Example text message sent after connecting:

```json
{
	"type": "subscribe",
	"sql": "SUBSCRIBE TO app.messages WHERE user_id = 'alice' OPTIONS (last_rows = 10);"
}
```

Example event from server:

```json
{
	"type": "INSERT",
	"table": "app.messages",
	"data": {
		"id": 123,
		"user_id": "alice",
		"content": "Hello",
		"timestamp": "2025-11-15T10:00:00Z"
	}
}
```

Use the `kalam` CLI (see `docs/cli.md`) for an easier way to work with subscriptions.
