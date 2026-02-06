'use client';

/**
 * KalamDB Provider - React context for kalam-link SDK
 * 
 * Provides a shared KalamDBClient instance with:
 * - Automatic login (Basic Auth → JWT)
 * - WebSocket connection for real-time subscriptions
 * - Auto-reconnection support
 * - Connection status tracking
 */

import React, { createContext, useContext, useEffect, useState, useRef, useCallback } from 'react';
import { createClient, Auth, type KalamDBClient } from 'kalam-link';
import type { ConnectionStatus } from '@/types';

interface KalamDBContextValue {
  /** The kalam-link client instance (null until connected) */
  client: KalamDBClient | null;
  /** Current connection status */
  connectionStatus: ConnectionStatus;
  /** Whether the client is ready for queries and subscriptions */
  isReady: boolean;
  /** Connection error message, if any */
  error: string | null;
}

const KalamDBContext = createContext<KalamDBContextValue>({
  client: null,
  connectionStatus: 'connecting',
  isReady: false,
  error: null,
});

/**
 * Hook to access the kalam-link client from any component
 */
export function useKalamDB(): KalamDBContextValue {
  return useContext(KalamDBContext);
}

interface KalamDBProviderProps {
  /** KalamDB server URL */
  url: string;
  /** Username for authentication */
  username: string;
  /** Password for authentication */
  password: string;
  children: React.ReactNode;
}

export function KalamDBProvider({ url, username, password, children }: KalamDBProviderProps) {
  const [connectionStatus, setConnectionStatus] = useState<ConnectionStatus>('connecting');
  const [isReady, setIsReady] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const clientRef = useRef<KalamDBClient | null>(null);
  const mountedRef = useRef(true);
  const initializingRef = useRef(false); // Prevent concurrent initialization

  useEffect(() => {
    // React 18 Strict Mode: effect runs twice, but cleanup runs DURING async operations
    // Solution: always reset mountedRef to true when effect runs
    mountedRef.current = true;
    console.log('[KalamDB] Effect triggered, mountedRef set to true, isReady:', isReady);

    // If already initializing, just restore mountedRef and wait
    if (initializingRef.current) {
      console.log('[KalamDB] Already initializing, mountedRef restored, waiting...');
      return;
    }

    // If client already exists and is ready, don't reinit
    if (clientRef.current && isReady) {
      console.log('[KalamDB] Client already ready, skipping init');
      return;
    }

    initializingRef.current = true;
    console.log('[KalamDB] Starting initialization...');

    const initClient = async () => {
      try {
        console.log('[KalamDB] Starting client initialization...');
        console.log('[KalamDB] URL:', url);
        console.log('[KalamDB] Username:', username);
        
        // Create client with basic auth credentials
        const client = createClient({
          url,
          auth: Auth.basic(username, password),
          wasmUrl: '/wasm/kalam_link_bg.wasm',
        });

        console.log('[KalamDB] Client created, setting clientRef');
        clientRef.current = client;

        setConnectionStatus('connecting');

        // Login to get JWT token (required for WebSocket)
        console.log('[KalamDB] Logging in...');
        await client.login();
        console.log('[KalamDB] Login successful');
        
        // Don't return early - we need to connect even if unmounting
        // The cleanup will handle disconnection properly

        // Connect WebSocket (initializes WASM)
        console.log('[KalamDB] Connecting WebSocket...');
        await client.connect();
        console.log('[KalamDB] WebSocket connected');

        // Configure auto-reconnect (must be AFTER connect, needs WASM initialized)
        client.setAutoReconnect(true);
        client.setReconnectDelay(1000, 30000);
        client.setMaxReconnectAttempts(0); // Infinite retries
        
        // Check if client is still valid (not disconnected/removed)
        // In Strict Mode, mountedRef may be false but component is remounting
        console.log('[KalamDB] Checking if client still valid:', clientRef.current === client);
        if (clientRef.current !== client) {
          console.warn('[KalamDB] Client was replaced, skipping state update');
          return;
        }

        console.log('[KalamDB] Setting connection status to connected');
        setConnectionStatus('connected');
        console.log('[KalamDB] Setting isReady to true');
        setIsReady(true);
        console.log('[KalamDB] Clearing error');
        setError(null);

        console.log('[KalamDB] ✅ Connected and ready for subscriptions!');
      } catch (err) {
        console.error('[KalamDB] Connection failed:', err);
        console.error('[KalamDB] Error stack:', err instanceof Error ? err.stack : 'no stack');
        
        // Check if client is still valid
        console.log('[KalamDB] Checking if client still valid after error:', !!clientRef.current);
        if (!clientRef.current) {
          console.warn('[KalamDB] Client was removed, skipping error state update');
          return;
        }
        
        setConnectionStatus('error');
        setError(err instanceof Error ? err.message : 'Connection failed');

        // Even if WebSocket fails, the client can still do HTTP queries
        // Mark as ready for query-only mode
        console.log('[KalamDB] Setting isReady to true (query-only mode)');
        setIsReady(true);
      } finally {
        initializingRef.current = false;
        console.log('[KalamDB] Initialization complete, initializingRef reset');
      }
    };

    initClient();

    return () => {
      console.log('[KalamDB] Cleanup triggered');
      // In Strict Mode, cleanup runs during async operations
      // We'll set mountedRef to false temporarily, but the next effect will restore it
      // Only actually cleanup if this is a real unmount (component removed from DOM)
      const wasReady = isReady;
      mountedRef.current = false;
      
      // In development (Strict Mode), keep client alive and don't reset initialization
      // The next effect run will restore mountedRef and reuse the client
      if (process.env.NODE_ENV === 'development') {
        console.log('[KalamDB] Cleanup: Strict Mode detected, keeping client alive');
        return;
      }
      
      // Production cleanup: actually disconnect
      console.log('[KalamDB] Cleanup: Production mode, cleaning up...');
      const currentClient = clientRef.current;
      clientRef.current = null;
      initializingRef.current = false;
      
      if (currentClient) {
        setTimeout(() => {
          currentClient.disconnect().catch(() => {});
        }, 100);
      }
    };
  }, [url, username, password]);

  return (
    <KalamDBContext.Provider
      value={{
        client: clientRef.current,
        connectionStatus,
        isReady,
        error,
      }}
    >
      {children}
    </KalamDBContext.Provider>
  );
}
