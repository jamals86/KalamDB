# kalam-link

Official TypeScript/JavaScript client for KalamDB.

- Execute SQL over HTTP
- Subscribe to real-time changes over WebSocket
- Works in modern browsers and Node.js (>= 18)

## Installation

```bash
npm i kalam-link
```

## Quick Start

```ts
import { createClient, Auth, MessageType, ChangeType } from 'kalam-link';

const client = createClient({
  url: 'http://localhost:8080',
  auth: Auth.basic('admin', 'admin'),
});

await client.connect();

// Query
const result = await client.query('SELECT 1 AS ok');
console.log(result.results[0]);

// Parameterized query ($1, $2, ...)
const filtered = await client.query(
  'SELECT * FROM app.messages WHERE conversation_id = $1 AND is_deleted = $2',
  ['conv_42', false],
);
console.log(filtered.results[0]);

// Subscribe (returns an unsubscribe function)
const unsubscribe = await client.subscribe('app.messages', (event) => {
  switch (event.type) {
    case MessageType.Change:
      if (event.change_type === ChangeType.Insert) {
        console.log('New row:', event.rows);
      }
      break;
    case MessageType.InitialDataBatch:
      console.log('Initial data:', event.rows);
      break;
    case MessageType.Error:
      console.error('Subscription error:', event.message);
      break;
  }
});

// Later
await unsubscribe();
await client.disconnect();
```

## Authentication

```ts
import { createClient, Auth } from 'kalam-link';

// Username/password
createClient({ url: 'http://localhost:8080', auth: Auth.basic('user', 'pass') });

// JWT token
createClient({ url: 'http://localhost:8080', auth: Auth.jwt('eyJhbGciOiJIUzI1NiIs...') });

// No auth (for local/dev setups that allow it)
createClient({ url: 'http://localhost:8080', auth: Auth.none() });
```

## API Reference (Complete)

Everything below is exported from `kalam-link` unless noted otherwise.

### Imports

```ts
import KalamDBClient, {
  createClient,
  Auth,
  // Enums
  MessageType,
  ChangeType,
  BatchStatus,
  // Types
  type AuthCredentials,
  type BasicAuthCredentials,
  type JwtAuthCredentials,
  type NoAuthCredentials,
  type ClientOptions,
  type ClientOptionsWithAuth,
  type ClientOptionsLegacy,
  type ConnectionOptions,
  type SubscriptionOptions,
  type SubscribeOptions,
  type QueryResponse,
  type QueryResult,
  type SchemaField,
  type ErrorDetail,
  type ServerMessage,
  type BatchControl,
  type SubscriptionCallback,
  type SubscriptionInfo,
  type Unsubscribe,
  type WasmKalamClient,
  // Helpers
  buildAuthHeader,
  encodeBasicAuth,
  isAuthenticated,
  isBasicAuth,
  isJwtAuth,
  isNoAuth,
} from 'kalam-link';
```

### Factory

#### `createClient(options: ClientOptions): KalamDBClient`

```ts
type ClientOptions = ClientOptionsWithAuth | ClientOptionsLegacy;

interface ClientOptionsWithAuth {
  url: string;
  auth: AuthCredentials;
}

interface ClientOptionsLegacy {
  url: string;
  username: string;
  password: string;
}
```

### Class: `KalamDBClient` (default export)

#### Constructors

```ts
new KalamDBClient(options: ClientOptionsWithAuth)

// Legacy (deprecated)
new KalamDBClient(url: string, username: string, password: string)
```

#### Lifecycle

```ts
initialize(): Promise<void>
connect(): Promise<void>
disconnect(): Promise<void>
isConnected(): boolean
```

#### Authentication

```ts
getAuthType(): 'basic' | 'jwt' | 'none'
```

#### Queries

```ts
query(sql: string, params?: any[]): Promise<QueryResponse>
```

#### Convenience DML

```ts
insert(tableName: string, data: Record<string, any>): Promise<QueryResponse>
delete(tableName: string, rowId: string | number): Promise<void>
```

#### Subscriptions

```ts
subscribe(
  tableName: string,
  callback: SubscriptionCallback,
  options?: SubscriptionOptions
): Promise<Unsubscribe>

subscribeWithSql(
  sql: string,
  callback: SubscriptionCallback,
  options?: SubscriptionOptions
): Promise<Unsubscribe>

unsubscribe(subscriptionId: string): Promise<void>
unsubscribeAll(): Promise<void>
```

Subscription helpers:

```ts
getSubscriptionCount(): number
getSubscriptions(): SubscriptionInfo[]
isSubscribedTo(tableNameOrSql: string): boolean
getLastSeqId(subscriptionId: string): string | undefined
```

#### Reconnection controls

```ts
setAutoReconnect(enabled: boolean): void
setReconnectDelay(initialDelayMs: number, maxDelayMs: number): void
setMaxReconnectAttempts(maxAttempts: number): void
getReconnectAttempts(): number
isReconnecting(): boolean
```

### Auth API

#### Types

```ts
interface BasicAuthCredentials { type: 'basic'; username: string; password: string }
interface JwtAuthCredentials { type: 'jwt'; token: string }
interface NoAuthCredentials { type: 'none' }

type AuthCredentials =
  | BasicAuthCredentials
  | JwtAuthCredentials
  | NoAuthCredentials;
```

#### Factories

```ts
Auth.basic(username: string, password: string): BasicAuthCredentials
Auth.jwt(token: string): JwtAuthCredentials
Auth.none(): NoAuthCredentials
```

#### Helpers

```ts
encodeBasicAuth(username: string, password: string): string
buildAuthHeader(auth: AuthCredentials): string | undefined

isBasicAuth(auth: AuthCredentials): auth is BasicAuthCredentials
isJwtAuth(auth: AuthCredentials): auth is JwtAuthCredentials
isNoAuth(auth: AuthCredentials): auth is NoAuthCredentials
isAuthenticated(auth: AuthCredentials): auth is BasicAuthCredentials | JwtAuthCredentials
```

### Query result types

```ts
interface SchemaField {
  name: string;
  data_type: string;
  index: number;
  flags?: ("pk" | "nn" | "uq")[];
}

interface QueryResult {
  schema: SchemaField[];
  rows?: unknown[][];
  row_count: number;
  message?: string;
}

interface QueryResponse {
  status: 'success' | 'error';
  results: QueryResult[];
  took?: number;
  error?: ErrorDetail;
}

interface ErrorDetail {
  code: string;
  message: string;
  details?: any;
}
```

### Enums

```ts
// Message type for WebSocket subscription events
enum MessageType {
  SubscriptionAck = 'subscription_ack',
  InitialDataBatch = 'initial_data_batch',
  Change = 'change',
  Error = 'error',
}

// Change type for live subscription change events
enum ChangeType {
  Insert = 'insert',
  Update = 'update',
  Delete = 'delete',
}

// Batch loading status
enum BatchStatus {
  Loading = 'loading',
  LoadingBatch = 'loading_batch',
  Ready = 'ready',
}
```

### Live subscription event types

```ts
type ServerMessage =
  | {
      type: MessageType.SubscriptionAck;
      subscription_id: string;
      total_rows: number;
      batch_control: BatchControl;
      schema: SchemaField[];
    }
  | {
      type: MessageType.InitialDataBatch;
      subscription_id: string;
      rows: Record<string, any>[];
      batch_control: BatchControl;
    }
  | {
      type: MessageType.Change;
      subscription_id: string;
      change_type: ChangeType;
      rows?: Record<string, any>[];
      old_values?: Record<string, any>[];
    }
  | {
      type: MessageType.Error;
      subscription_id: string;
      code: string;
      message: string;
    };

interface BatchControl {
  batch_num: number;
  has_more: boolean;
  status: BatchStatus;
  last_seq_id?: string;
  snapshot_end_seq?: string;
}

type SubscriptionCallback = (event: ServerMessage) => void;
type Unsubscribe = () => Promise<void>;

interface SubscriptionInfo {
  id: string;
  tableName: string;
  createdAt: Date;
}
```

### Options

```ts
interface ConnectionOptions {
  auto_reconnect?: boolean;
  reconnect_delay_ms?: number;
  max_reconnect_delay_ms?: number;
  max_reconnect_attempts?: number;
}

interface SubscriptionOptions {
  batch_size?: number;
  last_rows?: number;
  from_seq_id?: string;
}

type SubscribeOptions = SubscriptionOptions;
```

### Advanced: WASM entrypoint (`kalam-link/wasm`)

```ts
import init, {
  KalamClient,
  WasmTimestampFormatter,
  parseIso8601,
  timestampNow,
  initSync,
} from 'kalam-link/wasm';
```

Exports:

- `default init(moduleOrPath?) => Promise<InitOutput>`
- `initSync(moduleOrBytes) => InitOutput`
- `class KalamClient` (low-level WASM client)
- `class WasmTimestampFormatter`
- `parseIso8601(isoString: string): number`
- `timestampNow(): number`

## Notes (Browser/Node)

This package includes a small `.wasm` runtime under the hood.

- In browsers with bundlers (Vite/Webpack/Rollup), it should “just work”.
- If you see WASM loading errors, ensure your build serves/copies:
  `node_modules/kalam-link/dist/wasm/kalam_link_bg.wasm`

## Links

- KalamDB repository: https://github.com/jamals86/KalamDB
- Issues: https://github.com/jamals86/KalamDB/issues

## Development (Build From This Repo)

Only needed if you are contributing to the SDK.

Prerequisites:

- Node.js `>=18`
- Rust toolchain + `wasm-pack`

```bash
cd link/sdks/typescript
npm install
npm run build
```

## License

Apache-2.0 (see the repository root).

## Contributing

Issues/PRs are welcome: https://github.com/jamals86/KalamDB/issues
