/**
 * useTodos Hook
 * Feature: 006-docker-wasm-examples
 * 
 * Custom React hook for managing TODO state with KalamDB real-time sync
 * and localStorage caching for offline-first capabilities.
 * 
 * Features:
 * - Loads TODOs from localStorage cache on mount (instant render)
 * - Connects to KalamDB WebSocket for real-time updates
 * - Subscribes to TODO changes from last known ID
 * - Syncs insert/update/delete events across all tabs
 * - Persists changes to localStorage
 * - Tracks connection status
 * - Disables writes when disconnected
 */

import { useState, useEffect, useCallback, useRef } from 'react';
import type { Todo, CreateTodoInput, ConnectionStatus, SubscriptionEvent } from '../types/todo';
import { 
  createKalamClient, 
  getKalamConfig,
  type KalamDBClient 
} from '../services/kalamdb';
import {
  loadTodosFromCache,
  saveTodosToCache,
  getLastSyncId,
  updateLastSyncId
} from '../services/localStorage';

/**
 * Hook return value
 */
export interface UseTodosResult {
  /** Current TODO list */
  todos: Todo[];
  
  /** WebSocket connection status */
  connectionStatus: ConnectionStatus;
  
  /** Add a new TODO */
  addTodo: (title: string) => Promise<void>;
  
  /** Delete a TODO by ID */
  deleteTodo: (id: number) => Promise<void>;
  
  /** Toggle TODO completion status */
  toggleTodo: (id: number) => Promise<void>;
  
  /** Loading state (initial data fetch) */
  isLoading: boolean;
  
  /** Error message (if any) */
  error: string | null;
}

/**
 * Custom hook for TODO management with KalamDB
 */
export function useTodos(): UseTodosResult {
  // State
  const [todos, setTodos] = useState<Todo[]>([]);
  const [connectionStatus, setConnectionStatus] = useState<ConnectionStatus>('disconnected');
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  
  // Refs (to avoid re-creating callbacks)
  const clientRef = useRef<KalamDBClient | null>(null);
  const mountedRef = useRef(true);

  /**
   * Handle subscription events (insert/update/delete)
   */
  const handleSubscriptionEvent = useCallback((event: SubscriptionEvent<Todo>) => {
    console.log('Subscription event:', event);

    switch (event.type) {
      case 'insert':
        if (event.data) {
          setTodos(prev => {
            // Avoid duplicates
            if (prev.some(t => t.id === event.data!.id)) {
              return prev;
            }
            const updated = [...prev, event.data!];
            saveTodosToCache(updated);
            updateLastSyncId(event.id);
            return updated;
          });
        }
        break;

      case 'update':
        if (event.data) {
          setTodos(prev => {
            const updated = prev.map(t => t.id === event.id ? event.data! : t);
            saveTodosToCache(updated);
            updateLastSyncId(event.id);
            return updated;
          });
        }
        break;

      case 'delete':
        setTodos(prev => {
          const updated = prev.filter(t => t.id !== event.id);
          saveTodosToCache(updated);
          updateLastSyncId(event.id);
          return updated;
        });
        break;

      default:
        console.warn('Unknown event type:', event.type);
    }
  }, []);

  /**
   * Initialize KalamDB client and subscribe to TODOs
   */
  useEffect(() => {
    let isCancelled = false;

    async function init() {
      try {
        // 1. Load from cache immediately (instant render)
        const cached = loadTodosFromCache();
        if (cached.length > 0 && !isCancelled) {
          console.log(`Loaded ${cached.length} TODOs from cache`);
          setTodos(cached);
          setIsLoading(false);
        }

        // 2. Get configuration
        const config = getKalamConfig();

        // 3. Connect to KalamDB
        if (!isCancelled) {
          setConnectionStatus('connecting');
        }
        
        const client = await createKalamClient(config);
        
        if (isCancelled) {
          await client.disconnect();
          return;
        }

        clientRef.current = client;
        setConnectionStatus('connected');
        setError(null);

        // 4. Subscribe to TODO table from last sync ID
        const lastId = getLastSyncId();
        console.log(`Subscribing to todos from ID ${lastId}`);
        
        await client.subscribe('todos', lastId, handleSubscriptionEvent);

        // 5. If no cache, fetch all TODOs
        if (cached.length === 0) {
          const allTodos = await client.query<Todo>('SELECT * FROM todos ORDER BY id');
          if (!isCancelled) {
            setTodos(allTodos);
            saveTodosToCache(allTodos);
            
            // Update last sync ID to highest ID
            if (allTodos.length > 0) {
              const maxId = Math.max(...allTodos.map(t => t.id));
              updateLastSyncId(maxId);
            }
          }
        }

        setIsLoading(false);
      } catch (err) {
        if (!isCancelled) {
          const message = err instanceof Error ? err.message : 'Failed to connect to KalamDB';
          console.error('Init error:', err);
          setError(message);
          setConnectionStatus('error');
          setIsLoading(false);
        }
      }
    }

    init();

    // Cleanup on unmount
    return () => {
      isCancelled = true;
      mountedRef.current = false;
      
      if (clientRef.current) {
        clientRef.current.disconnect().catch(console.error);
        clientRef.current = null;
      }
    };
  }, [handleSubscriptionEvent]);

  /**
   * Add a new TODO
   */
  const addTodo = useCallback(async (title: string) => {
    if (!clientRef.current || connectionStatus !== 'connected') {
      throw new Error('Not connected to KalamDB');
    }

    if (!title.trim()) {
      throw new Error('Title cannot be empty');
    }

    if (title.length > 500) {
      throw new Error('Title must be 500 characters or less');
    }

    const input: CreateTodoInput = {
      title: title.trim(),
      completed: false
    };

    try {
      await clientRef.current.insertTodo(input);
      // Note: The actual TODO will be added via subscription event
      // so we don't update state here to avoid duplicates
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to add TODO';
      console.error('Add TODO error:', err);
      throw new Error(message);
    }
  }, [connectionStatus]);

  /**
   * Delete a TODO by ID
   */
  const deleteTodo = useCallback(async (id: number) => {
    if (!clientRef.current || connectionStatus !== 'connected') {
      throw new Error('Not connected to KalamDB');
    }

    try {
      await clientRef.current.deleteTodo(id);
      // Note: The TODO will be removed via subscription event
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to delete TODO';
      console.error('Delete TODO error:', err);
      throw new Error(message);
    }
  }, [connectionStatus]);

  /**
   * Toggle TODO completion status
   */
  const toggleTodo = useCallback(async (id: number) => {
    if (!clientRef.current || connectionStatus !== 'connected') {
      throw new Error('Not connected to KalamDB');
    }

    const todo = todos.find(t => t.id === id);
    if (!todo) {
      throw new Error('TODO not found');
    }

    try {
      await clientRef.current.query(
        `UPDATE todos SET completed = ${!todo.completed} WHERE id = ${id}`
      );
      // Note: The TODO will be updated via subscription event
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to toggle TODO';
      console.error('Toggle TODO error:', err);
      throw new Error(message);
    }
  }, [connectionStatus, todos]);

  return {
    todos,
    connectionStatus,
    addTodo,
    deleteTodo,
    toggleTodo,
    isLoading,
    error
  };
}
