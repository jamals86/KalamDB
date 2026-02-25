# kalam_link

KalamDB client SDK for Dart and Flutter — queries, live subscriptions, and authentication powered by [flutter_rust_bridge](https://cjycode.com/flutter_rust_bridge/).

## Features

- **SQL Queries** — execute parameterized SQL with `$1`, `$2` placeholders
- **Live Subscriptions** — real-time change streams via WebSocket (insert, update, delete events)
- **Authentication** — HTTP Basic Auth, JWT tokens, or anonymous access
- **Cross-platform** — iOS, Android, macOS, Windows, Linux, and Web (WASM)
- **Zero-copy bridge** — native Rust performance via flutter_rust_bridge v2

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

## Setup

### Prerequisites

- Flutter SDK >= 3.10
- Rust toolchain (stable)
- `flutter_rust_bridge_codegen` CLI

### Build

```bash
# Install the FRB codegen tool
dart pub global activate flutter_rust_bridge

# Generate Dart bindings from Rust
cd link/kalam-link-dart
flutter_rust_bridge_codegen generate

# Run your Flutter app
cd link/sdks/dart
flutter pub get
flutter run
```

## Architecture

```
Flutter App
  └─ kalam_link (Dart package)      ← this package
      └─ Generated FRB bindings      ← auto-generated
          └─ kalam-link-dart (Rust)  ← bridge crate
              └─ kalam-link          ← core client library
```

The Dart SDK wraps the existing `kalam-link` Rust library through a bridge crate that provides flutter_rust_bridge-annotated functions. The FRB codegen tool generates Dart bindings that handle FFI, async dispatch, and type marshalling automatically.

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
