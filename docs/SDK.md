# KalamDB SDK (Under Development)

KalamDB will ship a small TypeScript/JavaScript SDK built on top of a Rust → WASM core.

This SDK is **not stable yet** and the API may change.

## Goals

- Simple client for browsers and Node.js
- First-class support for:
  - Running SQL over HTTP
  - Managing namespaces and tables
  - Subscribing to live updates over WebSocket
- Tiny bundle size and minimal dependencies

## Planned usage (sketch)

```ts
import init, { KalamClient } from '@kalamdb/client';

await init();

const client = new KalamClient('http://localhost:8080', {
  username: 'myuser',
  password: 'Secret123!',
});

await client.connect();
const rows = await client.query('SELECT * FROM app.messages LIMIT 10;');

await client.subscribe(
  "SUBSCRIBE TO app.messages WHERE user_id = 'alice' OPTIONS (last_rows = 10);",
  (eventJson) => {
    const event = JSON.parse(eventJson);
    console.log('Change detected:', event);
  }
);
```

## Status

- Core design in progress
- WASM build pipeline being wired into the repo
- NPM packaging and versioning not finalized yet

When the SDK is ready, this file will be updated with the final API and installation instructions.
