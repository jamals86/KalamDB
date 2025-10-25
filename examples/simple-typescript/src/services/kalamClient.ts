/**
 * KalamDB WASM Client Service
 * Wraps the WASM client initialization and provides a singleton instance
 */

import init, { KalamClient } from '../wasm/kalam_link.js';

let client: KalamClient | null = null;
let initialized = false;

/**
 * Initialize the WASM module and create KalamClient instance
 */
export async function initializeClient(): Promise<KalamClient> {
  if (client) {
    return client;
  }

  if (!initialized) {
    // Initialize WASM module
    await init();
    initialized = true;
  }

  // Get configuration from environment variables
  const url = import.meta.env.VITE_KALAMDB_URL || 'http://localhost:8080';
  const apiKey = import.meta.env.VITE_KALAMDB_API_KEY;

  if (!apiKey || apiKey === 'your-api-key-here') {
    throw new Error(
      'VITE_KALAMDB_API_KEY is not set. Please create a .env file with your API key.'
    );
  }

  // Create client instance
  client = new KalamClient(url, apiKey);
  
  return client;
}

/**
 * Get the KalamClient instance (must call initializeClient first)
 */
export function getClient(): KalamClient {
  if (!client) {
    throw new Error('KalamClient not initialized. Call initializeClient() first.');
  }
  return client;
}
