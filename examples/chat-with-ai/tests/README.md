# Chat-with-AI Tests

Comprehensive test suite for the KalamDB chat-with-ai example.

## Test Files

### `wasm-client.test.ts` - WASM Client Integration Tests ⭐ NEW

**What it tests:**
- WASM initialization and loading
- WebSocket connection establishment  
- Live query subscriptions (real-time updates)
- Message insertion and subscription notifications
- Multiple simultaneous subscriptions
- Conversation and message CRUD operations

**Why it's important:**
- Tests the actual WASM client used by the UI (not just HTTP endpoints)
- Verifies real-time subscription functionality
- Catches WASM loading issues early (before UI complications)
- Runs in Node.js (faster, no browser needed)

**Run:**
```bash
npm run test:wasm
```

**Requirements:**
- Running KalamDB server on `localhost:8080`
- SDK built (`cd link/sdks/typescript && bash build.sh`)
- Test user credentials (demo-user/demo123)

### `api.test.ts` - HTTP API Tests

**What it tests:**
- REST API endpoint responses
- Authentication flows
- SQL query execution
- Error handling

**Run:**
```bash
npm run test:api
```

### `sdk.test.ts` - SDK Method Tests

**What it tests:**
- Login endpoint validation
- Topic consumption endpoints
- Query patterns used by the SDK
- JWT token flows

**Run:**
```bash
npx tsx tests/sdk.test.ts
```

### `hooks.test.tsx` - React Hook Tests

**What it tests:**
- Custom React hooks behavior
- Hook state management
- Component integration

**Run:**
```bash
npm test  # Uses vitest
```

## Running All Tests

```bash
# Run all tests sequentially
npm run test:all

# Or run individually
npm run test:api    # HTTP endpoints
npm run test:wasm   # WASM client + subscriptions
npm test            # React hooks (vitest)
```

## Environment Variables

Tests use these environment variables (with defaults):

```bash
KALAMDB_SERVER_URL=http://localhost:8080  # Server URL
```

## Test Order Recommendation

When debugging issues, run tests in this order:

1. **`test:api`** - Verify server is running and HTTP endpoints work
2. **`test:wasm`** - Verify WASM client can connect and subscribe
3. **`npm test`** - Verify React components work with the client

## WASM Client Test Flow

The `wasm-client.test.ts` mimics the exact usage pattern in the UI:

```typescript
// 1. Create client with WASM URL
const client = createClient({
  url: KALAMDB_URL,
  auth: Auth.basic(username, password),
  wasmUrl: '/path/to/kalam_link_bg.wasm', // Explicit WASM path
});

// 2. Login (HTTP, no WASM)
await client.login();

// 3. Connect WebSocket (initializes WASM)
await client.connect();

// 4. Configure auto-reconnect (requires WASM initialized)
client.setAutoReconnect(true);
client.setReconnectDelay(1000, 30000);

// 5. Subscribe to live queries
const subscription = await client.subscribe(
  'SELECT * FROM chat.messages WHERE conversation_id = ?',
  (data) => console.log('Received:', data),
  (error) => console.error('Error:', error)
);

// 6. Cleanup
await subscription.unsubscribe();
await client.disconnect();
```

## Debugging Test Failures

### WASM file not found
```bash
cd /Users/jamal/git/KalamDB/link/sdks/typescript
bash build.sh
```

### Server not running
```bash
cd /Users/jamal/git/KalamDB/backend
cargo run
```

### Authentication fails
Check if test users exist:
```sql
SELECT username FROM system.users WHERE username IN ('demo-user', 'ai-service');
```

### Subscription timeout
- Check server logs for WebSocket errors
- Verify JWT token is valid
- Ensure live query feature is enabled

## Success Criteria

All tests pass means:
- ✅ Server HTTP API working
- ✅ Authentication functioning
- ✅ WASM loads successfully
- ✅ WebSocket connects
- ✅ Live queries deliver real-time updates
- ✅ Multiple subscriptions work simultaneously
- ✅ React hooks integrate properly

This gives confidence that the UI will work!
