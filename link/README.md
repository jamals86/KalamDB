# kalam-link

Rust client library for KalamDB with WebAssembly support and multi-language SDKs.

## Project Structure

```
link/
├── src/                      # Rust source code
│   ├── lib.rs               # Library entry point
│   ├── wasm.rs              # WASM bindings
│   ├── client.rs            # Native Rust client (used by CLI)
│   ├── models.rs            # Data models
│   └── ...
├── tests/                    # Rust crate tests
├── Cargo.toml               # Rust package configuration
├── README.md                # This file
└── sdks/                    # Multi-language SDK directory
    └── typescript/          # TypeScript/JavaScript SDK (npm-publishable)
      ├── package.json     # npm package: kalam-link
        ├── build.sh         # Compiles Rust → WASM
        ├── README.md        # Complete SDK documentation
        ├── tests/           # 14 passing tests
        ├── .gitignore       # Excludes node_modules
        ├── kalam_link.js    # WASM bindings (37 KB)
        ├── kalam_link.d.ts  # TypeScript definitions
        └── kalam_link_bg.wasm  # Compiled WASM module
```

## SDK Architecture Principles

**SDKs as First-Class Packages**:
- Each language SDK in `sdks/{language}/` is a complete, publishable package
- SDKs include: build system, tests, docs, package config, .gitignore
- Examples import SDKs as local dependencies (e.g., `"kalam-link": "file:../../link/sdks/typescript"`)
- **Examples MUST NOT implement their own clients** - all functionality comes from SDKs
- If examples need features, add them to the SDK for all users

**Benefits**:
- ✅ Examples validate real SDK usability
- ✅ No code duplication between examples  
- ✅ SDKs ready to publish without modification
- ✅ Improvements benefit all users immediately

See [SDK Integration Guide](../specs/006-docker-wasm-examples/SDK_INTEGRATION.md) for detailed architecture.

## Features

- 🦀 **Dual-mode library**: Use natively in Rust or compile to WebAssembly for JavaScript/TypeScript
- 🔐 **HTTP Basic Auth & JWT**: Secure authentication for all API requests
- 🔄 **Real-time subscriptions**: Subscribe to table changes with WebSocket support
- 📊 **SQL queries**: Execute SQL queries and get results
- 🌐 **Cross-platform**: Works in native Rust applications, browsers, and Node.js
- 🌍 **Multi-language SDKs**: Official SDKs for different languages

## Installation

### Native Rust Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
kalam-link = { path = "../link" }
```

### TypeScript/JavaScript SDK

The TypeScript SDK is a complete, npm-publishable package at `sdks/typescript/`:

**Installation** (as local dependency in examples):
```json
{
  "dependencies": {
    "kalam-link": "file:../../link/sdks/typescript"
  }
}
```

**Building the SDK**:
```bash
cd link/sdks/typescript
npm install
npm run build  # Compiles Rust → WASM (wasm-pack) and builds TypeScript into dist/
```

**Testing**:
```bash
npx http-server -p 3000
# Open http://localhost:3000/tests/browser-test.html
```

**Usage**:
```typescript
import { createClient, Auth } from 'kalam-link';

const client = createClient({
  url: 'http://localhost:8080',
  auth: Auth.basic('username', 'password')
});

// Connect and query
await client.connect();
const result = await client.query('SELECT * FROM todos');
console.log(result.results[0]?.rows);
```

**Complete Documentation**: See [sdks/typescript/README.md](sdks/typescript/README.md) for full API reference, examples, and troubleshooting.

## Usage

### Native Rust

```rust
use kalam_link::client::KalamClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = KalamClient::new("http://localhost:8080", "your-api-key")?;
    
    // Insert data
    client.insert("users", serde_json::json!({
        "name": "Alice",
        "email": "alice@example.com"
    })).await?;
    
    // Query data
    let results = client.query("SELECT * FROM users WHERE name = 'Alice'").await?;
    println!("Results: {:?}", results);
    
    Ok(())
}
```

### WebAssembly - Node.js

```javascript
import { readFile } from 'fs/promises';
import init, { KalamClient } from './pkg/kalam_link.js';

// Initialize WASM module
const wasmBuffer = await readFile('./pkg/kalam_link_bg.wasm');
await init(wasmBuffer);

// Create client
const client = new KalamClient('http://localhost:8080', 'username', 'password');

// Connect to server
await client.connect();

// Insert data
await client.insert('users', JSON.stringify({
  name: 'Alice',
  email: 'alice@example.com'
}));

// Query data
const results = await client.query("SELECT * FROM users WHERE name = 'Alice'");
console.log('Results:', results);

// Disconnect
await client.disconnect();
```

### WebAssembly - Browser

```html
<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <title>KalamDB Browser Example</title>
</head>
<body>
  <script type="module">
    import init, { KalamClient } from './pkg/kalam_link.js';

    // Initialize WASM module
    await init();

    // Create client
    const client = new KalamClient('http://localhost:8080', 'username', 'password');

    // Connect to server
    await client.connect();

    // Insert data
    await client.insert('users', JSON.stringify({
      name: 'Alice',
      email: 'alice@example.com'
    }));

    // Query data
    const results = await client.query("SELECT * FROM users WHERE name = 'Alice'");
    console.log('Results:', results);

    // Subscribe to changes
    const subscriptionId = await client.subscribe('users', (event) => {
      console.log('Table changed:', event);
    });

    // Later: Unsubscribe
    await client.unsubscribe(subscriptionId);

    // Disconnect
    await client.disconnect();
  </script>
</body>
</html>
```

### TypeScript Support

The WASM build includes TypeScript definitions (`kalam_link.d.ts`):

```typescript
import init, { KalamClient } from './pkg/kalam_link.js';

// TypeScript knows the types!
const client: KalamClient = new KalamClient(
  'http://localhost:8080',
  'username', 'password'
);

// Methods are fully typed
const isConnected: boolean = client.isConnected();
```

## API Reference

### `KalamClient`

#### Constructor

```rust
new KalamClient(url: string, username, password: string)
```

Creates a new KalamDB client.

**Parameters:**
- `url` - Server URL (e.g., `http://localhost:8080`)
- `username, password` - API key for authentication

**Throws:**
- Error if `url` or `username, password` is empty

**Example:**
```javascript
const client = new KalamClient('http://localhost:8080', 'my-api-key');
```

#### Connection Methods

##### `connect()`

```rust
async connect() -> Promise<void>
```

Establishes connection to the KalamDB server.

##### `disconnect()`

```rust
async disconnect() -> Promise<void>
```

Closes the connection to the server.

##### `isConnected()`

```rust
isConnected() -> boolean
```

Returns `true` if currently connected, `false` otherwise.

#### Data Methods

##### `insert()`

```rust
async insert(table_name: string, data: string) -> Promise<string>
```

Inserts a row into a table.

**Parameters:**
- `table_name` - Name of the table
- `data` - JSON string containing the data to insert

**Returns:** Response from the server

##### `delete()`

```rust
async delete(table_name: string, row_id: string) -> Promise<string>
```

Deletes a row from a table.

**Parameters:**
- `table_name` - Name of the table
- `row_id` - ID of the row to delete

**Returns:** Response from the server

##### `query()`

```rust
async query(sql: string) -> Promise<string>
```

Executes a SQL query.

**Parameters:**
- `sql` - SQL query string

**Returns:** JSON string containing query results

#### Subscription Methods

##### `subscribe()`

```rust
async subscribe(table_name: string, callback: Function) -> Promise<string>
```

Subscribes to changes in a table.

**Parameters:**
- `table_name` - Name of the table to subscribe to
- `callback` - Function called when the table changes

**Returns:** Subscription ID

##### `unsubscribe()`

```rust
async unsubscribe(subscription_id: string) -> Promise<void>
```

Unsubscribes from a table.

**Parameters:**
- `subscription_id` - ID returned from `subscribe()`

## Feature Flags

The library supports two mutually exclusive feature sets:

### `tokio-runtime` (default)

For native Rust applications. Includes:
- `tokio` - Async runtime
- `reqwest` - HTTP client
- `tokio-tungstenite` - WebSocket client

**Build:**
```bash
cargo build  # Uses default features
```

### `wasm`

For WebAssembly (browser/Node.js). Includes:
- `wasm-bindgen` - Rust/JS interop
- `wasm-bindgen-futures` - Async support
- `js-sys` - JavaScript global APIs
- `web-sys` - Web APIs
- `getrandom` with "js" feature - Random number generation

**Build:**
```bash
wasm-pack build --target web --features wasm --no-default-features
```

## Testing

### Native Tests

```bash
cargo test
```

### WASM Tests (Node.js)

```bash
# Build WASM first
wasm-pack build --target web --out-dir pkg --features wasm --no-default-features

# Run Node.js tests
node test-wasm.mjs
```

Expected output:
```
🧪 Testing kalam-link WASM module...

✅ WASM module initialized successfully
✅ KalamClient created successfully
✅ client.connect() succeeded
✅ client.disconnect() succeeded
✅ Correctly rejected empty URL
✅ Correctly rejected empty API key

🎉 All WASM tests passed!
```

## Development

### Project Structure

```
kalam-link/
├── Cargo.toml              # Package manifest with feature flags
├── README.md               # This file
├── src/
│   ├── lib.rs              # Library root with conditional modules
│   ├── client.rs           # Native Rust client (tokio-runtime)
│   ├── auth.rs             # Authentication (tokio-runtime)
│   ├── query.rs            # Query execution (tokio-runtime)
│   ├── subscription.rs     # WebSocket subscriptions (tokio-runtime)
│   ├── error.rs            # Error types (conditional conversions)
│   └── wasm.rs             # WASM bindings (wasm feature)
├── pkg/                    # WASM build output (generated)
└── test-wasm.mjs           # Node.js WASM test script
```

### Building for Different Targets

**Native (CLI usage):**
```bash
cargo build --release
```

**WASM (web target):**
```bash
wasm-pack build --target web --features wasm --no-default-features
```

**WASM (Node.js target):**
```bash
wasm-pack build --target nodejs --features wasm --no-default-features
```

**WASM (bundler target for Webpack/Rollup):**
```bash
wasm-pack build --target bundler --features wasm --no-default-features
```

## License

See the main KalamDB repository for license information.

## Contributing

See the main KalamDB repository for contribution guidelines.

