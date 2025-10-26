/**
 * localStorage service for TODO app caching
 * Feature: 006-docker-wasm-examples
 * 
 * Provides persistent caching of TODOs and sync state
 * for instant initial render and offline capabilities.
 */

import type { Todo, TodoCache } from '../types/todo';

const CACHE_KEY = 'kalamdb_todos_cache';
const LAST_SYNC_ID_KEY = 'kalamdb_last_sync_id';
// const CACHE_VERSION = 1; // Reserved for future cache versioning

/**
 * Check if localStorage is available
 */
function isLocalStorageAvailable(): boolean {
  try {
    const test = '__localStorage_test__';
    localStorage.setItem(test, test);
    localStorage.removeItem(test);
    return true;
  } catch (e) {
    console.warn('localStorage is not available:', e);
    return false;
  }
}

/**
 * Load TODOs from localStorage cache
 * @returns Cached TODOs or empty array if cache miss
 */
export function loadTodosFromCache(): Todo[] {
  if (!isLocalStorageAvailable()) {
    return [];
  }

  try {
    const cached = localStorage.getItem(CACHE_KEY);
    if (!cached) {
      return [];
    }

    const cache: TodoCache = JSON.parse(cached);
    
    // Validate cache structure
    if (!Array.isArray(cache.todos)) {
      console.warn('Invalid cache structure, clearing');
      clearCache();
      return [];
    }

    // Check cache age (invalidate if older than 7 days)
    const age = Date.now() - (cache.timestamp || 0);
    const MAX_AGE = 7 * 24 * 60 * 60 * 1000; // 7 days in ms
    if (age > MAX_AGE) {
      console.info('Cache expired, clearing');
      clearCache();
      return [];
    }

    return cache.todos;
  } catch (error) {
    console.error('Failed to load cache:', error);
    clearCache();
    return [];
  }
}

/**
 * Save TODOs to localStorage cache
 * @param todos - Array of TODOs to cache
 */
export function saveTodosToCache(todos: Todo[]): void {
  if (!isLocalStorageAvailable()) {
    return;
  }

  try {
    const cache: TodoCache = {
      todos,
      lastSyncId: getLastSyncId(),
      timestamp: Date.now()
    };

    localStorage.setItem(CACHE_KEY, JSON.stringify(cache));
  } catch (error) {
    // localStorage quota exceeded or other error
    console.error('Failed to save cache:', error);
    
    // Try to clear old cache and retry once
    try {
      clearCache();
      const cache: TodoCache = {
        todos,
        lastSyncId: getLastSyncId(),
        timestamp: Date.now()
      };
      localStorage.setItem(CACHE_KEY, JSON.stringify(cache));
    } catch (retryError) {
      console.error('Failed to save cache after clearing:', retryError);
    }
  }
}

/**
 * Get the last synchronized TODO ID
 * Used to subscribe from this ID onwards on reconnection
 * @returns Last sync ID or 0 if none
 */
export function getLastSyncId(): number {
  if (!isLocalStorageAvailable()) {
    return 0;
  }

  try {
    const value = localStorage.getItem(LAST_SYNC_ID_KEY);
    return value ? parseInt(value, 10) : 0;
  } catch (error) {
    console.error('Failed to get last sync ID:', error);
    return 0;
  }
}

/**
 * Set the last synchronized TODO ID
 * @param id - Last synchronized ID
 */
export function setLastSyncId(id: number): void {
  if (!isLocalStorageAvailable()) {
    return;
  }

  try {
    localStorage.setItem(LAST_SYNC_ID_KEY, id.toString());
  } catch (error) {
    console.error('Failed to set last sync ID:', error);
  }
}

/**
 * Update last sync ID if the given ID is higher
 * @param id - Candidate ID to update to
 */
export function updateLastSyncId(id: number): void {
  const current = getLastSyncId();
  if (id > current) {
    setLastSyncId(id);
  }
}

/**
 * Clear all cached data
 */
export function clearCache(): void {
  if (!isLocalStorageAvailable()) {
    return;
  }

  try {
    localStorage.removeItem(CACHE_KEY);
    localStorage.removeItem(LAST_SYNC_ID_KEY);
  } catch (error) {
    console.error('Failed to clear cache:', error);
  }
}

/**
 * Get cache statistics
 * @returns Cache info for debugging
 */
export function getCacheInfo(): {
  hasTodos: boolean;
  todoCount: number;
  lastSyncId: number;
  cacheAge: number | null;
} {
  const todos = loadTodosFromCache();
  const lastSyncId = getLastSyncId();
  
  let cacheAge: number | null = null;
  if (isLocalStorageAvailable()) {
    try {
      const cached = localStorage.getItem(CACHE_KEY);
      if (cached) {
        const cache: TodoCache = JSON.parse(cached);
        cacheAge = Date.now() - (cache.timestamp || 0);
      }
    } catch (error) {
      // Ignore
    }
  }

  return {
    hasTodos: todos.length > 0,
    todoCount: todos.length,
    lastSyncId,
    cacheAge
  };
}
