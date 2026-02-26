# kalam_link

Official Dart and Flutter SDK for [KalamDB](https://kalamdb.org) — private, realtime storage for AI agents.

KalamDB is a SQL-first realtime database. Every user or tenant gets a private namespace. Subscribe to any SQL query live over WebSocket. Publish and consume events via Topics. Store recent data fast and move cold history to object storage.

→ **[kalamdb.org](https://kalamdb.org)** · [Docs](https://kalamdb.org/docs) · [GitHub](https://github.com/jamals86/KalamDB)

## Features

- **SQL Queries** — execute parameterized SQL with `$1`, `$2` placeholders
- **Live Subscriptions** — subscribe to any SQL query; receive inserts, updates, and deletes in real-time over WebSocket
- **Per-tenant isolation** — each user gets a private namespace; no app-side `WHERE user_id = ?` filters needed
- **Topics & Pub/Sub** — publish events to topics and consume them from any client or agent
- **Authentication** — HTTP Basic Auth, JWT tokens, dynamic `authProvider` callback, or anonymous access
- **Cross-platform** — iOS, Android, macOS, Windows, Linux, and Web (WASM)

## Installation

```yaml
dependencies:
  kalam_link: ^0.1.1
```

```bash
flutter pub add kalam_link
```

## Initialization

**`KalamClient.init()` must be called once at app startup** before any other SDK call. It initializes the underlying Rust runtime.

```dart
void main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await KalamClient.init();
  runApp(MyApp());
}
```

## Connecting

```dart
import 'package:kalam_link/kalam_link.dart';

final client = await KalamClient.connect(
  url: 'https://db.example.com',
  authProvider: () async {
    final token = await myApp.getOrRefreshJwt();
    return Auth.jwt(token);
  },
);
```

### `connect()` options

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `url` | `String` | required | Server URL |
| `authProvider` | `AuthProvider?` | `null` | **Recommended.** Async callback invoked before each connection for fresh credentials. Takes precedence over `auth`. |
| `auth` | `Auth` | `Auth.none()` | Static credentials. Use `authProvider` for tokens that can expire. |
| `timeout` | `Duration` | 30 s | HTTP request timeout |
| `maxRetries` | `int` | 3 | Retry count for idempotent (SELECT) queries |
| `disableCompression` | `bool` | `false` | Disable gzip on WebSocket (useful during development) |
| `connectionHandlers` | `ConnectionHandlers?` | `null` | Lifecycle event callbacks — see below |

## Authentication

### `authProvider` — recommended

Use `authProvider` for OAuth flows, refresh tokens, or any credentials fetched from secure storage. The callback is invoked before every connection attempt, so tokens are always fresh.

```dart
final client = await KalamClient.connect(
  url: 'https://db.example.com',
  authProvider: () async {
    // fetch from keychain, refresh if expired, etc.
    final token = await myApp.getOrRefreshJwt();
    return Auth.jwt(token);
  },
);
```

Re-invoke the `authProvider` on demand (e.g. after a 401 or on a schedule):

```dart
await client.refreshAuth();

// Periodic refresh example:
Timer.periodic(Duration(minutes: 55), (_) => client.refreshAuth());
```

### Static auth

For simpler scenarios where tokens do not expire:

```dart
// HTTP Basic Auth
final client = await KalamClient.connect(
  url: 'https://db.example.com',
  auth: Auth.basic('alice', 'secret123'),
);

// JWT token
final client = await KalamClient.connect(
  url: 'https://db.example.com',
  auth: Auth.jwt('eyJhbGci...'),
);

// No authentication (localhost bypass)
final client = await KalamClient.connect(
  url: 'http://localhost:8080',
);
```

### Login and token refresh

Exchange Basic credentials for a JWT and use it for subsequent connections:

```dart
final bootstrap = await KalamClient.connect(url: serverUrl);
final tokens = await bootstrap.login('alice', 'secret123');
await bootstrap.dispose();

final client = await KalamClient.connect(
  url: serverUrl,
  auth: Auth.jwt(tokens.accessToken),
);
```

Refresh an expiring token:

```dart
final fresh = await client.refreshToken(tokens.refreshToken!);
// use fresh.accessToken for the next connection
```

## Executing Queries

```dart
// Simple query
final result = await client.query('SELECT * FROM users LIMIT 10');
for (final row in result.rows) {
  print(row); // List<dynamic>
}

// Rows as maps keyed by column name
final maps = result.results.first.toMaps();

// Parameterized query
final result = await client.query(
  r'SELECT * FROM orders WHERE user_id = $1 AND status = $2',
  params: ['user-uuid-123', 'pending'],
);

// Query scoped to a user namespace
final result = await client.query(
  'SELECT * FROM messages LIMIT 5',
  namespace: 'alice',
);
```

### `QueryResponse` fields

| Field | Type | Description |
|-------|------|-------------|
| `success` | `bool` | Whether the query succeeded |
| `results` | `List<QueryResult>` | Result sets (one per statement) |
| `tookMs` | `double?` | Execution time in milliseconds |
| `error` | `ErrorDetail?` | Error details when `success` is `false` |

### `QueryResult` fields

| Field | Type | Description |
|-------|------|-------------|
| `columns` | `List<SchemaField>` | Column metadata |
| `rows` | `List<List<dynamic>>` | Parsed row values |
| `rowCount` | `int` | Rows affected or returned |
| `message` | `String?` | DDL message (e.g. `"Table created"`) |
| `toMaps()` | `List<Map<String, dynamic>>` | Rows keyed by column name |

## Live Subscriptions

Subscribe to any SQL query. The returned `Stream<ChangeEvent>` emits real-time events:

```dart
final stream = client.subscribe(
  'SELECT * FROM chat.messages WHERE room_id = $1 ORDER BY created_at ASC',
  batchSize: 100,   // rows per initial-data batch
  lastRows: 50,     // rewind: include last N rows before live changes
);

await for (final event in stream) {
  switch (event) {
    case AckEvent(:final subscriptionId, :final totalRows):
      print('Subscribed $subscriptionId. Snapshot rows: $totalRows');
    case InitialDataBatch(:final rowsJson, :final hasMore):
      print('Snapshot batch, hasMore=$hasMore');
    case InsertEvent(:final row):
      print('New row: $row');
    case UpdateEvent(:final row, :final oldRow):
      print('Updated: $oldRow → $row');
    case DeleteEvent(:final row):
      print('Deleted: $row');
    case SubscriptionError(:final message):
      print('Error: $message');
  }
}
```

Cancel the subscription by cancelling the `StreamSubscription`:

```dart
final sub = stream.listen((_) {});
// later:
await sub.cancel();
```

### `subscribe()` options

| Parameter | Type | Description |
|-----------|------|-------------|
| `sql` | `String` | SQL query to watch |
| `batchSize` | `int?` | Max rows per initial snapshot batch |
| `lastRows` | `int?` | Include the last N rows before live changes begin |
| `subscriptionId` | `String?` | Custom subscription ID |

### `ChangeEvent` variants

| Variant | Fields |
|---------|--------|
| `AckEvent` | `subscriptionId`, `totalRows`, `schema`, `batchNum`, `hasMore`, `status` |
| `InitialDataBatch` | `subscriptionId`, `rowsJson`, `batchNum`, `hasMore`, `status` |
| `InsertEvent` | `subscriptionId`, `rowsJson`, `row` |
| `UpdateEvent` | `subscriptionId`, `rowsJson`, `oldRowsJson`, `row`, `oldRow` |
| `DeleteEvent` | `subscriptionId`, `oldRowsJson`, `row` |
| `SubscriptionError` | `subscriptionId`, `code`, `message` |

## Connection Lifecycle Handlers

Hook into connection events for logging, UI state, or diagnostics:

```dart
final client = await KalamClient.connect(
  url: 'https://db.example.com',
  authProvider: () async => Auth.jwt(await getToken()),
  connectionHandlers: ConnectionHandlers(
    onConnect: () => print('connected'),
    onDisconnect: (reason) => print('disconnected: ${reason.message}'),
    onError: (error) => print('error: ${error.message}'),
    onReceive: (message) => print('[recv] $message'),
    onSend: (message) => print('[send] $message'),
  ),
);
```

## Server Health & Setup

```dart
// Check server health (version, status)
final health = await client.healthCheck();
print('${health.status} — v${health.version}');

// Check if first-time setup is required
final setup = await client.checkSetupStatus();
if (setup.needsSetup) {
  await client.serverSetup(ServerSetupRequest(
    username: 'admin',
    password: 'AdminPass123!',
    rootPassword: 'RootPass456!',
  ));
}
```

## Disposing

Always dispose the client when done to release resources:

```dart
await client.dispose();
```

## Full API Reference

| Method | Description |
|--------|-------------|
| `KalamClient.init()` | **Required.** Initialize the Rust runtime once at app startup |
| `KalamClient.connect(url, {authProvider, auth, timeout, maxRetries, disableCompression, connectionHandlers})` | Create a connected client |
| `query(sql, {params, namespace})` | Execute parameterized SQL |
| `subscribe(sql, {batchSize, lastRows, subscriptionId})` | Subscribe to live changes — returns `Stream<ChangeEvent>` |
| `login(username, password)` | Exchange Basic credentials for JWT tokens |
| `refreshToken(refreshToken)` | Refresh an expiring access token |
| `refreshAuth()` | Re-invoke `authProvider` and update credentials in-place |
| `healthCheck()` | Check server health and version |
| `checkSetupStatus()` | Check if first-time setup is needed |
| `serverSetup(request)` | Perform initial server setup |
| `dispose()` | Release resources |

## License

Apache-2.0

## Links

- Website: [kalamdb.org](https://kalamdb.org)
- Docs: [kalamdb.org/docs](https://kalamdb.org/docs)
- SDK reference: [kalamdb.org/docs/sdk](https://kalamdb.org/docs/sdk)
- Authentication & token setup: [kalamdb.org/docs/getting-started/authentication](https://kalamdb.org/docs/getting-started/authentication)
- Docker deployment: [kalamdb.org/docs/getting-started/docker](https://kalamdb.org/docs/getting-started/docker)
- Live Query / WebSocket architecture: [kalamdb.org/docs/architecture/live-query](https://kalamdb.org/docs/architecture/live-query)
- WebSocket protocol reference: [kalamdb.org/docs/api/websocket-protocol](https://kalamdb.org/docs/api/websocket-protocol)
- GitHub: [github.com/jamals86/KalamDB](https://github.com/jamals86/KalamDB)

---

> Native performance on iOS and Android is powered by the excellent [flutter_rust_bridge](https://cjycode.com/flutter_rust_bridge/) project — thank you to its maintainers and contributors.

