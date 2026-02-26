# kalam_link

Official Dart and Flutter SDK for [KalamDB](https://kalamdb.org) — private, realtime storage for AI agents.

KalamDB is a SQL-first realtime database. Every user or tenant gets a private namespace. Subscribe to any SQL query live over WebSocket. Publish and consume events via Topics. Store recent data fast and move cold history to object storage.

→ **[kalamdb.org](https://kalamdb.org)** · [Docs](https://kalamdb.org/docs) · [GitHub](https://github.com/jamals86/KalamDB)

## Features

- **SQL Queries** — execute parameterized SQL with `$1`, `$2` placeholders
- **Live Subscriptions** — subscribe to any SQL query; receive inserts, updates, and deletes in real-time over WebSocket
- **Per-tenant isolation** — each user gets a private namespace; no app-side `WHERE user_id = ?` filters needed
- **Topics & Pub/Sub** — publish events to topics and consume them from any client or agent
- **Authentication** — HTTP Basic Auth, JWT tokens, or anonymous access
- **Cross-platform** — iOS, Android, macOS, Windows, Linux, and Web (WASM)

## Installation

```yaml
dependencies:
  kalam_link: ^0.1.0
```

```bash
flutter pub add kalam_link
```

## Quick Start

```dart
import 'package:kalam_link/kalam_link.dart';

// Connect with basic auth
final client = await KalamClient.connect(
  url: 'https://db.example.com',
  auth: Auth.basic('alice', 'secret123'),
);

// Execute a query
final result = await client.query('SELECT * FROM users LIMIT 10');
for (final row in result.rows) {
  print(row);
}

// Parameterized query
final filtered = await client.query(
  r'SELECT * FROM orders WHERE status = $1',
  params: ['pending'],
);

// Subscribe to live changes
final stream = client.subscribe('SELECT * FROM messages');
await for (final event in stream) {
  switch (event) {
    case InsertEvent(:final row):
      print('New message: ${row['body']}');
    case DeleteEvent(:final row):
      print('Deleted: ${row['id']}');
    case _:
      break;
  }
}

await client.dispose();
```

## Authentication

```dart
// HTTP Basic Auth
final auth = Auth.basic('username', 'password');

// JWT token (e.g. after login)
final loginResult = await client.login('alice', 'secret123');
final auth = Auth.jwt(loginResult.accessToken);

// No authentication
final auth = Auth.none();
```

## Connection Lifecycle Handlers

Pass `ConnectionHandlers` to `KalamClient.connect` to hook into connection events:

```dart
final client = await KalamClient.connect(
  url: 'https://db.example.com',
  auth: Auth.jwt(token),
  connectionHandlers: ConnectionHandlers(
    onConnect: () => print('connected'),
    onDisconnect: (reason) => print('disconnected: ${reason.message}'),
    onError: (error) => print('error: ${error.message}'),
    onReceive: (message) => print('[recv] $message'),
    onSend: (message) => print('[send] $message'),
  ),
);
```

## API Reference

### KalamClient

| Method | Description |
|--------|-------------|
| `KalamClient.connect(url, auth, timeout, maxRetries)` | Create a connected client |
| `query(sql, params?, namespace?)` | Execute a SQL query |
| `subscribe(sql, batchSize?, lastRows?)` | Subscribe to live changes (returns `Stream<ChangeEvent>`) |
| `login(username, password)` | Authenticate and get tokens |
| `refreshToken(refreshToken)` | Refresh an expiring access token |
| `healthCheck()` | Check server health |
| `checkSetupStatus()` | Check if server needs setup |
| `serverSetup(request)` | Perform initial server setup |
| `dispose()` | Release resources |

### ChangeEvent (sealed class)

| Variant | Fields |
|---------|--------|
| `AckEvent` | `subscriptionId`, `totalRows`, `schema`, `batchNum`, `hasMore`, `status` |
| `InitialDataBatch` | `subscriptionId`, `rows`, `batchNum`, `hasMore`, `status` |
| `InsertEvent` | `subscriptionId`, `rows`, `row` |
| `UpdateEvent` | `subscriptionId`, `rows`, `oldRows`, `row`, `oldRow` |
| `DeleteEvent` | `subscriptionId`, `oldRows`, `row` |
| `SubscriptionError` | `subscriptionId`, `code`, `message` |

## License

Apache-2.0

## Links

- Website: [kalamdb.org](https://kalamdb.org)
- Docs: [kalamdb.org/docs](https://kalamdb.org/docs)
- SDK reference: [kalamdb.org/docs/sdk](https://kalamdb.org/docs/sdk)
- GitHub: [github.com/jamals86/KalamDB](https://github.com/jamals86/KalamDB)

---

> Native performance on iOS and Android is powered by the excellent [flutter_rust_bridge](https://cjycode.com/flutter_rust_bridge/) project — thank you to its maintainers and contributors.
