/* tslint:disable */
/* eslint-disable */

export class KalamClient {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Create a new KalamDB client with HTTP Basic Authentication (T042, T043, T044)
   *
   * # Arguments
   * * `url` - KalamDB server URL (required, e.g., "http://localhost:8080")
   * * `username` - Username for authentication (required)
   * * `password` - Password for authentication (required)
   *
   * # Errors
   * Returns JsValue error if url, username, or password is empty
   */
  constructor(url: string, username: string, password: string);
  /**
   * Create a new KalamDB client with JWT Token Authentication
   *
   * # Arguments
   * * `url` - KalamDB server URL (required, e.g., "http://localhost:8080")
   * * `token` - JWT token for authentication (required)
   *
   * # Errors
   * Returns JsValue error if url or token is empty
   *
   * # Example (JavaScript)
   * ```js
   * const client = KalamClient.withJwt(
   *   "http://localhost:8080",
   *   "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..."
   * );
   * await client.connect();
   * ```
   */
  static withJwt(url: string, token: string): KalamClient;
  /**
   * Create a new KalamDB client with no authentication
   *
   * Useful for localhost connections where the server allows
   * unauthenticated access, or for development/testing scenarios.
   *
   * # Arguments
   * * `url` - KalamDB server URL (required, e.g., "http://localhost:8080")
   *
   * # Errors
   * Returns JsValue error if url is empty
   *
   * # Example (JavaScript)
   * ```js
   * const client = KalamClient.anonymous("http://localhost:8080");
   * await client.connect();
   * ```
   */
  static anonymous(url: string): KalamClient;
  /**
   * Get the current authentication type
   *
   * Returns one of: "basic", "jwt", or "none"
   */
  getAuthType(): string;
  /**
   * Enable or disable automatic reconnection
   *
   * # Arguments
   * * `enabled` - Whether to automatically reconnect on connection loss
   */
  setAutoReconnect(enabled: boolean): void;
  /**
   * Set reconnection delay parameters
   *
   * # Arguments
   * * `initial_delay_ms` - Initial delay in milliseconds between reconnection attempts
   * * `max_delay_ms` - Maximum delay (for exponential backoff)
   */
  setReconnectDelay(initial_delay_ms: bigint, max_delay_ms: bigint): void;
  /**
   * Set maximum reconnection attempts
   *
   * # Arguments
   * * `max_attempts` - Maximum number of attempts (0 = infinite)
   */
  setMaxReconnectAttempts(max_attempts: number): void;
  /**
   * Get the current reconnection attempt count
   */
  getReconnectAttempts(): number;
  /**
   * Check if currently reconnecting
   */
  isReconnecting(): boolean;
  /**
   * Get the last received seq_id for a subscription
   *
   * Useful for debugging or manual resumption tracking
   */
  getLastSeqId(subscription_id: string): string | undefined;
  /**
   * Connect to KalamDB server via WebSocket (T045, T063C-T063D)
   *
   * # Returns
   * Promise that resolves when connection is established and authenticated
   */
  connect(): Promise<void>;
  /**
   * Disconnect from KalamDB server (T046, T063E)
   */
  disconnect(): Promise<void>;
  /**
   * Check if client is currently connected (T047)
   *
   * # Returns
   * true if WebSocket connection is active, false otherwise
   */
  isConnected(): boolean;
  /**
   * Insert data into a table (T048, T063G)
   *
   * # Arguments
   * * `table_name` - Name of the table to insert into
   * * `data` - JSON string representing the row data
   *
   * # Example (JavaScript)
   * ```js
   * await client.insert("todos", JSON.stringify({
   *   title: "Buy groceries",
   *   completed: false
   * }));
   * ```
   */
  insert(table_name: string, data: string): Promise<string>;
  /**
   * Delete a row from a table (T049, T063H)
   *
   * # Arguments
   * * `table_name` - Name of the table
   * * `row_id` - ID of the row to delete
   */
  delete(table_name: string, row_id: string): Promise<void>;
  /**
   * Execute a SQL query (T050, T063F)
   *
   * # Arguments
   * * `sql` - SQL query string
   *
   * # Returns
   * JSON string with query results
   *
   * # Example (JavaScript)
   * ```js
   * const result = await client.query("SELECT * FROM todos WHERE completed = false");
   * const data = JSON.parse(result);
   * ```
   */
  query(sql: string): Promise<string>;
  /**
   * Execute a SQL query with parameters
   *
   * # Arguments
   * * `sql` - SQL query string with placeholders ($1, $2, ...)
   * * `params` - JSON array string of parameter values
   *
   * # Returns
   * JSON string with query results
   *
   * # Example (JavaScript)
   * ```js
   * const result = await client.queryWithParams(
   *   "SELECT * FROM users WHERE id = $1 AND age > $2",
   *   JSON.stringify([42, 18])
   * );
   * const data = JSON.parse(result);
   * ```
   */
  queryWithParams(sql: string, params?: string | null): Promise<string>;
  /**
   * Subscribe to table changes (T051, T063I-T063J)
   *
   * # Arguments
   * * `table_name` - Name of the table to subscribe to
   * * `callback` - JavaScript function to call when changes occur
   *
   * # Returns
   * Subscription ID for later unsubscribe
   */
  subscribe(table_name: string, callback: Function): Promise<string>;
  /**
   * Subscribe to a SQL query with optional subscription options
   *
   * # Arguments
   * * `sql` - SQL SELECT query to subscribe to
   * * `options` - Optional JSON string with subscription options:
   *   - `batch_size`: Number of rows per batch (default: server-configured)
   *   - `auto_reconnect`: Override client auto-reconnect for this subscription (default: true)
   *   - `include_old_values`: Include old values in UPDATE/DELETE events (default: false)
   *   - `resume_from_seq_id`: Resume from a specific sequence ID (internal use)
   * * `callback` - JavaScript function to call when changes occur
   *
   * # Returns
   * Subscription ID for later unsubscribe
   *
   * # Example (JavaScript)
   * ```js
   * // Subscribe with options
   * const subId = await client.subscribeWithSql(
   *   "SELECT * FROM chat.messages WHERE conversation_id = 1",
   *   JSON.stringify({ batch_size: 50, include_old_values: true }),
   *   (event) => console.log('Change:', event)
   * );
   * ```
   */
  subscribeWithSql(sql: string, options: string | null | undefined, callback: Function): Promise<string>;
  /**
   * Unsubscribe from table changes (T052, T063M)
   *
   * # Arguments
   * * `subscription_id` - ID returned from subscribe()
   */
  unsubscribe(subscription_id: string): Promise<void>;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
  readonly memory: WebAssembly.Memory;
  readonly __wbg_kalamclient_free: (a: number, b: number) => void;
  readonly kalamclient_new: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number];
  readonly kalamclient_withJwt: (a: number, b: number, c: number, d: number) => [number, number, number];
  readonly kalamclient_anonymous: (a: number, b: number) => [number, number, number];
  readonly kalamclient_getAuthType: (a: number) => [number, number];
  readonly kalamclient_setAutoReconnect: (a: number, b: number) => void;
  readonly kalamclient_setReconnectDelay: (a: number, b: bigint, c: bigint) => void;
  readonly kalamclient_setMaxReconnectAttempts: (a: number, b: number) => void;
  readonly kalamclient_getReconnectAttempts: (a: number) => number;
  readonly kalamclient_isReconnecting: (a: number) => number;
  readonly kalamclient_getLastSeqId: (a: number, b: number, c: number) => [number, number];
  readonly kalamclient_connect: (a: number) => any;
  readonly kalamclient_disconnect: (a: number) => any;
  readonly kalamclient_isConnected: (a: number) => number;
  readonly kalamclient_insert: (a: number, b: number, c: number, d: number, e: number) => any;
  readonly kalamclient_delete: (a: number, b: number, c: number, d: number, e: number) => any;
  readonly kalamclient_query: (a: number, b: number, c: number) => any;
  readonly kalamclient_queryWithParams: (a: number, b: number, c: number, d: number, e: number) => any;
  readonly kalamclient_subscribe: (a: number, b: number, c: number, d: any) => any;
  readonly kalamclient_subscribeWithSql: (a: number, b: number, c: number, d: number, e: number, f: any) => any;
  readonly kalamclient_unsubscribe: (a: number, b: number, c: number) => any;
  readonly wasm_bindgen__convert__closures_____invoke__h53534a1ef46f6383: (a: number, b: number, c: any) => void;
  readonly wasm_bindgen__closure__destroy__h642479b1977f4759: (a: number, b: number) => void;
  readonly wasm_bindgen__convert__closures_____invoke__hedbd134adbce07f2: (a: number, b: number) => void;
  readonly wasm_bindgen__convert__closures_____invoke__hd3a7b72008ab54d8: (a: number, b: number, c: any) => void;
  readonly wasm_bindgen__closure__destroy__h3c65cb5d3157b83f: (a: number, b: number) => void;
  readonly wasm_bindgen__convert__closures_____invoke__hc196795d34a3c3f6: (a: number, b: number, c: any, d: any) => void;
  readonly __wbindgen_malloc: (a: number, b: number) => number;
  readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
  readonly __wbindgen_exn_store: (a: number) => void;
  readonly __externref_table_alloc: () => number;
  readonly __wbindgen_externrefs: WebAssembly.Table;
  readonly __externref_table_dealloc: (a: number) => void;
  readonly __wbindgen_free: (a: number, b: number, c: number) => void;
  readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
* Instantiates the given `module`, which can either be bytes or
* a precompiled `WebAssembly.Module`.
*
* @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
*
* @returns {InitOutput}
*/
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
* If `module_or_path` is {RequestInfo} or {URL}, makes a request and
* for everything else, calls `WebAssembly.instantiate` directly.
*
* @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
*
* @returns {Promise<InitOutput>}
*/
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
