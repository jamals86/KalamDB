/**
 * KalamDB Client Service
 * Feature: 006-docker-wasm-examples
 * 
 * This module provides a wrapper around the KalamDB WASM SDK (@kalamdb/client)
 * with helper methods specific to the TODO application.
 * 
 * Architecture:
 * - Imports the real WASM client from '@kalamdb/client' (link/sdks/typescript/)
 * - Extends SDK with application-specific helpers (insertTodo, deleteTodo)
 * - Provides type-safe interface for React components
 */

import init, { KalamClient } from '@kalamdb/client';
import type { Todo, CreateTodoInput, SubscriptionEvent } from '../types/todo';

/**
 * Configuration for KalamDB client
 */
export interface KalamClientConfig {
  url: string;
  apiKey: string;
}

/**
 * Subscription callback function
 */
export type SubscriptionCallback = (event: SubscriptionEvent<Todo>) => void;

/**
 * Enhanced KalamDB client with TODO-specific helpers
 * 
 * This wraps the WASM SDK client and adds convenience methods
 * for working with TODOs.
 */
export class KalamDBClient {
  private client: KalamClient;
  private subscriptionId: string | null = null;

  constructor(client: KalamClient) {
    this.client = client;
  }

  /**
   * Connect to KalamDB server
   */
  async connect(): Promise<void> {
    await this.client.connect();
  }

  /**
   * Disconnect from server
   */
  async disconnect(): Promise<void> {
    if (this.subscriptionId) {
      await this.client.unsubscribe(this.subscriptionId);
      this.subscriptionId = null;
    }
    await this.client.disconnect();
  }

  /**
   * Check if connected
   */
  isConnected(): boolean {
    return this.client.isConnected();
  }

  /**
   * Subscribe to table changes
   * @param table - Table name to subscribe to
   * @param fromId - Start from this ID (for catching up on missed events) - TODO: Pass to SDK when supported
   * @param callback - Function to call when changes occur
   */
  async subscribe(table: string, fromId: number = 0, callback?: SubscriptionCallback): Promise<void> {
    if (!callback) {
      return;
    }

    // TODO: When SDK supports fromId parameter, pass it here
    // For now, we subscribe from the current point
    console.log(`Subscribing to ${table} from ID ${fromId} (fromId not yet supported by SDK)`);

    // Wrap callback to parse JSON and convert to SubscriptionEvent
    const wrappedCallback = (eventJson: string) => {
      try {
        const event = JSON.parse(eventJson) as SubscriptionEvent<Todo>;
        callback(event);
      } catch (error) {
        console.error('Failed to parse subscription event:', error);
      }
    };

    this.subscriptionId = await this.client.subscribe(table, wrappedCallback);
  }

  /**
   * Execute SQL query
   * @param sql - SQL query string
   * @returns Parsed JSON results
   */
  async query<T = any>(sql: string): Promise<T[]> {
    const resultJson = await this.client.query(sql);
    const result = JSON.parse(resultJson);
    
    // Handle different result formats
    if (Array.isArray(result)) {
      return result;
    }
    if (result.rows && Array.isArray(result.rows)) {
      return result.rows;
    }
    if (result.data && Array.isArray(result.data)) {
      return result.data;
    }
    
    return [];
  }

  /**
   * Insert a TODO
   * @param todo - TODO data to insert
   * @returns The inserted TODO (with generated ID)
   */
  async insertTodo(todo: CreateTodoInput): Promise<Todo> {
    const dataJson = JSON.stringify({
      title: todo.title,
      completed: todo.completed ?? false
    });

    const resultJson = await this.client.insert('todos', dataJson);
    const result = JSON.parse(resultJson);
    
    // Return the inserted TODO
    // The result might be the full row or just confirmation
    if (result.id) {
      return result as Todo;
    }
    
    // If insert doesn't return the row, query for it
    const rows = await this.query<Todo>('SELECT * FROM todos ORDER BY id DESC LIMIT 1');
    if (rows.length === 0) {
      throw new Error('Failed to retrieve inserted TODO');
    }
    
    return rows[0];
  }

  /**
   * Delete a TODO by ID
   * @param id - TODO ID to delete
   */
  async deleteTodo(id: number): Promise<void> {
    await this.client.delete('todos', String(id));
  }
}

/**
 * Initialize and create KalamDB client
 * @param config - Client configuration
 * @returns Initialized client instance
 */
export async function createKalamClient(config: KalamClientConfig): Promise<KalamDBClient> {
  // Validate configuration
  if (!config.url) {
    throw new Error('KalamDB URL is required');
  }
  if (!config.apiKey) {
    throw new Error('API key is required');
  }

  // Initialize WASM module
  await init();

  // Create WASM client
  const wasmClient = new KalamClient(config.url, config.apiKey);
  
  // Connect to server
  await wasmClient.connect();

  // Wrap in enhanced client
  return new KalamDBClient(wasmClient);
}

/**
 * Get KalamDB configuration from environment variables
 * @returns Client configuration from .env
 */
export function getKalamConfig(): KalamClientConfig {
  const url = import.meta.env.VITE_KALAMDB_URL;
  const apiKey = import.meta.env.VITE_KALAMDB_API_KEY;

  if (!url) {
    throw new Error('VITE_KALAMDB_URL environment variable is required');
  }
  if (!apiKey || apiKey === 'your-api-key-here') {
    throw new Error('VITE_KALAMDB_API_KEY environment variable is required. Run: kalam user create --name "demo-user" --role "user"');
  }

  return { url, apiKey };
}
