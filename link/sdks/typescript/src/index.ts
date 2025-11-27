/**
 * @kalamdb/client - Official TypeScript/JavaScript client for KalamDB
 * 
 * This package provides a type-safe wrapper around the KalamDB WASM bindings
 * for use in Node.js and browser environments.
 */

import init, { KalamClient as WasmClient } from '../kalam_link.js';

// Re-export types from WASM bindings
export type { KalamClient as WasmKalamClient } from '../kalam_link.js';

/**
 * Query result structure matching KalamDB server response
 */
export interface QueryResult {
  /** Result rows as JSON objects */
  rows?: Record<string, any>[];
  /** Number of rows affected or returned */
  row_count: number;
  /** Column names in the result set */
  columns: string[];
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
 * Server message types for WebSocket subscriptions
 */
export type ServerMessage =
  | { type: 'subscription_ack'; subscription_id: string; total_rows: number; batch_control: BatchControl }
  | { type: 'initial_data_batch'; subscription_id: string; rows: Record<string, any>[]; batch_control: BatchControl }
  | { type: 'change'; subscription_id: string; change_type: 'insert' | 'update' | 'delete'; rows?: Record<string, any>[]; old_values?: Record<string, any>[] }
  | { type: 'error'; subscription_id: string; code: string; message: string };

/**
 * Batch control metadata for paginated data loading
 */
export interface BatchControl {
  /** Current batch number (0-indexed) */
  batch_num: number;
  /** Total number of batches (optional/estimated) */
  total_batches?: number;
  /** Whether more batches are available */
  has_more: boolean;
  /** Loading status */
  status: 'loading' | 'loading_batch' | 'ready';
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
 * Configuration options for KalamDB client
 */
export interface ClientOptions {
  /** Server URL (e.g., 'http://localhost:8080') */
  url: string;
  /** Username for authentication */
  username: string;
  /** Password for authentication */
  password: string;
}

/**
 * KalamDB Client - TypeScript wrapper around WASM bindings
 * 
 * Provides a type-safe interface to KalamDB with support for:
 * - SQL query execution
 * - Real-time WebSocket subscriptions
 * - HTTP Basic authentication
 * - Cross-platform (Node.js & Browser)
 * 
 * @example
 * ```typescript
 * // Create and connect
 * const client = new KalamDBClient('http://localhost:8080', 'alice', 'password123');
 * await client.connect();
 * 
 * // Execute queries
 * const users = await client.query('SELECT * FROM users WHERE active = true');
 * console.log(users.results[0].rows);
 * 
 * // Subscribe to changes
 * const subId = await client.subscribe('messages', (event) => {
 *   if (event.type === 'change') {
 *     console.log('New message:', event.rows);
 *   }
 * });
 * 
 * // Cleanup
 * await client.unsubscribe(subId);
 * await client.disconnect();
 * ```
 */
export class KalamDBClient {
  private wasmClient: WasmClient | null = null;
  private initialized = false;
  private url: string;
  private username: string;
  private password: string;

  /**
   * Create a new KalamDB client
   * 
   * @param url - Server URL (e.g., 'http://localhost:8080')
   * @param username - Username for authentication
   * @param password - Password for authentication
   * 
   * @throws Error if url, username, or password is empty
   */
  constructor(url: string, username: string, password: string) {
    if (!url) throw new Error('KalamDBClient: url parameter is required');
    if (!username) throw new Error('KalamDBClient: username parameter is required');
    if (!password) throw new Error('KalamDBClient: password parameter is required');

    this.url = url;
    this.username = username;
    this.password = password;
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
      
      this.wasmClient = new WasmClient(this.url, this.username, this.password);
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
   * Disconnect from KalamDB server
   * 
   * Closes the WebSocket connection and cleans up all active subscriptions.
   */
  async disconnect(): Promise<void> {
    if (this.wasmClient) {
      await this.wasmClient.disconnect();
    }
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
   * Execute a SQL query
   * 
   * Supports all SQL statements: SELECT, INSERT, UPDATE, DELETE, CREATE, DROP, etc.
   * 
   * @param sql - SQL query string
   * @returns Parsed query response with results
   * 
   * @throws Error if query execution fails
   * 
   * @example
   * ```typescript
   * // SELECT query
   * const result = await client.query('SELECT * FROM users WHERE id = 1');
   * console.log(result.results[0].rows);
   * 
   * // INSERT query
   * await client.query("INSERT INTO users (name, email) VALUES ('Alice', 'alice@example.com')");
   * 
   * // DDL statements
   * await client.query('CREATE TABLE products (id BIGINT PRIMARY KEY, name TEXT)');
   * ```
   */
  async query(sql: string): Promise<QueryResponse> {
    await this.initialize();
    if (!this.wasmClient) {
      throw new Error('WASM client not initialized');
    }

    const resultStr = await this.wasmClient.query(sql);
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
   * @param tableName - Name of the table to subscribe to
   * @param callback - Function called when changes occur
   * @returns Subscription ID for later unsubscribe
   * 
   * @throws Error if subscription fails or not connected
   * 
   * @example
   * ```typescript
   * const subId = await client.subscribe('messages', (event) => {
   *   switch (event.type) {
   *     case 'initial_data_batch':
   *       console.log('Initial data:', event.rows);
   *       break;
   *     case 'change':
   *       console.log(`${event.change_type}:`, event.rows);
   *       break;
   *     case 'error':
   *       console.error('Subscription error:', event.message);
   *       break;
   *   }
   * });
   * ```
   */
  async subscribe(tableName: string, callback: SubscriptionCallback): Promise<string> {
    await this.initialize();
    if (!this.wasmClient) {
      throw new Error('WASM client not initialized');
    }

    // Wrap callback to parse JSON and provide typed event
    const wrappedCallback = (eventJson: string) => {
      try {
        const event = JSON.parse(eventJson) as ServerMessage;
        callback(event);
      } catch (error) {
        console.error('Failed to parse subscription event:', error);
      }
    };

    return await this.wasmClient.subscribe(tableName, wrappedCallback as any);
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
   * const subId = await client.subscribe('messages', handleChange);
   * // Later...
   * await client.unsubscribe(subId);
   * ```
   */
  async unsubscribe(subscriptionId: string): Promise<void> {
    if (!this.wasmClient) {
      throw new Error('WASM client not initialized');
    }
    await this.wasmClient.unsubscribe(subscriptionId);
  }
}

/**
 * Create a KalamDB client with the given configuration
 * 
 * Factory function alternative to constructor
 * 
 * @param options - Client configuration
 * @returns Configured KalamDB client
 */
export function createClient(options: ClientOptions): KalamDBClient {
  return new KalamDBClient(options.url, options.username, options.password);
}

// Default export
export default KalamDBClient;
