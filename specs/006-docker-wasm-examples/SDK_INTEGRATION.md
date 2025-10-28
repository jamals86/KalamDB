# SDK Integration Architecture

**Feature**: 006-docker-wasm-examples  
**Date**: 2025-10-26  
**Status**: UPDATED - Corrected example app to use SDK as dependency

## Problem Statement

The initial implementation created a mock `kalamClient.ts` in `examples/simple-typescript/src/services/kalamClient.ts`, which violated the architectural principle that:

> **SDKs at `link/sdks/{language}/` are complete, publishable packages that examples MUST use as dependencies**

This created code duplication and prevented the example from validating the real SDK.

## Solution

### 1. SDK as First-Class Package

The TypeScript SDK at `link/sdks/typescript/` is structured as an npm-publishable package:

```
link/sdks/typescript/
├── package.json           # npm package config (name: @kalamdb/client)
├── build.sh               # Compiles Rust → WASM
├── README.md              # Complete API documentation
├── .gitignore             # Excludes node_modules, build artifacts
├── src/                   # TypeScript wrapper code (if needed)
├── tests/                 # SDK test suite
│   └── basic.test.mjs     # 14 passing tests
└── [WASM artifacts]       # kalam_link.js, kalam_link.d.ts, kalam_link_bg.wasm
```

### 2. Example Uses SDK as Local Dependency

The React example at `examples/simple-typescript/` imports the SDK:

```json
// examples/simple-typescript/package.json
{
  "dependencies": {
    "@kalamdb/client": "file:../../link/sdks/typescript",
    "react": "^18.2.0",
    // ... other deps
  }
}
```

```typescript
// examples/simple-typescript/src/App.tsx
import { KalamClient } from '@kalamdb/client'; // From SDK!

const client = new KalamClient(config);
```

### 3. No Mock Implementations

The example app **does NOT** create its own client wrapper:

- ❌ **REMOVED**: `examples/simple-typescript/src/services/kalamClient.ts` (mock)
- ✅ **KEPT**: `examples/simple-typescript/src/services/localStorage.ts` (example-specific utilities)

### 4. SDK Extension for Missing Features

If the example needs functionality not in the SDK (e.g., `insertTodo`, `deleteTodo` helpers):

1. **Add to SDK**: Implement in `link/sdks/typescript/src/helpers.ts`
2. **Export from SDK**: Update `link/sdks/typescript/package.json` exports
3. **Import in Example**: Use from `@kalamdb/client`

This ensures all users benefit from improvements.

## Updated Tasks

### New Tasks Added to tasks.md

- **T064A**: Add `"@kalamdb/client": "file:../../link/sdks/typescript"` to `examples/simple-typescript/package.json`
- **T064B**: Verify `npm install` correctly links local SDK package
- **T072A**: Remove mock `kalamClient.ts` from example
- **T072B**: If SDK is missing `insertTodo`/`deleteTodo` helpers, add them to SDK (NOT example)

### Modified Tasks

- **T064**: Updated description to include "AND local SDK dependency"
- **T072**: Changed from "Create kalamClient.ts wrapper" to "Import from '@kalamdb/client'"
- **T073**: Changed to initialize client in App.tsx using SDK import

## Benefits

### For SDK Development
- ✅ Examples validate real SDK usability
- ✅ Force SDK to be complete and user-friendly
- ✅ Examples catch SDK bugs early

### For Users
- ✅ Examples demonstrate actual SDK usage
- ✅ Copy-paste code works with published SDK
- ✅ No confusion between "example code" and "real code"

### For Publishing
- ✅ SDK is ready to publish to npm without changes
- ✅ Examples prove SDK works in real applications
- ✅ No need to extract/refactor code before publishing

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────┐
│ link/ (Rust library)                                     │
│ ├── src/lib.rs                                           │
│ ├── src/wasm.rs                                          │
│ └── Cargo.toml                                           │
└─────────────────────────────────────────────────────────┘
                        │
                        │ build.sh (wasm-pack)
                        ▼
┌─────────────────────────────────────────────────────────┐
│ link/sdks/typescript/ (npm package)                      │
│ ├── package.json ("@kalamdb/client")                     │
│ ├── kalam_link.js                                        │
│ ├── kalam_link.d.ts                                      │
│ ├── kalam_link_bg.wasm                                   │
│ ├── tests/basic.test.mjs (14 tests)                      │
│ └── README.md (API docs)                                 │
└─────────────────────────────────────────────────────────┘
                        │
                        │ file:../../link/sdks/typescript
                        ▼
┌─────────────────────────────────────────────────────────┐
│ examples/simple-typescript/ (React app)                  │
│ ├── package.json (depends on @kalamdb/client)            │
│ ├── src/App.tsx (imports from '@kalamdb/client')         │
│ ├── src/services/localStorage.ts (example-specific)      │
│ └── NO kalamClient.ts (uses SDK!)                        │
└─────────────────────────────────────────────────────────┘
```

## Migration Checklist

For any example app using KalamDB:

- [ ] Add SDK as local dependency in `package.json`
- [ ] Remove any mock client implementations
- [ ] Import from `'@kalamdb/client'` instead
- [ ] If missing SDK features, add to SDK (not example)
- [ ] Run `npm install` to verify linking works
- [ ] Test that example works with real WASM client

## Future SDKs

This pattern applies to all language SDKs:

### Python SDK (Future)
```python
# link/sdks/python/setup.py
setup(name="kalamdb-client", ...)

# examples/python-example/requirements.txt
-e ../../link/sdks/python

# examples/python-example/app.py
from kalamdb_client import KalamClient  # From SDK!
```

### Rust SDK (Future)
```toml
# examples/rust-example/Cargo.toml
[dependencies]
kalamdb-link = { path = "../../link" }

# examples/rust-example/src/main.rs
use kalamdb_link::KalamClient;  // From SDK!
```

## References

- **Spec**: `specs/006-docker-wasm-examples/spec.md`
- **Plan**: `specs/006-docker-wasm-examples/plan.md` (updated)
- **Tasks**: `specs/006-docker-wasm-examples/tasks.md` (updated)
- **SDK README**: `link/sdks/typescript/README.md`
- **Example README**: `examples/simple-typescript/README.md` (needs update)
