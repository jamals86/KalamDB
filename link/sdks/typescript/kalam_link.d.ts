/* tslint:disable */
/* eslint-disable */
/**
 * WASM-compatible KalamDB client
 *
 * # Example (JavaScript)
 * ```js
 * import init, { KalamClient } from './pkg/kalam_link.js';
 *
 * await init();
 * const client = new KalamClient(
 *   "http://localhost:8080",
 *   "username",
 *   "password"
 * );
 * ```
 */
export class KalamClient {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Create a new KalamDB client (T042, T043, T044)
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
   * * `options` - Optional JSON string with subscription options (e.g., `{"batch_size": 100}`)
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
   *   JSON.stringify({ batch_size: 50 }),
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
  readonly kalamclient_connect: (a: number) => any;
  readonly kalamclient_disconnect: (a: number) => any;
  readonly kalamclient_isConnected: (a: number) => number;
  readonly kalamclient_insert: (a: number, b: number, c: number, d: number, e: number) => any;
  readonly kalamclient_delete: (a: number, b: number, c: number, d: number, e: number) => any;
  readonly kalamclient_query: (a: number, b: number, c: number) => any;
  readonly kalamclient_subscribe: (a: number, b: number, c: number, d: any) => any;
  readonly kalamclient_subscribeWithSql: (a: number, b: number, c: number, d: number, e: number, f: any) => any;
  readonly kalamclient_unsubscribe: (a: number, b: number, c: number) => any;
  readonly wasm_bindgen__convert__closures_____invoke__hbe312fe94906cca5: (a: number, b: number, c: any) => void;
  readonly wasm_bindgen__closure__destroy__h07a7edbb3f89726e: (a: number, b: number) => void;
  readonly wasm_bindgen__convert__closures_____invoke__h762ee00b4618786e: (a: number, b: number) => void;
  readonly wasm_bindgen__convert__closures_____invoke__h58212949185b0233: (a: number, b: number, c: any) => void;
  readonly wasm_bindgen__closure__destroy__h882e84b55e202da7: (a: number, b: number) => void;
  readonly wasm_bindgen__convert__closures_____invoke__h631465b85669de06: (a: number, b: number, c: any, d: any) => void;
  readonly __wbindgen_malloc: (a: number, b: number) => number;
  readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
  readonly __wbindgen_exn_store: (a: number) => void;
  readonly __externref_table_alloc: () => number;
  readonly __wbindgen_externrefs: WebAssembly.Table;
  readonly __externref_table_dealloc: (a: number) => void;
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
