#+#+#+#+

# kalam-link (TypeScript SDK)

This folder (`link/sdks/typescript`) is the npm-publishable **TypeScript/JavaScript SDK** for KalamDB.

It is built from the Rust crate at `link/` (crate name: `kalam-link`) using `wasm-pack`.

## Build From Source (This Repo)

### Prerequisites

- Rust toolchain
- Node.js `>=18`
- `wasm-pack` (Rust → WASM build tool)

Install wasm-pack:

```bash
cargo install wasm-pack
```

### Build

From the repo root:

```bash
cd link/sdks/typescript
npm install
npm run build
```

This will:

1. Compile the Rust crate (`link/`) to WASM via `wasm-pack`
2. Copy generated WASM artifacts into `dist/wasm/`
3. Compile TypeScript into `dist/`

### Output

After a successful build:

- `dist/index.js` / `dist/index.d.ts`
- `dist/wasm/kalam_link.js` / `dist/wasm/kalam_link.d.ts`
- `dist/wasm/kalam_link_bg.wasm`

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
- 🌐 **Cross-platform**: Browser-first WASM SDK (Node.js usage depends on your WASM + fetch environment)
- 🌍 **Multi-language SDKs**: Official SDKs for different languages

## Installation

## Running Locally (Quick Sanity Check)

1. Start the server (separate terminal):

```bash
cd backend
cargo run
```

2. Build the SDK:

```bash
cd link/sdks/typescript
npm run build
```

3. Serve the folder and open the browser test page:

```bash
npx http-server -p 3000
```

Open `http://localhost:3000/tests/browser-test.html`.

For a step-by-step guide to build and run the example app, see `example/README.md`.

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

