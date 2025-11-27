# Testing the TypeScript SDK

The TypeScript SDK wraps WASM bindings and is **browser-only**. Here's how to test it:

## Quick Test

1. **Make sure KalamDB server is running:**
   ```powershell
   # In another terminal
   cd C:\Jamal\git\KalamDB\backend
   cargo run
   ```

2. **Start the HTTP server:**
   ```powershell
   cd C:\Jamal\git\KalamDB\link\sdks\typescript
   npx http-server -p 3000
   ```

3. **Open in browser:**
   - Navigate to: http://localhost:3000/example.html
   - Click "Run All Tests" button
   - Watch the output console

## What the Example Tests

The `example.html` file tests:
- ✅ WASM initialization
- ✅ Simple SELECT query
- ✅ CREATE NAMESPACE
- ✅ CREATE TABLE  
- ✅ INSERT data (3 records)
- ✅ SELECT query with results

## Current Status

✅ **HTTP Queries Working** - The SDK can execute SQL via HTTP (fetch API)
⚠️ **WebSocket Subscriptions** - Not working yet due to authentication headers limitation in browser WebSocket API

The browser WebSocket API doesn't allow setting custom headers (like `Authorization: Basic ...`), which means real-time subscriptions need backend changes to support query parameter authentication or protocol-based auth.

## Unit Tests (Node.js)

Basic constructor and validation tests work in Node.js:

```powershell
npm test
```

These test:
- Constructor parameter validation
- Error handling
- Type safety

## Summary

- **Works**: SQL queries via HTTP in browser ✅
- **Works**: Constructor validation tests ✅
- **Not Working**: WebSocket subscriptions (auth limitation)
- **Not Working**: Node.js examples (browser-only WASM)

For Node.js server-side usage, use the Rust client or REST API directly.
