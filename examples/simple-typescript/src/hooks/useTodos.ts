/**
 * useTodos Hook
 * Custom React hook for managing TODO state with real-time sync and localStorage caching
 */

import { useState, useEffect } from 'react';
import type { Todo, NewTodo } from '../types/todo';
import { initializeClient, getClient } from '../services/kalamClient';
import {
  loadTodosFromCache,
  saveTodosToCache,
  getLastSyncId,
  setLastSyncId,
} from '../services/localStorage';

interface UseTodosReturn {
  todos: Todo[];
  isConnected: boolean;
  isLoading: boolean;
  error: string | null;
  addTodo: (newTodo: NewTodo) => Promise<void>;
  deleteTodo: (id: number) => Promise<void>;
}

export function useTodos(): UseTodosReturn {
  const [todos, setTodos] = useState<Todo[]>([]);
  const [isConnected, setIsConnected] = useState(false);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [subscriptionId, setSubscriptionId] = useState<string | null>(null);

  // Initialize client and load data
  useEffect(() => {
    let mounted = true;

    async function init() {
      try {
        // Load from cache immediately for instant UI
        const cachedTodos = loadTodosFromCache();
        if (mounted) {
          setTodos(cachedTodos);
          setIsLoading(false);
        }

        // Initialize WASM client
        const client = await initializeClient();

        // Connect to server
        await client.connect();
        
        if (mounted) {
          setIsConnected(client.isConnected());
        }

        // Subscribe to changes from last sync point
        const lastSyncId = getLastSyncId();
        const subId = await client.subscribe('app.todos', (event: string) => {
          if (!mounted) return;

          try {
            const eventData = JSON.parse(event);
            
            // Update last sync ID
            if (eventData.id) {
              setLastSyncId(eventData.id);
            }

            // Handle different event types
            if (eventData.type === 'insert' || eventData.type === 'update') {
              const newTodo = eventData.data as Todo;
              setTodos((prev) => {
                // Check if TODO already exists (update case)
                const index = prev.findIndex((t) => t.id === newTodo.id);
                let updated: Todo[];
                
                if (index >= 0) {
                  // Update existing
                  updated = [...prev];
                  updated[index] = newTodo;
                } else {
                  // Add new
                  updated = [...prev, newTodo];
                }
                
                // Save to cache
                saveTodosToCache(updated);
                return updated;
              });
            } else if (eventData.type === 'delete') {
              const deletedId = eventData.id || eventData.data?.id;
              setTodos((prev) => {
                const updated = prev.filter((t) => t.id !== deletedId);
                saveTodosToCache(updated);
                return updated;
              });
            }
          } catch (err) {
            console.error('Failed to process subscription event:', err);
          }
        });

        if (mounted) {
          setSubscriptionId(subId);
        }
      } catch (err) {
        console.error('Failed to initialize:', err);
        if (mounted) {
          setError(err instanceof Error ? err.message : 'Failed to connect');
          setIsLoading(false);
        }
      }
    }

    init();

    // Cleanup
    return () => {
      mounted = false;
      if (subscriptionId) {
        try {
          const client = getClient();
          client.unsubscribe(subscriptionId);
          client.disconnect();
        } catch (err) {
          console.error('Failed to cleanup:', err);
        }
      }
    };
  }, []);

  /**
   * Add a new TODO
   */
  const addTodo = async (newTodo: NewTodo): Promise<void> => {
    try {
      const client = getClient();
      
      // Prepare data
      const data = {
        title: newTodo.title,
        completed: newTodo.completed ?? false,
        created_at: new Date().toISOString(),
      };

      // Insert via WASM client
      await client.insert('app.todos', JSON.stringify(data));
      
      // The subscription will handle updating the local state
    } catch (err) {
      console.error('Failed to add TODO:', err);
      setError(err instanceof Error ? err.message : 'Failed to add TODO');
      throw err;
    }
  };

  /**
   * Delete a TODO
   */
  const deleteTodo = async (id: number): Promise<void> => {
    try {
      const client = getClient();
      
      // Delete via WASM client
      await client.delete('app.todos', id.toString());
      
      // The subscription will handle updating the local state
    } catch (err) {
      console.error('Failed to delete TODO:', err);
      setError(err instanceof Error ? err.message : 'Failed to delete TODO');
      throw err;
    }
  };

  return {
    todos,
    isConnected,
    isLoading,
    error,
    addTodo,
    deleteTodo,
  };
}
