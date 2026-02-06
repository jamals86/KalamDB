/**
 * kalam-link — Official TypeScript/JavaScript client for KalamDB
 *
 * @example
 * ```typescript
 * import { createClient, Auth } from 'kalam-link';
 *
 * const client = createClient({
 *   url: 'http://localhost:8080',
 *   auth: Auth.basic('admin', 'admin'),
 * });
 *
 * await client.login();
 * await client.connect();
 *
 * const unsub = await client.subscribe('messages', (event) => {
 *   console.log('Change:', event);
 * });
 *
 * await unsub();
 * await client.disconnect();
 * ```
 *
 * @packageDocumentation
 */

/* ------------------------------------------------------------------ */
/*  Re-exports                                                        */
/* ------------------------------------------------------------------ */

// Auth
export {
  Auth,
  buildAuthHeader,
  encodeBasicAuth,
  isAuthenticated,
  isBasicAuth,
  isJwtAuth,
  isNoAuth,
} from './auth.js';

export type {
  AuthCredentials,
  BasicAuthCredentials,
  JwtAuthCredentials,
  NoAuthCredentials,
} from './auth.js';

// Types & enums
export {
  ChangeType,
  MessageType,
} from './types.js';

export type {
  AckResponse,
  BatchControl,
  BatchStatus,
  ChangeTypeRaw,
  ClientOptions,
  ConsumeContext,
  ConsumerHandle,
  ConsumerHandler,
  ConsumeMessage,
  ConsumeRequest,
  ConsumeResponse,
  ErrorDetail,
  HealthCheckResponse,
  HttpVersion,
  JsonValue,
  KalamDataType,
  LoginResponse,
  LoginUserInfo,
  QueryResponse,
  QueryResult,
  ResponseStatus,
  SchemaField,
  SeqId,
  ServerMessage,
  SubscriptionCallback,
  SubscriptionInfo,
  SubscriptionOptions,
  TimestampFormat,
  TypedSubscriptionCallback,
  Unsubscribe,
  UploadProgress,
} from './types.js';

// Client
export { createClient, KalamDBClient } from './client.js';

// Query helpers
export {
  normalizeQueryResponse,
  parseRows,
  sortColumns,
  SYSTEM_TABLES_ORDER,
} from './helpers/query_helpers.js';

// WASM bindings (re-exported so advanced users can access low-level API)
export type { KalamClient as WasmKalamClient } from '../.wasm-out/kalam_link.js';

// Default export
export { KalamDBClient as default } from './client.js';
