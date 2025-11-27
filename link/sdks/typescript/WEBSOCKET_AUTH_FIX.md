# WebSocket Authentication Issue - FIXED

## Problem
The root user has NO PASSWORD by default, causing WebSocket authentication to fail with:
```
Invalid credentials: Invalid username or password
```

## Solution
Set a password for the root user using SQL:

```sql
ALTER USER root SET PASSWORD 'root';
```

## How to Execute

### Option 1: Using CLI
```bash
cd cli
cargo run -- --url http://localhost:8080 --username root query "ALTER USER root SET PASSWORD 'root';"
```

### Option 2: Using SQL Query in example.html
1. Start the server
2. Open example.html in browser
3. In the SQL Query textarea, enter:
   ```sql
   ALTER USER root SET PASSWORD 'root';
   ```
4. Click Execute (or press Ctrl+Enter)

### Option 3: Using curl
```bash
curl -X POST http://localhost:8080/v1/query \
  -H "Content-Type: application/json" \
  -d '{"sql": "ALTER USER root SET PASSWORD '\''root'\'';"}'
```

## After Setting Password
The WebSocket authentication will work with:
- URL: `ws://localhost:8080/v1/ws?auth=cm9vdDpyb290`
- Where `cm9vdDpyb290` is base64(`root:root`)

## Future Improvement
For better security, implement post-connection authentication:
1. Open WebSocket without auth parameter
2. Send authentication message after connection
3. Server validates and associates connection with user

This prevents credentials from appearing in logs/URLs.
