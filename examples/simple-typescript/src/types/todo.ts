/**
 * Type definitions for KalamDB TODO application
 * Feature: 006-docker-wasm-examples
 */

/**
 * TODO item as stored in KalamDB
 */
export interface Todo {
  /** Unique identifier (auto-increment) */
  id: number;
  
  /** TODO description (1-500 characters) */
  title: string;
  
  /** Completion status */
  completed: boolean;
  
  /** Creation timestamp (ISO 8601) */
  created_at: string;
}

/**
 * Input for creating a new TODO (without auto-generated fields)
 */
export interface CreateTodoInput {
  title: string;
  completed?: boolean;
}

/**
 * WebSocket connection status
 */
export type ConnectionStatus = 'connecting' | 'connected' | 'disconnected' | 'error';

/**
 * Subscription event types from KalamDB
 */
export type SubscriptionEventType = 'insert' | 'update' | 'delete';

/**
 * Subscription event from KalamDB WebSocket
 */
export interface SubscriptionEvent<T = any> {
  type: SubscriptionEventType;
  table: string;
  id: number;
  data?: T;
}

/**
 * localStorage cache structure
 */
export interface TodoCache {
  todos: Todo[];
  lastSyncId: number;
  timestamp: number;
}
