/**
 * LocalStorage Service
 * Handles reading/writing TODOs and sync state to browser localStorage
 */

import type { Todo } from '../types/todo';

const TODOS_KEY = 'kalamdb_todos';
const LAST_SYNC_ID_KEY = 'kalamdb_last_sync_id';

/**
 * Load TODOs from localStorage cache
 */
export function loadTodosFromCache(): Todo[] {
  try {
    const data = localStorage.getItem(TODOS_KEY);
    if (!data) {
      return [];
    }
    return JSON.parse(data) as Todo[];
  } catch (error) {
    console.error('Failed to load TODOs from cache:', error);
    return [];
  }
}

/**
 * Save TODOs to localStorage cache
 */
export function saveTodosToCache(todos: Todo[]): void {
  try {
    localStorage.setItem(TODOS_KEY, JSON.stringify(todos));
  } catch (error) {
    console.error('Failed to save TODOs to cache:', error);
  }
}

/**
 * Get the last sync ID (for resuming subscriptions)
 */
export function getLastSyncId(): number | null {
  try {
    const data = localStorage.getItem(LAST_SYNC_ID_KEY);
    if (!data) {
      return null;
    }
    return parseInt(data, 10);
  } catch (error) {
    console.error('Failed to get last sync ID:', error);
    return null;
  }
}

/**
 * Set the last sync ID
 */
export function setLastSyncId(id: number): void {
  try {
    localStorage.setItem(LAST_SYNC_ID_KEY, id.toString());
  } catch (error) {
    console.error('Failed to set last sync ID:', error);
  }
}

/**
 * Clear all cached data (useful for reset/logout)
 */
export function clearCache(): void {
  try {
    localStorage.removeItem(TODOS_KEY);
    localStorage.removeItem(LAST_SYNC_ID_KEY);
  } catch (error) {
    console.error('Failed to clear cache:', error);
  }
}
