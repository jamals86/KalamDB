# TypeScript SDK - Build & Test Checklist

## ✅ Implementation Complete

The TypeScript SDK has been fully implemented. Follow these steps to build and test it.

## Prerequisites

- [ ] Rust toolchain installed (for wasm-pack)
- [ ] Node.js 18+ installed
- [ ] wasm-pack installed: `cargo install wasm-pack`
- [ ] KalamDB server running (for integration tests)

## Build Steps

### 1. Install Dependencies

```powershell
cd link\sdks\typescript
npm install
```

### 2. Build WASM and TypeScript

**Option A - Windows (PowerShell):**
```powershell
.\build.ps1
```

**Option B - Cross-platform (via npm):**
```powershell
npm run build
```

**Option C - Linux/macOS/WSL (Bash):**
```bash
./build.sh
```

Expected output files:
- ✅ `kalam_link.js` (WASM bindings)
- ✅ `kalam_link.d.ts` (WASM type definitions)
- ✅ `kalam_link_bg.wasm` (WebAssembly module)
- ✅ `dist/index.js` (Compiled TypeScript client)
- ✅ `dist/index.d.ts` (TypeScript type definitions)

## Testing Steps

### 1. Basic Tests (No Server Required)

```powershell
npm test
```

Expected: All tests pass ✅

### 2. Type Checking

```powershell
npm run test:types
```

Expected: No TypeScript errors ✅

### 3. Integration Tests (Server Required)

**Terminal 1 - Start Server:**
```powershell
cd backend
cargo run
```

**Terminal 2 - Run Tests:**
```powershell
cd link\sdks\typescript
npm run test:all
```

Expected: All tests pass, including subscriptions ✅

### 4. Run Example

```powershell
npm run example
```

Expected output:
```
🚀 KalamDB TypeScript SDK Example

Connecting to: http://localhost:8080
User: root

📡 Connecting...
✅ Connected!

📦 Creating namespace...
✅ Namespace created

📋 Creating table...
✅ Table created

➕ Inserting todos...
✅ Inserted 3 todos

🔍 Querying todos...
Found 3 todos:

  1. [○] HIGH   Buy groceries
  2. [○] HIGH   Review pull requests
  3. [✓] MEDIUM Write documentation

👂 Subscribing to real-time changes...
✅ Subscription active (3 total rows)

➕ Adding a new todo (will trigger subscription event)...
🆕 New todo added:
   - Test real-time subscription [low]

📝 Updating a todo (will trigger subscription event)...
📝 Todo updated:
   - Buy groceries [DONE]

👋 Unsubscribing...
✅ Unsubscribed

🔌 Disconnecting...
✅ Disconnected

🎉 Example completed successfully!
```

## Troubleshooting

### Build Issues

**"wasm-pack: command not found"**
```powershell
cargo install wasm-pack
```

**"npx: command not found"**
```powershell
npm install
```

**TypeScript compilation errors**
- Check that `node_modules` exists
- Run `npm install` again
- Verify Node.js version: `node --version` (should be 18+)

### Test Issues

**"Cannot find module '../dist/index.js'"**
- Run `npm run build` first to compile TypeScript

**Integration tests fail with "ECONNREFUSED"**
- Ensure KalamDB server is running on `http://localhost:8080`
- Check server logs for errors
- Try: `cd backend && cargo run`

**"WebSocket connection failed"**
- Verify server is accessible
- Check firewall settings
- Ensure WebSocket endpoint is available at `/v1/ws`

### Example Issues

**"Authentication failed"**
- Default credentials: `root` / `root`
- Set environment variables:
  ```powershell
  $env:KALAMDB_USER = "root"
  $env:KALAMDB_PASSWORD = "root"
  ```

## Verification Checklist

After building and testing, verify:

- [ ] All WASM files generated successfully
- [ ] TypeScript compiled without errors
- [ ] Basic tests pass
- [ ] Type tests pass (no TypeScript errors)
- [ ] Integration tests pass (with server)
- [ ] Example runs successfully
- [ ] Subscriptions work (events received)
- [ ] No console errors or warnings

## Publishing Checklist

Before publishing to npm:

- [ ] Version number updated in `package.json`
- [ ] README.md is complete
- [ ] LICENSE file included
- [ ] All tests passing
- [ ] WASM files built and included
- [ ] TypeScript compiled
- [ ] No sensitive data in files
- [ ] `.gitignore` excludes `node_modules`

Publish command:
```powershell
npm publish --access public
```

## Using in Examples

To use this SDK in an example project:

**In example's `package.json`:**
```json
{
  "dependencies": {
    "@kalamdb/client": "file:../../link/sdks/typescript"
  }
}
```

**Then:**
```powershell
cd examples\your-example
npm install
```

The example will use the local SDK directly!

## Summary

✅ SDK implementation complete
✅ Build scripts ready (Windows + Linux/macOS)
✅ Comprehensive tests included
✅ Documentation complete
✅ Example usage provided

**Next Steps:**
1. Run `npm install` to install dependencies
2. Run `npm run build` to build WASM and TypeScript
3. Run `npm test` to verify basic functionality
4. Start server and run `npm run test:all` for full verification
5. Run `npm run example` to see it in action!
