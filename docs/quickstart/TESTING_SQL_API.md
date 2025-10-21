# Testing the SQL-Only API

This guide shows you how to test the `/api/v1/query` SQL-only endpoint in KalamDB.

## Overview

The API has been simplified to a single endpoint that accepts SQL statements:
- **Endpoint**: `POST /api/v1/query`
- **Content-Type**: `application/json`
- **Supported SQL**: `INSERT` and `SELECT` statements

## Request Format

```json
{
  "sql": "YOUR SQL STATEMENT HERE"
}
```

## Response Formats

### INSERT Response
```json
{
  "rowsAffected": 1,
  "insertedId": 123456789
}
```

### SELECT Response
```json
{
  "columns": ["msg_id", "conversation_id", "from", "timestamp", "content", "metadata"],
  "rows": [
    [123456789, "conv_123", "alice", 1699000000000000, "Hello", null]
  ],
  "rowCount": 1
}
```

## Testing Methods

### 1. Using curl (Command Line)

#### Insert a Message
```bash
curl -X POST http://localhost:8080/api/v1/query \
  -H "Content-Type: application/json" \
  -d '{"sql": "INSERT INTO messages (conversation_id, from, content) VALUES ('\''conv_123'\'', '\''alice'\'', '\''Hello, world!'\'')"}'
```

**PowerShell Version:**
```powershell
$body = @{
    sql = "INSERT INTO messages (conversation_id, from, content) VALUES ('conv_123', 'alice', 'Hello, world!')"
} | ConvertTo-Json

Invoke-RestMethod -Uri "http://localhost:8080/api/v1/query" `
    -Method Post `
    -ContentType "application/json" `
    -Body $body
```

#### Query Messages
```bash
curl -X POST http://localhost:8080/api/v1/query \
  -H "Content-Type: application/json" \
  -d '{"sql": "SELECT * FROM messages WHERE conversation_id = '\''conv_123'\'' LIMIT 10"}'
```

**PowerShell Version:**
```powershell
$body = @{
    sql = "SELECT * FROM messages WHERE conversation_id = 'conv_123' LIMIT 10"
} | ConvertTo-Json

Invoke-RestMethod -Uri "http://localhost:8080/api/v1/query" `
    -Method Post `
    -ContentType "application/json" `
    -Body $body
```

### 2. Using Rust Integration Tests

Run the integration tests:

```bash
cd backend

# Run all SQL query tests
cargo test --test test_api_query

# Run a specific test
cargo test --test test_api_query test_sql_insert_basic

# Run with output
cargo test --test test_api_query -- --nocapture
```

### 3. Using Postman or Bruno

**Collection**: See `backend/docs/bruno/KalamDb/` for Bruno collections

#### Request Setup
1. **Method**: POST
2. **URL**: `http://localhost:8080/api/v1/query`
3. **Headers**: 
   - `Content-Type: application/json`
4. **Body** (raw JSON):
   ```json
   {
     "sql": "INSERT INTO messages (conversation_id, from, content) VALUES ('conv_123', 'alice', 'Hello, world!')"
   }
   ```

### 4. Using Python

```python
import requests
import json

# Insert a message
insert_response = requests.post(
    'http://localhost:8080/api/v1/query',
    headers={'Content-Type': 'application/json'},
    json={
        'sql': "INSERT INTO messages (conversation_id, from, content) VALUES ('conv_123', 'alice', 'Hello, world!')"
    }
)
print(f"Insert response: {insert_response.json()}")

# Query messages
select_response = requests.post(
    'http://localhost:8080/api/v1/query',
    headers={'Content-Type': 'application/json'},
    json={
        'sql': "SELECT * FROM messages WHERE conversation_id = 'conv_123'"
    }
)
print(f"Select response: {select_response.json()}")
```

## SQL Examples

### INSERT Statements

#### Basic Insert
```sql
INSERT INTO messages (conversation_id, from, content) 
VALUES ('conv_123', 'alice', 'Hello, world!')
```

#### Insert with Metadata
```sql
INSERT INTO messages (conversation_id, from, content, metadata) 
VALUES ('conv_123', 'alice', 'Hello', '{"role":"user","model":"gpt-4"}')
```

#### Insert with Timestamp
```sql
INSERT INTO messages (conversation_id, from, timestamp, content) 
VALUES ('conv_123', 'alice', 1699000000000000, 'Hello')
```

### SELECT Statements

#### Select All Messages in a Conversation
```sql
SELECT * FROM messages WHERE conversation_id = 'conv_123'
```

#### Select with Limit
```sql
SELECT * FROM messages WHERE conversation_id = 'conv_123' LIMIT 50
```

#### Select with Order By
```sql
SELECT * FROM messages 
WHERE conversation_id = 'conv_123' 
ORDER BY timestamp DESC 
LIMIT 10
```

#### Select Messages After a Specific ID
```sql
SELECT * FROM messages 
WHERE conversation_id = 'conv_123' AND msg_id > 123456789
```

#### Select Specific Columns
```sql
SELECT msg_id, from, content FROM messages 
WHERE conversation_id = 'conv_123'
```

## Running the Server

1. **Start the server:**
   ```bash
   cd backend
   cargo run
   ```

2. **Server will start on** `http://localhost:8080` (default)

3. **Check logs:** `backend/logs/app.log`

## Error Responses

### Invalid SQL Syntax
```json
{
  "error": "Invalid SQL syntax",
  "message": "Only INSERT and SELECT statements are supported"
}
```

### Missing Required Field
```json
{
  "error": "Query execution failed",
  "message": "content is required"
}
```

### Invalid Table
```json
{
  "error": "Query execution failed",
  "message": "Unknown table: users. Only 'messages' table is supported"
}
```

## Testing Checklist

- [ ] Insert a basic message
- [ ] Insert a message with metadata
- [ ] Query messages by conversation_id
- [ ] Query with LIMIT
- [ ] Query with ORDER BY DESC
- [ ] Query messages after a specific msg_id
- [ ] Test invalid SQL (should return 400)
- [ ] Test missing required fields (should return 400)
- [ ] Test invalid table name (should return 400)
- [ ] Verify message persistence (restart server, query again)

## Advanced Testing

### Load Testing
```bash
# Install wrk
# Windows: Use WSL or download from GitHub

# Basic load test
wrk -t4 -c100 -d30s --latency \
  -s insert_test.lua \
  http://localhost:8080/api/v1/query
```

### Concurrent Inserts
```python
import concurrent.futures
import requests

def insert_message(i):
    return requests.post(
        'http://localhost:8080/api/v1/query',
        json={
            'sql': f"INSERT INTO messages (conversation_id, from, content) VALUES ('conv_test', 'user_{i}', 'Message {i}')"
        }
    )

with concurrent.futures.ThreadPoolExecutor(max_workers=10) as executor:
    futures = [executor.submit(insert_message, i) for i in range(100)]
    results = [f.result() for f in concurrent.futures.as_completed(futures)]

print(f"Completed {len(results)} inserts")
```

## Troubleshooting

### Server not responding
- Check if server is running: `ps aux | grep kalamdb` (Linux/Mac) or Task Manager (Windows)
- Check logs: `tail -f backend/logs/app.log`
- Verify port is not in use: `netstat -an | findstr 8080`

### "Failed to create store" error
- Ensure `backend/data/` directory exists
- Check file permissions
- Verify RocksDB path in `config.toml`

### Tests failing
- Ensure no server is running on port 8080
- Clean test data: `rm -rf backend/data/test_*`
- Run tests with `--nocapture` to see output

## Next Steps

Once basic testing is complete:
1. Implement Phase 4 (SELECT with complex queries)
2. Add WebSocket support for real-time streaming
3. Add authentication/authorization
4. Implement Parquet consolidation

## References

- REST API Contract: `specs/001-build-a-rust/contracts/rest-api.yaml`
- Architecture Docs: `specs/001-build-a-rust/ARCHITECTURE-UPDATE.md`
- SQL Changes Doc: `specs/001-build-a-rust/CHANGES-SQL-ONLY.md`
