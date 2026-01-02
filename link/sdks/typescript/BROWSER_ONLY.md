# TypeScript SDK - Browser First

This TypeScript SDK wraps the WASM bindings and is designed and tested primarily for **browser environments**.

## Why Browser First?

The WASM build is produced using `wasm-pack --target web`, which integrates naturally with browsers.

Node.js support depends on your runtime setup for WebAssembly modules and networking APIs.

## Usage

### ✅ For Browsers

```html
<!DOCTYPE html>
<html>
<head>
  <title>KalamDB Example</title>
</head>
<body>
  <h1>KalamDB Browser Client</h1>
  <div id="output"></div>

  <script type="module">
    import { KalamDBClient } from './node_modules/kalam-link/dist/index.js';

    async function main() {
      const client = new KalamDBClient(
        'http://localhost:8080',
        'root',
        ''
      );

      // Initialize (WASM loads automatically in browser)
      await client.initialize();

      // Execute query
      const result = await client.query('SELECT CURRENT_USER()');
      document.getElementById('output').innerHTML = `
        <pre>${JSON.stringify(result, null, 2)}</pre>
      `;
    }

    main().catch(console.error);
  </script>
</body>
</html>
```

### ❌ For Node.js

If you need a server-side client today, prefer the native Rust client or call the REST API directly.

```bash
# Use the CLI
cd cli
cargo run -- --help

# Or build a native Node.js addon if needed
```

## Testing

See [TESTING.md](TESTING.md) for the recommended browser-based test flow.

## Alternative for Node.js

If you need a Node.js client, you have these options:

1. **Use the REST API directly** with `fetch` or `axios`
2. **Use the Rust CLI** as a subprocess
3. **Create a native Node.js addon** wrapping the Rust client

## Summary

- ✅ **Browser**: Use this TypeScript SDK
- ❌ **Node.js**: Use CLI or REST API directly
- 🔧 **Server-side**: Use Rust client library directly

See the main [README](README.md) for browser usage examples.
