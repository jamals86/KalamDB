# KalamDB TypeScript SDK - Quick Start Guide

Get up and running with the KalamDB TypeScript SDK in 5 minutes!

## Prerequisites

- Node.js 18+ or modern browser
- Running KalamDB server (see [server setup](../../../docs/QUICK_START.md))
- Basic TypeScript/JavaScript knowledge

## Installation

```bash
npm install @kalamdb/client
```

## 1. Import and Create Client

```typescript
import { KalamDBClient } from '@kalamdb/client';

const client = new KalamDBClient(
  'http://localhost:8080',  // Server URL
  'your-username',          // Username
  'your-password'           // Password
);
```

## 2. Connect to Server

```typescript
await client.connect();
console.log('Connected:', client.isConnected()); // true
```

## 3. Execute SQL Queries

### Create a Namespace and Table

```typescript
// Create namespace (logical container)
await client.query('CREATE NAMESPACE IF NOT EXISTS app');

// Create table
await client.query(`
  CREATE TABLE IF NOT EXISTS app.users (
    id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
    name TEXT NOT NULL,
    email TEXT NOT NULL,
    created_at TIMESTAMP DEFAULT NOW()
  ) WITH (TYPE='SHARED', FLUSH_POLICY='rows:1000')
`);
```

### Insert Data

```typescript
// Using insert() convenience method
await client.insert('app.users', {
  name: 'Alice Johnson',
  email: 'alice@example.com'
});

// Or using SQL directly
await client.query(
  "INSERT INTO app.users (name, email) VALUES ('Bob Smith', 'bob@example.com')"
);
```

### Query Data

```typescript
const result = await client.query('SELECT * FROM app.users');

console.log(`Found ${result.results[0].row_count} users`);
result.results[0].rows.forEach(user => {
  console.log(`${user.name} - ${user.email}`);
});
```

## 4. Real-time Subscriptions

Subscribe to live updates on a table:

```typescript
const subId = await client.subscribe('app.users', (event) => {
  if (event.type === 'change' && event.change_type === 'insert') {
    console.log('New user:', event.rows[0]);
  }
});

// Later: unsubscribe
await client.unsubscribe(subId);
```

## 5. Cleanup

```typescript
await client.disconnect();
```

## Complete Example

```typescript
import { KalamDBClient } from '@kalamdb/client';

async function main() {
  // 1. Create and connect
  const client = new KalamDBClient(
    'http://localhost:8080',
    'root',
    'password'
  );
  await client.connect();

  try {
    // 2. Setup schema
    await client.query('CREATE NAMESPACE IF NOT EXISTS app');
    await client.query(`
      CREATE TABLE IF NOT EXISTS app.tasks (
        id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
        title TEXT NOT NULL,
        done BOOLEAN DEFAULT false
      ) WITH (TYPE='SHARED')
    `);

    // 3. Insert data
    await client.insert('app.tasks', { title: 'Learn KalamDB' });
    await client.insert('app.tasks', { title: 'Build awesome app' });

    // 4. Query data
    const result = await client.query('SELECT * FROM app.tasks');
    console.log('Tasks:', result.results[0].rows);

    // 5. Subscribe to changes
    const subId = await client.subscribe('app.tasks', (event) => {
      console.log('Event:', event.type);
    });

    // 6. Update data (triggers subscription)
    await client.query("UPDATE app.tasks SET done = true WHERE title = 'Learn KalamDB'");

    // Wait a bit for events
    await new Promise(r => setTimeout(r, 1000));

    // 7. Cleanup
    await client.unsubscribe(subId);
  } finally {
    await client.disconnect();
  }
}

main().catch(console.error);
```

## Next Steps

- 📖 Read the [full API documentation](README.md)
- 🔍 Explore [SQL syntax reference](../../../docs/SQL.md)
- 💡 Check out [complete examples](README.md#complete-examples)
- 🧪 Run the [integration tests](tests/integration.test.js)

## Troubleshooting

### "Cannot find module '@kalamdb/client'"

Make sure you've installed the package:
```bash
npm install @kalamdb/client
```

### "WebSocket connection failed"

- Ensure KalamDB server is running on the specified URL
- Check that the server URL is correct (default: `http://localhost:8080`)
- Verify firewall/network settings allow WebSocket connections

### "Authentication failed"

- Verify username and password are correct
- Default credentials are usually `root`/`root` for local development
- Check server logs for authentication errors

### Type errors in TypeScript

Ensure your `tsconfig.json` has:
```json
{
  "compilerOptions": {
    "module": "ES2020",
    "moduleResolution": "node",
    "esModuleInterop": true
  }
}
```

## Support

Need help? 
- 📖 [Full Documentation](../../../docs/)
- 🐛 [Report Issues](https://github.com/jamals86/KalamDB/issues)
- 💬 [Join Discussions](https://github.com/jamals86/KalamDB/discussions)
