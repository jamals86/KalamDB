/**
 * KalamDB Client wrapper for Admin UI
 * 
 * Uses the kalam-link SDK with JWT authentication.
 * The JWT token is obtained from the cookie-based auth flow,
 * then used for all SQL queries via the WASM SDK.
 * 
 * This is modeled after the example at link/sdks/typescript/example/app.js
 * which demonstrates proper WASM initialization and query execution.
 */

import { KalamDBClient, Auth, type QueryResponse, type ServerMessage, type Unsubscribe } from 'kalam-link';

let client: KalamDBClient | null = null;
let currentToken: string | null = null;
let isInitialized = false;
let initializationPromise: Promise<KalamDBClient> | null = null;

/**
 * Get the backend URL
 * In development, Vite runs on port 5173 but backend is on 8080
 * In production, both are served from the same origin
 */
function getBackendUrl(): string {
  if (import.meta.env.DEV) {
    return 'http://localhost:8080';
  }
  return window.location.origin;
}

/**
 * Initialize the KalamDB client with JWT token
 * 
 * Creates a new client with JWT auth and initializes the WASM module.
 * Uses a promise lock to prevent concurrent initialization (which causes WASM crashes).
 */
export async function initializeClient(jwtToken: string): Promise<KalamDBClient> {
  // If same token and already initialized, return existing client
  if (jwtToken === currentToken && client && isInitialized) {
    console.log('[kalam-client] Returning existing initialized client');
    return client;
  }
  
  // If initialization is in progress with same token, wait for it
  if (initializationPromise && jwtToken === currentToken) {
    console.log('[kalam-client] Waiting for ongoing initialization...');
    return initializationPromise;
  }
  
  // Start new initialization
  console.log('[kalam-client] initializeClient called');
  
  initializationPromise = (async () => {
    try {
      // If token changed, disconnect old client
      if (client && jwtToken !== currentToken) {
        console.log('[kalam-client] Token changed, disconnecting old client');
        try {
          await client.disconnect();
        } catch {
          // Ignore disconnect errors
        }
        client = null;
        isInitialized = false;
      }
      
      // Create new client with JWT auth
      console.log('[kalam-client] Creating new KalamDBClient with JWT auth');
      client = new KalamDBClient({
        url: getBackendUrl(),
        auth: Auth.jwt(jwtToken),
      });
      currentToken = jwtToken;
      
      // Initialize WASM (must be done before queries)
      console.log('[kalam-client] Initializing WASM...');
      await client.initialize();
      isInitialized = true;
      console.log('[kalam-client] WASM initialized successfully');
      
      return client;
    } finally {
      initializationPromise = null;
    }
  })();
  
  return initializationPromise;
}

/**
 * Get the current client instance (must be initialized first)
 */
export function getClient(): KalamDBClient | null {
  return isInitialized ? client : null;
}

/**
 * Set the JWT token for the client (called on login/refresh)
 * Creates and initializes the client immediately
 */
export async function setClientToken(token: string): Promise<void> {
  console.log('[kalam-client] setClientToken called');
  await initializeClient(token);
}

/**
 * Clear the client when user logs out
 */
export async function clearClient(): Promise<void> {
  console.log('[kalam-client] clearClient called');
  if (client) {
    try {
      await client.disconnect();
    } catch {
      // Ignore disconnect errors on logout
    }
  }
  client = null;
  currentToken = null;
  isInitialized = false;
  initializationPromise = null;
}

/**
 * Get current token (for debugging)
 */
export function getCurrentToken(): string | null {
  return currentToken;
}

/**
 * Execute SQL query using the WASM SDK
 * Returns the full QueryResponse
 */
export async function executeQuery(sql: string): Promise<QueryResponse> {
  if (!currentToken) {
    throw new Error('Not authenticated. Please log in first.');
  }
  
  if (!client || !isInitialized) {
    console.log('[kalam-client] Client not initialized, initializing now...');
    await initializeClient(currentToken);
  }
  
  console.log('[kalam-client] Executing query via SDK:', sql.substring(0, 50) + (sql.length > 50 ? '...' : ''));
  return client!.query(sql);
}

/**
 * Execute SQL and return rows from the first result set
 * Convenience function for hooks that just need rows
 */
export async function executeSql(sql: string): Promise<Record<string, unknown>[]> {
  const response = await executeQuery(sql);
  
  if (response.status === 'error' && response.error) {
    throw new Error(response.error.message);
  }
  
  return (response.results?.[0]?.rows as Record<string, unknown>[]) ?? [];
}

/**
 * Insert data into a table using SDK's insert method
 */
export async function insert(tableName: string, data: Record<string, unknown>): Promise<QueryResponse> {
  if (!currentToken) {
    throw new Error('Not authenticated. Please log in first.');
  }
  
  if (!client || !isInitialized) {
    await initializeClient(currentToken);
  }
  
  return client!.insert(tableName, data);
}

/**
 * Delete a row from a table using SDK's delete method
 */
export async function deleteRow(tableName: string, rowId: string | number): Promise<void> {
  if (!currentToken) {
    throw new Error('Not authenticated. Please log in first.');
  }
  
  if (!client || !isInitialized) {
    await initializeClient(currentToken);
  }
  
  return client!.delete(tableName, rowId);
}

/**
 * Check if client is connected (WebSocket)
 */
export function isClientConnected(): boolean {
  return client !== null && isInitialized && client.isConnected();
}

/**
 * Connect WebSocket for real-time subscriptions
 */
export async function connectWebSocket(): Promise<void> {
  if (!currentToken) {
    throw new Error('Not authenticated. Please log in first.');
  }
  
  if (!client || !isInitialized) {
    await initializeClient(currentToken);
  }
  
  await client!.connect();
}

/**
 * Subscribe to table changes
 * Returns an unsubscribe function (Firebase/Supabase style)
 */
export async function subscribe(
  tableOrQuery: string,
  callback: (event: ServerMessage) => void
): Promise<Unsubscribe> {
  if (!currentToken) {
    throw new Error('Not authenticated. Please log in first.');
  }
  
  if (!client || !isInitialized) {
    await initializeClient(currentToken);
  }
  
  // Ensure WebSocket is connected
  if (!client!.isConnected()) {
    await client!.connect();
  }
  
  return client!.subscribe(tableOrQuery, callback);
}

/**
 * Get subscription count
 */
export function getSubscriptionCount(): number {
  return client?.getSubscriptionCount() ?? 0;
}

// Re-export types for convenience
export type { QueryResponse, ServerMessage, Unsubscribe };
