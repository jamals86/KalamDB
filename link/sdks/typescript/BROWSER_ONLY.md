# TypeScript SDK - Browser Only

⚠️ **IMPORTANT**: This TypeScript SDK wraps the WASM bindings and is **designed for browser environments only**.

## Why Browser Only?

The WASM bindings use browser-specific APIs:
- `window.fetch()` for HTTP requests  
- `WebSocket` API for real-time subscriptions
- Browser DOM APIs

These APIs **do not exist in Node.js**.

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
    import { KalamDBClient } from './node_modules/@kalamdb/client/dist/index.js';

    async function main() {
      const client = new KalamDBClient(
        'http://localhost:8080',
        'root',
        'root'
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

**Do NOT use this SDK in Node.js**. Instead, use the native Rust client via the CLI:

```bash
# Use the CLI
cd cli
cargo run -- --help

# Or build a native Node.js addon if needed
```

## Testing

The tests are designed to work without actually connecting (constructor validation, etc.):

```bash
npm test  # Basic tests (no server needed)
```

Integration tests that require a server are **skipped by default** because they would need browser environment.

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
