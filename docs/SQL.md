## KalamDB SQL

Short guide to the SQL you can run against KalamDB.

### Supported data types

- `BOOLEAN`
- `SMALLINT`
- `INT`
- `BIGINT`
- `FLOAT`
- `DOUBLE`
- `DECIMAL`
- `TEXT`
- `JSON`
- `BYTES`
- `UUID`
- `EMBEDDING`
- `DATE`
- `TIME`
- `TIMESTAMP`
- `DATETIME`

### 1. Namespaces

```sql
-- Logical database
CREATE NAMESPACE app;

SHOW NAMESPACES;
DROP NAMESPACE app; -- if empty
```

### 2. Table types

```sql
CREATE USER TABLE app.messages (
	id BIGINT DEFAULT SNOWFLAKE_ID(),
	user_id TEXT NOT NULL,
	content TEXT NOT NULL,
	created_at TIMESTAMP DEFAULT NOW()
) FLUSH ROW_THRESHOLD 1000;
-- One table instance per user id
CREATE USER TABLE app.messages (
	id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
	user_id TEXT NOT NULL,
	content TEXT NOT NULL,
	created_at TIMESTAMP DEFAULT NOW()
) FLUSH ROW_THRESHOLD 1000;

CREATE SHARED TABLE app.config (
	key TEXT PRIMARY KEY,
	value JSON
);
-- Shared global table
CREATE SHARED TABLE app.config (
	key TEXT PRIMARY KEY,
	value JSON
);

CREATE STREAM TABLE app.events (
	id BIGINT DEFAULT SNOWFLAKE_ID(),
	payload JSON,
	created_at TIMESTAMP DEFAULT NOW()
) TTL 600;
-- In‑memory stream table with TTL in seconds
CREATE STREAM TABLE app.events (
	id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
	payload JSON,
	created_at TIMESTAMP DEFAULT NOW()
) TTL 600;
```

Drop tables:

```sql
DROP TABLE app.messages;
DROP TABLE app.config;
DROP TABLE app.events;
```

### 3. Inserts, updates, deletes

```sql
-- Insert rows
INSERT INTO app.messages (user_id, content)
VALUES
	('alice', 'Hello'),
	('bob', 'Hi there');

-- Update
UPDATE app.messages
SET content = 'Hello again'
WHERE user_id = 'alice';

-- Delete
DELETE FROM app.messages
WHERE user_id = 'bob';
```

### 4. Queries

```sql
-- Basic select
SELECT * FROM app.messages;

-- Filter + order + limit
SELECT user_id, content, created_at
FROM app.messages
WHERE created_at > NOW() - INTERVAL '5 minutes'
ORDER BY created_at DESC
LIMIT 20;

-- Aggregation
SELECT user_id, COUNT(*) AS message_count
FROM app.messages
GROUP BY user_id
HAVING message_count > 10;
```

### 5. Manual flush

```sql
-- Move recent hot data to Parquet segments
FLUSH TABLE app.messages;
```

### 6. Live subscriptions

Use SQL to express what you want to watch; results stream over WebSocket.

```sql
SUBSCRIBE TO app.messages
WHERE user_id = 'alice'
OPTIONS (last_rows = 10);
```

### 7. System tables & helpers

```sql
-- Introspection
SELECT * FROM system.tables;
SELECT * FROM system.namespaces;
SELECT * FROM system.users;

-- Built‑in helpers
SELECT SNOWFLAKE_ID() AS id,
			 UUID_V7()      AS uuid,
			 ULID()         AS ulid,
			 CURRENT_USER() AS current_user;
```
