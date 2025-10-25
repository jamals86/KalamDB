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
 *   "your-api-key-here"
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
   * * `api_key` - API key for authentication (required, generated via create-user command)
   * 
   * # Errors
   * Returns JsValue error if url or api_key is empty
   */
  constructor(url: string, api_key: string);
  /**
   * Connect to KalamDB server via WebSocket (T045)
   * 
   * # Returns
   * Promise that resolves when connection is established
   */
  connect(): Promise<void>;
  /**
   * Disconnect from KalamDB server (T046)
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
   * Insert data into a table (T048)
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
   * Delete a row from a table (T049)
   * 
   * # Arguments
   * * `table_name` - Name of the table
   * * `row_id` - ID of the row to delete
   */
  delete(table_name: string, row_id: string): Promise<void>;
  /**
   * Execute a SQL query (T050)
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
   * Subscribe to table changes (T051)
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
   * Unsubscribe from table changes (T052)
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
  readonly kalamclient_new: (a: number, b: number, c: number, d: number) => [number, number, number];
  readonly kalamclient_connect: (a: number) => any;
  readonly kalamclient_disconnect: (a: number) => any;
  readonly kalamclient_isConnected: (a: number) => number;
  readonly kalamclient_insert: (a: number, b: number, c: number, d: number, e: number) => any;
  readonly kalamclient_delete: (a: number, b: number, c: number, d: number, e: number) => any;
  readonly kalamclient_query: (a: number, b: number, c: number) => any;
  readonly kalamclient_subscribe: (a: number, b: number, c: number, d: any) => any;
  readonly kalamclient_unsubscribe: (a: number, b: number, c: number) => any;
  readonly __wbindgen_exn_store: (a: number) => void;
  readonly __externref_table_alloc: () => number;
  readonly __wbindgen_export_2: WebAssembly.Table;
  readonly __wbindgen_export_3: WebAssembly.Table;
  readonly __wbindgen_malloc: (a: number, b: number) => number;
  readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
  readonly __externref_table_dealloc: (a: number) => void;
  readonly closure29_externref_shim: (a: number, b: number, c: any) => void;
  readonly closure44_externref_shim: (a: number, b: number, c: any, d: any) => void;
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
