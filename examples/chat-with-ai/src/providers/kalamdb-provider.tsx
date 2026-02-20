'use client';

/**
 * KalamDB Provider - React context for kalam-link SDK
 * 
 * Provides a shared KalamDBClient instance with:
 * - Token-based auth (Keycloak JWT) OR Basic Auth fallback
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

// ---- Token-based auth props (e.g. Keycloak JWT) ----
interface TokenAuthProps {
  /** KalamDB server URL */
  url: string;
  /** JWT access token from an external OIDC provider */
  token: string;
  children: React.ReactNode;
  username?: undefined;
  password?: undefined;
}

// ---- Basic auth props (legacy / fallback) ----
interface BasicAuthProps {
  /** KalamDB server URL */
  url: string;
  /** Username for authentication */
  username: string;
  /** Password for authentication */
  password: string;
  children: React.ReactNode;
  token?: undefined;
}

type KalamDBProviderProps = TokenAuthProps | BasicAuthProps;

export function KalamDBProvider(props: KalamDBProviderProps) {
  const { url, children } = props;

  const [connectionStatus, setConnectionStatus] = useState<ConnectionStatus>('connecting');
  const [isReady, setIsReady] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const clientRef = useRef<KalamDBClient | null>(null);
  const mountedRef = useRef(true);
  const initializingRef = useRef(false);

  // Derive a stable key so we re-initialise when credentials change
  const authKey = props.token
    ? `jwt:${props.token.slice(0, 16)}`
    : `basic:${props.username}:${props.password}`;

  useEffect(() => {
    mountedRef.current = true;
    console.log('[KalamDB] Effect triggered, isReady:', isReady);

    if (initializingRef.current) {
      console.log('[KalamDB] Already initializing, waiting...');
      return;
    }

    // When the token changes we need to re-create the client
    // so always discard previous client when authKey changes
    if (clientRef.current) {
      console.log('[KalamDB] Auth changed, tearing down old client');
      const old = clientRef.current;
      clientRef.current = null;
      setIsReady(false);
      setConnectionStatus('connecting');
      old.disconnect().catch(() => {});
    }

    initializingRef.current = true;
    console.log('[KalamDB] Starting initialization...');

    const initClient = async () => {
      try {
        console.log('[KalamDB] URL:', url);

        const auth = props.token
          ? Auth.jwt(props.token)
          : Auth.basic(props.username!, props.password!);

        const client = createClient({
          url,
          auth,
          wasmUrl: '/wasm/kalam_link_bg.wasm',
        });

        clientRef.current = client;
        setConnectionStatus('connecting');

        // Token auth: skip login(), go straight to connect()
        // Basic auth: call login() first to obtain a JWT
        if (!props.token) {
          console.log('[KalamDB] Logging in with Basic auth...');
          await client.login();
          console.log('[KalamDB] Basic auth login successful');
        } else {
          console.log('[KalamDB] Using external JWT, skipping login()');
        }

        console.log('[KalamDB] Connecting WebSocket...');
        await client.connect();
        console.log('[KalamDB] WebSocket connected');

        client.setAutoReconnect(true);
        client.setReconnectDelay(1000, 30000);
        client.setMaxReconnectAttempts(0);

        if (clientRef.current !== client) {
          console.warn('[KalamDB] Client was replaced, skipping state update');
          return;
        }

        setConnectionStatus('connected');
        setIsReady(true);
        setError(null);
        console.log('[KalamDB] ✅ Connected and ready!');
      } catch (err) {
        console.error('[KalamDB] Connection failed:', err);

        if (!clientRef.current) return;

        setConnectionStatus('error');
        setError(err instanceof Error ? err.message : 'Connection failed');
        setIsReady(true); // query-only fallback
      } finally {
        initializingRef.current = false;
      }
    };

    initClient();

    return () => {
      mountedRef.current = false;

      if (process.env.NODE_ENV === 'development') {
        console.log('[KalamDB] Cleanup: Strict Mode, keeping client alive');
        return;
      }

      const currentClient = clientRef.current;
      clientRef.current = null;
      initializingRef.current = false;
      if (currentClient) {
        setTimeout(() => currentClient.disconnect().catch(() => {}), 100);
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [url, authKey]);

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
