/**
 * kalam-link - Official TypeScript/JavaScript client for KalamDB
 * 
 * This package provides a type-safe wrapper around the KalamDB WASM bindings
 * for use in Node.js and browser environments.
 * 
 * Features:
 * - SQL query execution via HTTP
 * - Real-time subscriptions via WebSocket (single connection, multiple subscriptions)
 * - Subscription management with modern patterns (unsubscribe functions)
 * - Cross-platform support (Node.js & Browser)
 * - Type-safe authentication with multiple providers (Basic Auth, JWT, Anonymous)
 * 
 * @example
 * ```typescript
 * import { createClient, Auth } from 'kalam-link';
 * 
 * // Basic Auth (username/password)
 * const client = createClient({
 *   url: 'http://localhost:8080',
 *   auth: Auth.basic('admin', 'admin')
 * });
 * 
 * // JWT Token Auth
 * const jwtClient = createClient({
 *   url: 'http://localhost:8080',
 *   auth: Auth.jwt('eyJhbGciOiJIUzI1NiIs...')
 * });
 * 
 * // Anonymous (localhost bypass)
 * const anonClient = createClient({
 *   url: 'http://localhost:8080',
 *   auth: Auth.none()
 * });
 * 
 * await client.connect();
 * 
 * // Subscribe to changes (returns unsubscribe function - Firebase/Supabase style)
 * const unsubscribe = await client.subscribe('messages', (event) => {
 *   console.log('Change:', event);
 * });
 * 
 * // Check subscription count
 * console.log(`Active subscriptions: ${client.getSubscriptionCount()}`);
 * 
 * // Later: unsubscribe when done
 * await unsubscribe();
 * ```
 */

import init, { KalamClient as WasmClient } from './wasm/kalam_link.js';

// Re-export authentication types
export { 
  Auth,
  AuthCredentials,
  BasicAuthCredentials,
  JwtAuthCredentials,
  NoAuthCredentials,
  buildAuthHeader,
  encodeBasicAuth,
  isAuthenticated,
  isBasicAuth,
  isJwtAuth,
  isNoAuth
} from './auth.js';

import type { AuthCredentials } from './auth.js';

// Re-export types from WASM bindings
export type { KalamClient as WasmKalamClient } from './wasm/kalam_link.js';

/**
 * Schema field describing a column in the result set
 */
export interface SchemaField {
  /** Column name */
  name: string;
  /** Data type (e.g., 'BigInt', 'Text', 'Timestamp') */
  data_type: string;
  /** Column index in the row array */
  index: number;
}

/**
 * Query result structure matching KalamDB server response
 */
export interface QueryResult {
  /** Schema describing the columns in the result set */
  schema: SchemaField[];
  /** Result rows as arrays of values (ordered by schema index) */
  rows?: unknown[][];
  /** Number of rows affected or returned */
  row_count: number;
  /** Optional message for non-query statements */
  message?: string;
}

/**
 * Full query response from the server
 */
export interface QueryResponse {
  /** Query execution status */
  status: 'success' | 'error';
  /** Array of result sets, one per executed statement */
  results: QueryResult[];
  /** Query execution time in milliseconds */
  took?: number;
  /** Error details if status is "error" */
  error?: ErrorDetail;
}

/**
 * Error detail structure
 */
export interface ErrorDetail {
  /** Error code */
  code: string;
  /** Error message */
  message: string;
  /** Optional error details */
  details?: any;
}

/**
 * Message type enum for WebSocket subscription events
 */
export enum MessageType {
  SubscriptionAck = 'subscription_ack',
  InitialDataBatch = 'initial_data_batch',
  Change = 'change',
  Error = 'error',
}

/**
 * Change type enum for live subscription change events
 */
export enum ChangeType {
  Insert = 'insert',
  Update = 'update',
  Delete = 'delete',
}

/**
 * Batch loading status enum
 */
export enum BatchStatus {
  Loading = 'loading',
  LoadingBatch = 'loading_batch',
  Ready = 'ready',
}

/**
 * Server message types for WebSocket subscriptions
 */
export type ServerMessage =
  | { type: MessageType.SubscriptionAck | 'subscription_ack'; subscription_id: string; total_rows: number; batch_control: BatchControl; schema: SchemaField[] }
  | { type: MessageType.InitialDataBatch | 'initial_data_batch'; subscription_id: string; rows: Record<string, any>[]; batch_control: BatchControl }
  | { type: MessageType.Change | 'change'; subscription_id: string; change_type: ChangeType | 'insert' | 'update' | 'delete'; rows?: Record<string, any>[]; old_values?: Record<string, any>[] }
  | { type: MessageType.Error | 'error'; subscription_id: string; code: string; message: string };

/**
 * Batch control metadata for paginated data loading
 *
 * Note: We don't include total_batches because we can't know it upfront
 * without counting all rows first (expensive). The `has_more` field is
 * sufficient for clients to know whether to request more batches.
 */
export interface BatchControl {
  /** Current batch number (0-indexed) */
  batch_num: number;
  /** Whether more batches are available */
  has_more: boolean;
  /** Loading status */
  status: BatchStatus | 'loading' | 'loading_batch' | 'ready';
  /** Last sequence ID in this batch */
  last_seq_id?: string;
  /** Snapshot boundary sequence ID */
  snapshot_end_seq?: string;
}

/**
 * Subscription callback function type
 */
export type SubscriptionCallback = (event: ServerMessage) => void;

/**
 * Function to unsubscribe from a subscription (Firebase/Supabase style)
 * @returns Promise that resolves when unsubscription is complete
 */
export type Unsubscribe = () => Promise<void>;

/**
 * Information about an active subscription
 */
export interface SubscriptionInfo {
  /** Unique subscription ID */
  id: string;
  /** Table name or SQL query being subscribed to */
  tableName: string;
  /** Timestamp when subscription was created */
  createdAt: Date;
}

/**
 * Connection-level options for WebSocket connection behavior
 * 
 * These options control the overall WebSocket connection, including:
 * - Automatic reconnection on connection loss
 * - Reconnection timing and retry limits
 * 
 * Separate from SubscriptionOptions which control individual subscriptions.
 * 
 * @example
 * ```typescript
 * const client = new KalamDBClient('http://localhost:8080', 'user', 'pass');
 * 
 * // Configure connection options before connecting
 * client.setAutoReconnect(true);
 * client.setReconnectDelay(2000, 60000);
 * client.setMaxReconnectAttempts(10);
 * 
 * await client.connect();
 * ```
 */
export interface ConnectionOptions {
  /**
   * Enable automatic reconnection on connection loss
   * Default: true - automatically attempts to reconnect
   */
  auto_reconnect?: boolean;

  /**
   * Initial delay in milliseconds between reconnection attempts
   * Default: 1000ms (1 second)
   * Uses exponential backoff up to max_reconnect_delay_ms
   */
  reconnect_delay_ms?: number;

  /**
   * Maximum delay between reconnection attempts (for exponential backoff)
   * Default: 30000ms (30 seconds)
   */
  max_reconnect_delay_ms?: number;

  /**
   * Maximum number of reconnection attempts before giving up
   * Default: undefined (infinite retries)
   * Set to 0 to disable reconnection entirely
   */
  max_reconnect_attempts?: number;
}

/**
 * Subscription options for controlling individual subscription behavior
 * 
 * These options control subscription behavior including:
 * - Initial data loading (batch_size, last_rows)
 * - Data resumption after reconnection (from_seq_id)
 * 
 * Aligned with backend's SubscriptionOptions.
 * 
 * @example
 * ```typescript
 * // Fetch last 100 rows with batch size of 50
 * const options: SubscriptionOptions = {
 *   batch_size: 50,
 *   last_rows: 100
 * };
 * 
 * const unsubscribe = await client.subscribe('messages', callback, options);
 * ```
 */
export interface SubscriptionOptions {
  /** 
   * Hint for server-side batch sizing during initial data load
   * Default: server-configured (typically 1000 rows per batch)
   */
  batch_size?: number;

  /**
   * Number of last (newest) rows to fetch for initial data
   * Default: undefined (fetch all matching rows)
   */
  last_rows?: number;

  /**
   * Resume subscription from a specific sequence ID
   * When set, the server will only send changes after this seq_id
   * Typically set automatically during reconnection to resume from last received event
   */
  from_seq_id?: string;
}

/**
 * @deprecated Use SubscriptionOptions instead. This type alias is for backwards compatibility.
 */
export type SubscribeOptions = SubscriptionOptions;

/**
 * Configuration options for KalamDB client (new type-safe API)
 * 
 * Uses discriminated unions for type-safe authentication.
 * 
 * @example
 * ```typescript
 * // Type-safe auth options
 * const client = createClient({
 *   url: 'http://localhost:8080',
 *   auth: Auth.basic('admin', 'admin')
 * });
 * 
 * // JWT authentication
 * const jwtClient = createClient({
 *   url: 'http://localhost:8080',
 *   auth: Auth.jwt('eyJhbGciOiJIUzI1NiIs...')
 * });
 * ```
 */
export interface ClientOptionsWithAuth {
  /** Server URL (e.g., 'http://localhost:8080') */
  url: string;
  /** Authentication credentials (type-safe) */
  auth: AuthCredentials;
}

/**
 * Configuration options for KalamDB client (legacy API)
 * 
 * @deprecated Use ClientOptionsWithAuth with Auth.basic() instead
 */
export interface ClientOptionsLegacy {
  /** Server URL (e.g., 'http://localhost:8080') */
  url: string;
  /** Username for authentication */
  username: string;
  /** Password for authentication */
  password: string;
}

/**
 * Configuration options for KalamDB client
 * 
 * Supports both new type-safe auth API and legacy username/password API.
 */
export type ClientOptions = ClientOptionsWithAuth | ClientOptionsLegacy;

/**
 * Type guard to check if options use the new auth API
 */
function isAuthOptions(options: ClientOptions): options is ClientOptionsWithAuth {
  return 'auth' in options;
}

/**
 * KalamDB Client - TypeScript wrapper around WASM bindings
 * 
 * Provides a type-safe interface to KalamDB with support for:
 * - SQL query execution
 * - Real-time WebSocket subscriptions
 * - Multiple authentication methods (Basic Auth, JWT, Anonymous)
 * - Cross-platform (Node.js & Browser)
 * - Subscription tracking and management
 * 
 * @example
 * ```typescript
 * // New API with type-safe auth (recommended)
 * import { createClient, Auth } from 'kalam-link';
 * 
 * const client = createClient({
 *   url: 'http://localhost:8080',
 *   auth: Auth.basic('alice', 'password123')
 * });
 * 
 * // JWT authentication
 * const jwtClient = createClient({
 *   url: 'http://localhost:8080',
 *   auth: Auth.jwt('eyJhbGciOiJIUzI1NiIs...')
 * });
 * 
 * // Anonymous (no auth - localhost bypass)
 * const anonClient = createClient({
 *   url: 'http://localhost:8080',
 *   auth: Auth.none()
 * });
 * 
 * await client.connect();
 * 
 * // Execute queries
 * const users = await client.query('SELECT * FROM users WHERE active = true');
 * console.log(users.results[0].rows);
 * 
 * // Subscribe to changes (returns unsubscribe function)
 * const unsubscribe = await client.subscribe('messages', (event) => {
 *   if (event.type === 'change') {
 *     console.log('New message:', event.rows);
 *   }
 * });
 * 
 * // Check subscription count
 * console.log(`Active: ${client.getSubscriptionCount()}`);
 * 
 * // Cleanup
 * await unsubscribe();
 * await client.disconnect();
 * ```
 */
export class KalamDBClient {
  private wasmClient: WasmClient | null = null;
  private initialized = false;
  private url: string;
  private auth: AuthCredentials;
  
  /** Track active subscriptions for management */
  private subscriptions: Map<string, SubscriptionInfo> = new Map();

  /**
   * Create a new KalamDB client with type-safe auth options
   * 
   * @param options - Client options with URL and auth credentials
   * 
   * @throws Error if url is empty
   * 
   * @example
   * ```typescript
  * import { KalamDBClient, Auth } from 'kalam-link';
   * 
   * const client = new KalamDBClient({
   *   url: 'http://localhost:8080',
   *   auth: Auth.basic('admin', 'secret')
   * });
   * ```
   */
  constructor(options: ClientOptionsWithAuth);
  
  /**
   * Create a new KalamDB client with username/password (legacy API)
   * 
   * @param url - Server URL (e.g., 'http://localhost:8080')
   * @param username - Username for authentication
   * @param password - Password for authentication
   * 
   * @throws Error if url, username, or password is empty
   * 
   * @deprecated Use constructor with ClientOptionsWithAuth and Auth.basic() instead
   */
  constructor(url: string, username: string, password: string);
  
  constructor(
    urlOrOptions: string | ClientOptionsWithAuth, 
    username?: string, 
    password?: string
  ) {
    // Handle new options-based API
    if (typeof urlOrOptions === 'object') {
      if (!urlOrOptions.url) throw new Error('KalamDBClient: url is required');
      if (!urlOrOptions.auth) throw new Error('KalamDBClient: auth is required');
      
      this.url = urlOrOptions.url;
      this.auth = urlOrOptions.auth;
    } 
    // Handle legacy API (string url, username, password)
    else {
      if (!urlOrOptions) throw new Error('KalamDBClient: url parameter is required');
      if (!username) throw new Error('KalamDBClient: username parameter is required');
      if (!password) throw new Error('KalamDBClient: password parameter is required');
      
      this.url = urlOrOptions;
      // Convert legacy API to new auth format
      this.auth = { type: 'basic', username, password };
    }
  }

  /**
   * Get the current authentication type
   * 
   * @returns 'basic', 'jwt', or 'none'
   */
  getAuthType(): 'basic' | 'jwt' | 'none' {
    return this.auth.type;
  }

  /**
   * Initialize WASM module and create client instance
   * 
   * Must be called before any other operations. Automatically called by connect()
   * if not already initialized.
   * 
   * @throws Error if WASM initialization fails
   */
  async initialize(): Promise<void> {
    if (this.initialized) return;

    try {
      // Browser environment - WASM will be fetched automatically
      await init();
      
      // Create WASM client based on auth type
      switch (this.auth.type) {
        case 'basic':
          this.wasmClient = new WasmClient(this.url, this.auth.username, this.auth.password);
          break;
        case 'jwt':
          this.wasmClient = WasmClient.withJwt(this.url, this.auth.token);
          break;
        case 'none':
          this.wasmClient = WasmClient.anonymous(this.url);
          break;
      }
      
      this.initialized = true;
    } catch (error) {
      throw new Error(`Failed to initialize WASM client: ${error}`);
    }
  }

  /**
   * Connect to KalamDB server via WebSocket
   * 
   * Establishes a persistent WebSocket connection for real-time subscriptions.
   * Also initializes the WASM module if not already done.
   * 
   * @throws Error if connection fails
   */
  async connect(): Promise<void> {
    await this.initialize();
    if (!this.wasmClient) {
      throw new Error('WASM client not initialized');
    }
    await this.wasmClient.connect();
  }

  /**
   * Enable or disable automatic reconnection
   * 
   * When enabled, the client will automatically attempt to reconnect
   * if the WebSocket connection is lost, and will re-subscribe to all
   * active subscriptions with resume_from_seq_id to catch up on missed events.
   * 
   * @param enabled - Whether to automatically reconnect on connection loss
   * 
   * @example
   * ```typescript
   * client.setAutoReconnect(true);  // Enable (default)
   * client.setAutoReconnect(false); // Disable for manual control
   * ```
   */
  setAutoReconnect(enabled: boolean): void {
    this.ensureInitialized();
    this.wasmClient!.setAutoReconnect(enabled);
  }

  /**
   * Configure reconnection delay parameters
   * 
   * The client uses exponential backoff starting from initialDelayMs,
   * doubling each attempt up to maxDelayMs.
   * 
   * @param initialDelayMs - Initial delay between reconnection attempts (default: 1000ms)
   * @param maxDelayMs - Maximum delay for exponential backoff (default: 30000ms)
   * 
   * @example
   * ```typescript
   * // Start with 500ms delay, max out at 10 seconds
   * client.setReconnectDelay(500, 10000);
   * ```
   */
  setReconnectDelay(initialDelayMs: number, maxDelayMs: number): void {
    this.ensureInitialized();
    this.wasmClient!.setReconnectDelay(BigInt(initialDelayMs), BigInt(maxDelayMs));
  }

  /**
   * Set maximum number of reconnection attempts
   * 
   * @param maxAttempts - Maximum attempts before giving up (0 = infinite)
   * 
   * @example
   * ```typescript
   * client.setMaxReconnectAttempts(5);  // Give up after 5 attempts
   * client.setMaxReconnectAttempts(0);  // Never give up (default)
   * ```
   */
  setMaxReconnectAttempts(maxAttempts: number): void {
    this.ensureInitialized();
    this.wasmClient!.setMaxReconnectAttempts(maxAttempts);
  }

  /**
   * Get the current number of reconnection attempts
   * 
   * Resets to 0 after a successful reconnection.
   * 
   * @returns Current reconnection attempt count
   */
  getReconnectAttempts(): number {
    this.ensureInitialized();
    return this.wasmClient!.getReconnectAttempts();
  }

  /**
   * Check if the client is currently attempting to reconnect
   * 
   * @returns true if a reconnection is in progress
   */
  isReconnecting(): boolean {
    this.ensureInitialized();
    return this.wasmClient!.isReconnecting();
  }

  /**
   * Get the last received sequence ID for a subscription
   * 
   * Useful for debugging or manual tracking of subscription progress.
   * This seq_id is automatically used during reconnection to resume
   * from where the subscription left off.
   * 
   * @param subscriptionId - The subscription ID to query
   * @returns The last seq_id as a string, or undefined if not set
   * 
   * @example
   * ```typescript
   * const lastSeq = client.getLastSeqId(subscriptionId);
   * console.log(`Last received seq: ${lastSeq}`);
   * ```
   */
  getLastSeqId(subscriptionId: string): string | undefined {
    this.ensureInitialized();
    return this.wasmClient!.getLastSeqId(subscriptionId) ?? undefined;
  }

  /**
   * Helper to ensure WASM client is initialized
   * @private
   */
  private ensureInitialized(): void {
    if (!this.wasmClient) {
      throw new Error('WASM client not initialized. Call connect() first.');
    }
  }

  /**
   * Disconnect from KalamDB server
   * 
   * Closes the WebSocket connection and cleans up all active subscriptions.
   */
  async disconnect(): Promise<void> {
    if (this.wasmClient) {
      await this.wasmClient.disconnect();
    }
    // Clear subscription tracking
    this.subscriptions.clear();
  }

  /**
   * Check if client is currently connected
   * 
   * @returns true if WebSocket connection is active, false otherwise
   */
  isConnected(): boolean {
    return this.wasmClient?.isConnected() ?? false;
  }

  /**
   * Execute a SQL query with optional parameters
   * 
   * Supports all SQL statements: SELECT, INSERT, UPDATE, DELETE, CREATE, DROP, etc.
   * Use parameterized queries to prevent SQL injection.
   * 
   * @param sql - SQL query string (may contain $1, $2, ... placeholders)
   * @param params - Optional array of parameter values for placeholders
   * @returns Parsed query response with results
   * 
   * @throws Error if query execution fails
   * 
   * @example
   * ```typescript
   * // Simple query
   * const result = await client.query('SELECT * FROM users');
   * 
   * // Parameterized query (recommended for user input)
   * const users = await client.query(
   *   'SELECT * FROM users WHERE id = $1 AND age > $2',
   *   [42, 18]
   * );
   * console.log(users.results[0].rows);
   * 
   * // INSERT with parameters
   * await client.query(
   *   "INSERT INTO users (name, email) VALUES ($1, $2)",
   *   ['Alice', 'alice@example.com']
   * );
   * 
   * // DDL statements (no params)
   * await client.query('CREATE TABLE products (id BIGINT PRIMARY KEY, name TEXT)');
   * ```
   */
  async query(sql: string, params?: any[]): Promise<QueryResponse> {
    await this.initialize();
    if (!this.wasmClient) {
      throw new Error('WASM client not initialized');
    }

    let resultStr: string;
    if (params && params.length > 0) {
      resultStr = await this.wasmClient.queryWithParams(sql, JSON.stringify(params));
    } else {
      resultStr = await this.wasmClient.query(sql);
    }
    return JSON.parse(resultStr) as QueryResponse;
  }

  /**
   * Insert data into a table (convenience method)
   * 
   * @param tableName - Name of the table (can include namespace, e.g., 'app.users')
   * @param data - Object containing column values
   * @returns Query response
   * 
   * @throws Error if insert fails
   * 
   * @example
   * ```typescript
   * await client.insert('todos', {
   *   title: 'Buy groceries',
   *   completed: false
   * });
   * ```
   */
  async insert(tableName: string, data: Record<string, any>): Promise<QueryResponse> {
    const dataJson = JSON.stringify(data);
    const resultStr = await this.wasmClient!.insert(tableName, dataJson);
    return JSON.parse(resultStr) as QueryResponse;
  }

  /**
   * Delete a row from a table (convenience method)
   * 
   * @param tableName - Name of the table
   * @param rowId - ID of the row to delete
   * 
   * @throws Error if delete fails
   * 
   * @example
   * ```typescript
   * await client.delete('todos', '123456789');
   * ```
   */
  async delete(tableName: string, rowId: string | number): Promise<void> {
    await this.wasmClient!.delete(tableName, String(rowId));
  }

  /**
   * Subscribe to real-time changes in a table
   * 
   * The callback will be invoked for:
   * - Initial data batches (type: 'initial_data_batch')
   * - Live changes (type: 'change')
   * - Errors (type: 'error')
   * 
   * Returns an unsubscribe function (Firebase/Supabase style) for easy cleanup.
   * 
   * @param tableName - Name of the table to subscribe to
   * @param callback - Function called when changes occur
   * @param options - Optional subscription options (batch_size, last_rows, from_seq_id)
   * @returns Unsubscribe function to stop receiving updates
   * 
   * @throws Error if subscription fails or not connected
   * 
   * @example
   * ```typescript
   * // Simple subscription
   * const unsubscribe = await client.subscribe('messages', (event) => {
   *   if (event.type === 'change') {
   *     console.log('New data:', event.rows);
   *   }
   * });
   * 
   * // With options
   * const unsubscribe = await client.subscribe('messages', callback, {
   *   batch_size: 100,  // Load initial data in batches of 100
   *   last_rows: 50     // Only fetch last 50 rows initially
   * });
   * 
   * // Later: unsubscribe when done
   * await unsubscribe();
   * ```
   */
  async subscribe(
    tableName: string,
    callback: SubscriptionCallback,
    options?: SubscriptionOptions
  ): Promise<Unsubscribe> {
    // Use subscribeWithSql internally with SELECT * FROM tableName
    const sql = `SELECT * FROM ${tableName}`;
    return this.subscribeWithSql(sql, callback, options);
  }

  /**
   * Subscribe to a SQL query with real-time updates
   * 
   * More flexible than subscribe() - allows custom SQL queries with WHERE clauses,
   * JOINs, and other SQL features.
   * 
   * @param sql - SQL SELECT query to subscribe to
   * @param callback - Function called when changes occur
   * @param options - Optional subscription options (batch_size, last_rows, from_seq_id)
   * @returns Unsubscribe function to stop receiving updates
   * 
   * @throws Error if subscription fails or not connected
   * 
   * @example
   * ```typescript
   * // Subscribe to filtered query
   * const unsubscribe = await client.subscribeWithSql(
   *   'SELECT * FROM chat.messages WHERE conversation_id = 1',
   *   (event) => {
   *     if (event.type === 'change') {
   *       console.log('New message:', event.rows);
   *     }
   *   },
   *   { batch_size: 50, last_rows: 100 }
   * );
   * 
   * // Later: unsubscribe when done
   * await unsubscribe();
   * ```
   */
  async subscribeWithSql(
    sql: string,
    callback: SubscriptionCallback,
    options?: SubscriptionOptions
  ): Promise<Unsubscribe> {
    await this.initialize();
    if (!this.wasmClient) {
      throw new Error('WASM client not initialized');
    }

    // Wrap callback to parse JSON and provide typed event
    const wrappedCallback = (eventJson: string) => {
      try {
        console.log('[KalamClient SDK] Received event JSON:', eventJson.substring(0, 200));
        const event = JSON.parse(eventJson) as ServerMessage;
        console.log('[KalamClient SDK] Parsed event type:', (event as any).type);
        callback(event);
      } catch (error) {
        console.error('Failed to parse subscription event:', error);
      }
    };

    // Convert options to JSON string if provided
    const optionsJson = options ? JSON.stringify(options) : undefined;
    
    const subscriptionId = await this.wasmClient.subscribeWithSql(
      sql,
      optionsJson,
      wrappedCallback as any
    );
    
    // Track the subscription (use SQL as tableName for tracking)
    this.subscriptions.set(subscriptionId, {
      id: subscriptionId,
      tableName: sql,
      createdAt: new Date()
    });

    // Return unsubscribe function (Firebase/Supabase style)
    return async () => {
      await this.unsubscribe(subscriptionId);
    };
  }

  /**
   * Unsubscribe from table changes
   * 
   * @param subscriptionId - ID returned from subscribe()
   * 
   * @throws Error if unsubscribe fails or not connected
   * 
   * @example
   * ```typescript
   * // Using the returned unsubscribe function (preferred)
   * const unsubscribe = await client.subscribe('messages', handleChange);
   * await unsubscribe();
   * 
   * // Or manually with subscription ID
   * const unsubscribe = await client.subscribe('messages', handleChange);
   * const subs = client.getSubscriptions();
   * await client.unsubscribe(subs[0].id);
   * ```
   */
  async unsubscribe(subscriptionId: string): Promise<void> {
    if (!this.wasmClient) {
      throw new Error('WASM client not initialized');
    }
    await this.wasmClient.unsubscribe(subscriptionId);
    
    // Remove from tracking
    this.subscriptions.delete(subscriptionId);
  }

  /**
   * Get the number of active subscriptions
   * 
   * @returns Number of active subscriptions
   * 
   * @example
   * ```typescript
   * console.log(`Active subscriptions: ${client.getSubscriptionCount()}`);
   * 
   * // Prevent too many subscriptions
   * if (client.getSubscriptionCount() >= 10) {
   *   console.warn('Too many subscriptions!');
   * }
   * ```
   */
  getSubscriptionCount(): number {
    return this.subscriptions.size;
  }

  /**
   * Get information about all active subscriptions
   * 
   * @returns Array of subscription info objects
   * 
   * @example
   * ```typescript
   * const subs = client.getSubscriptions();
   * for (const sub of subs) {
   *   console.log(`Subscribed to ${sub.tableName} since ${sub.createdAt}`);
   * }
   * ```
   */
  getSubscriptions(): SubscriptionInfo[] {
    return Array.from(this.subscriptions.values());
  }

  /**
   * Check if subscribed to a specific table
   * 
   * @param tableName - Name of the table to check
   * @returns true if there's an active subscription to this table
   * 
   * @example
   * ```typescript
   * if (!client.isSubscribedTo('messages')) {
   *   await client.subscribe('messages', handleChange);
   * }
   * ```
   */
  isSubscribedTo(tableName: string): boolean {
    for (const sub of this.subscriptions.values()) {
      if (sub.tableName === tableName) {
        return true;
      }
    }
    return false;
  }

  /**
   * Unsubscribe from all active subscriptions
   * 
   * Useful for cleanup before disconnecting or switching contexts.
   * 
   * @example
   * ```typescript
   * // Cleanup all subscriptions
   * await client.unsubscribeAll();
   * console.log(`Subscriptions remaining: ${client.getSubscriptionCount()}`); // 0
   * ```
   */
  async unsubscribeAll(): Promise<void> {
    const subscriptionIds = Array.from(this.subscriptions.keys());
    for (const id of subscriptionIds) {
      await this.unsubscribe(id);
    }
  }
}

/**
 * Create a KalamDB client with the given configuration
 * 
 * Factory function that supports both the new type-safe auth API and legacy API.
 * 
 * @param options - Client configuration with URL and authentication
 * @returns Configured KalamDB client
 * 
 * @example
 * ```typescript
 * import { createClient, Auth } from 'kalam-link';
 * 
 * // New type-safe API (recommended)
 * const client = createClient({
 *   url: 'http://localhost:8080',
 *   auth: Auth.basic('admin', 'admin')
 * });
 * 
 * // JWT authentication
 * const jwtClient = createClient({
 *   url: 'http://localhost:8080',
 *   auth: Auth.jwt('eyJhbGciOiJIUzI1NiIs...')
 * });
 * 
 * // Anonymous (no authentication)
 * const anonClient = createClient({
 *   url: 'http://localhost:8080',
 *   auth: Auth.none()
 * });
 * 
 * // Legacy API (deprecated but still works)
 * const legacyClient = createClient({
 *   url: 'http://localhost:8080',
 *   username: 'admin',
 *   password: 'admin'
 * });
 * ```
 */
export function createClient(options: ClientOptions): KalamDBClient {
  // Handle both new and legacy API
  if (isAuthOptions(options)) {
    return new KalamDBClient(options);
  } else {
    // Legacy API: convert to new format
    return new KalamDBClient(options.url, options.username, options.password);
  }
}

// Default export
export default KalamDBClient;
