# kalam-link — Developer Notes

This document is for contributors and developers working on the `kalam-link` TypeScript/JavaScript SDK itself.
For usage documentation, see [README.md](README.md).

## Architecture

The TypeScript SDK is a thin JavaScript wrapper over a Rust core compiled to WebAssembly (WASM).

```
Application (TypeScript / JavaScript)
  └─ kalam-link (this package)           ← high-level API, auth, reconnect logic
      └─ dist/wasm/                       ← bundled WASM + JS bindings (auto-generated)
          └─ link/src/ (Rust)             ← core kalam-link library
              └─ link/src/wasm/           ← WASM entry points (wasm-pack)
```

`createClient(...)` auto-loads the bundled WASM bytes and applies runtime shims for both
Node.js and browser environments — no manual WASM bootstrap required for normal use.

## Prerequisites

| Tool | Version |
|------|---------|
| Node.js | >= 18 |
| npm | >= 9 |
| Rust toolchain | stable (via `rustup`) |
| `wasm-pack` | latest |

Install `wasm-pack`:

```bash
cargo install wasm-pack
```

## Build From Source

```bash
cd link/sdks/typescript
npm install
npm run build
```

The full `build` script runs these steps in order:

| Step | Command | Description |
|------|---------|-------------|
| 1 | `build:wasm` | Compile Rust → WASM with `wasm-pack` (web target, `wasm` feature) |
| 2 | `build:fix-types` | Patch `JsonValue` type into generated `.d.ts` |
| 3 | `build:ts` | Compile TypeScript → `dist/` |
| 4 | `build:copy-wasm` | Copy WASM artifacts into `dist/wasm/` |

Intermediate WASM output lands in `.wasm-out/` (gitignored). Compiled output is in `dist/`.

## Running Tests

### Unit / offline tests

Does not require a running server:

```bash
npm test
```

Runs `tests/basic.test.mjs`, `tests/normalize.test.mjs`, and `tests/agent-runtime.test.mjs`
with `NO_SERVER=true`.

### Agent runtime tests only

```bash
npm run test:agent-runtime
```

### Live server integration tests

Requires a running KalamDB instance. Set connection env vars before running:

```bash
KALAM_URL=http://localhost:8080 \
KALAM_USER=admin \
KALAM_PASS=kalamdb123 \
node --test tests/integration.test.mjs
```

## Low-Level WASM Entrypoint

For cases where you need direct access to the raw WASM API (e.g. custom timestamp formatting,
low-level client construction):

```ts
import init, {
  KalamClient,
  WasmTimestampFormatter,
  parseIso8601,
  timestampNow,
  initSync,
} from 'kalam-link/wasm';
```

Most applications should use the high-level `kalam-link` exports instead.

## Publishing

Update `version` in `package.json`, add a `CHANGELOG` entry, then:

```bash
cd link/sdks/typescript
npm run build
npm publish
```

For a dry run:

```bash
npm publish --dry-run
```

## Crate Layout

| Path | Purpose |
|------|---------|
| `link/src/` | Core Rust client library (HTTP, WebSocket, auth, reconnect) |
| `link/src/wasm/` | WASM entry points (`#[wasm_bindgen]` annotations) |
| `link/sdks/typescript/src/` | TypeScript API layer (`client.ts`, `auth.ts`, etc.) |
| `link/sdks/typescript/dist/` | Compiled output (gitignored) |
| `link/sdks/typescript/.wasm-out/` | wasm-pack output (gitignored) |
