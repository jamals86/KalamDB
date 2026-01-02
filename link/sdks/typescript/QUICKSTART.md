# KalamDB TypeScript SDK - Quick Start Guide

Get up and running with the KalamDB TypeScript SDK in 5 minutes!

## Prerequisites

- Node.js 18+ or modern browser
- Running KalamDB server (see [server setup](../../../docs/getting-started/quick-start.md))
- Basic TypeScript/JavaScript knowledge

If you're building the SDK from source in this repo, see [README.md](README.md#build-from-source-this-repo).

## Installation

```bash
npm install kalam-link
```

## 1. Create a Client

```typescript
import { createClient, Auth } from 'kalam-link';

const client = createClient({
  url: 'http://localhost:8080',
  // For local dev, the default root user supports an empty password:
  auth: Auth.basic('root', '')
});
```

## 2. Connect to Server

```typescript
await client.connect();
console.log('Connected:', client.isConnected()); // true
```

## 3. Execute SQL Queries

Start with a simple query to verify connectivity:

```typescript
const result = await client.query('SELECT 1');
console.log(result.status);
console.log(result.results[0]);
```

For schema/table creation and the supported SQL dialect, use the canonical docs:

- [SQL Reference](../../../docs/reference/sql.md)
- [API Reference](../../../docs/api/api-reference.md)

## 4. Real-time Subscriptions

Subscribe to live updates on a table:

```typescript
const unsubscribe = await client.subscribe('app.users', (event) => {
  if (event.type === 'change') {
    console.log('Change:', event.change_type);
  }
});

await unsubscribe();
```

## 5. Cleanup

```typescript
await client.disconnect();
```

## Complete Example

```typescript
import { createClient, Auth } from 'kalam-link';

async function main() {
  // 1. Create and connect
  const client = createClient({
    url: 'http://localhost:8080',
    auth: Auth.basic('root', '')
  });
  await client.connect();

  try {
    const result = await client.query('SELECT CURRENT_USER()');
    console.log('Current user:', result);
  } finally {
    await client.disconnect();
  }
}

main().catch(console.error);
```

## Next Steps

- 📖 Read the [full API documentation](README.md)
- 🔍 Explore [SQL syntax reference](../../../docs/reference/sql.md)
- 💡 Check out [complete examples](README.md#complete-examples)
- 🧪 Run the [integration tests](tests/integration.test.js)

## Troubleshooting

### "Cannot find module 'kalam-link'"

Make sure you've installed the package:
```bash
npm install kalam-link
```

### "WebSocket connection failed"

- Ensure KalamDB server is running on the specified URL
- Check that the server URL is correct (default: `http://localhost:8080`)
- Verify firewall/network settings allow WebSocket connections

### "Authentication failed"

- Verify username and password are correct
- For local development, use `root` with an empty password: `Auth.basic('root', '')`
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
