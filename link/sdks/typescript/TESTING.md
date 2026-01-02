# Testing the TypeScript SDK

The TypeScript SDK wraps WASM bindings and is primarily tested in the browser.

## Quick Test (Recommended)

For a full step-by-step guide to build and run the example app, see:

- `example/README.md`

1. **Make sure KalamDB server is running:**
   ```bash
   cd backend
   cargo run
   ```

2. **Build the SDK:**

   ```bash
   cd link/sdks/typescript
   npm install
   npm run build
   ```

3. **Start a static server:**
   ```bash
   npx http-server -p 3000
   ```

4. **Open in browser:**
   - Navigate to: `http://localhost:3000/tests/browser-test.html`
   - Click "Run All Tests" button
   - Watch the output console

If you prefer a richer UI (and a self-contained static app), run the example:

```bash
cd link/sdks/typescript/example
npm install
npm run start
```

## What the Example Tests

The `example.html` file tests:
- ✅ WASM initialization
- ✅ Simple SELECT query
- ✅ CREATE NAMESPACE
- ✅ CREATE TABLE  
- ✅ INSERT data (3 records)
- ✅ SELECT query with results

## Current Status

✅ **HTTP Queries Working** - The SDK can execute SQL via HTTP.

Subscriptions are supported via the WebSocket protocol (auth happens via protocol messages after connect, not via headers).

## Summary

- **Works**: SQL queries via HTTP in browser ✅
- **Works**: Browser-based test harness ✅

For server-side usage, prefer the Rust client (`kalam-link`) or call the REST API directly.
