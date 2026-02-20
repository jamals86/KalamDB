'use client';

/**
 * Keycloak Authentication Provider
 *
 * Wraps the app with Keycloak OIDC login using keycloak-js.
 * On mount it initialises the Keycloak adapter with `login-required`,
 * which redirects the user to the Keycloak login page automatically.
 *
 * Once authenticated the access token is exposed via context so that
 * downstream providers (e.g. KalamDBProvider) can use it.
 */

import React, {
  createContext,
  useContext,
  useEffect,
  useState,
  useRef,
  useCallback,
} from 'react';
import Keycloak from 'keycloak-js';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface KeycloakUser {
  id: string;
  username: string;
  email?: string;
  firstName?: string;
  lastName?: string;
}

interface KeycloakContextValue {
  /** Whether Keycloak initialisation is still in progress */
  initialized: boolean;
  /** Whether the user is authenticated */
  authenticated: boolean;
  /** Current access token (JWT) – refreshed automatically */
  token: string | null;
  /** Parsed user profile from the ID token */
  user: KeycloakUser | null;
  /** Trigger a logout (redirects to Keycloak) */
  logout: () => void;
}

const KeycloakContext = createContext<KeycloakContextValue>({
  initialized: false,
  authenticated: false,
  token: null,
  user: null,
  logout: () => {},
});

/**
 * Hook to access the Keycloak auth state from any component.
 */
export function useKeycloak(): KeycloakContextValue {
  return useContext(KeycloakContext);
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

interface KeycloakProviderProps {
  /** Keycloak server URL (e.g. http://localhost:8081) */
  url: string;
  /** Keycloak realm name */
  realm: string;
  /** Keycloak client ID */
  clientId: string;
  children: React.ReactNode;
}

/** Minimum remaining token validity (seconds) before a silent refresh. */
const MIN_VALIDITY_SECS = 30;
/** How often (ms) we proactively check/refresh the token. */
const REFRESH_INTERVAL_MS = 10_000;

export function KeycloakProvider({
  url,
  realm,
  clientId,
  children,
}: KeycloakProviderProps) {
  const [initialized, setInitialized] = useState(false);
  const [authenticated, setAuthenticated] = useState(false);
  const [token, setToken] = useState<string | null>(null);
  const [user, setUser] = useState<KeycloakUser | null>(null);
  const keycloakRef = useRef<Keycloak | null>(null);
  const initStartedRef = useRef(false);

  // ---- initialise Keycloak once ----
  useEffect(() => {
    // Guard against React 18 Strict-Mode double-mount
    if (initStartedRef.current) return;
    initStartedRef.current = true;

    const kc = new Keycloak({ url, realm, clientId });
    keycloakRef.current = kc;

    kc.init({
      onLoad: 'login-required',   // redirect to Keycloak login automatically
      checkLoginIframe: false,    // avoid cross-origin iframe issues in dev
      pkceMethod: 'S256',         // use PKCE for public clients
    })
      .then((auth) => {
        console.log('[Keycloak] Initialised, authenticated:', auth);
        setAuthenticated(auth);
        setToken(kc.token ?? null);

        if (auth && kc.tokenParsed) {
          setUser({
            id: kc.tokenParsed.sub ?? '',
            username:
              (kc.tokenParsed as Record<string, unknown>).preferred_username as string ??
              kc.tokenParsed.sub ??
              '',
            email: (kc.tokenParsed as Record<string, unknown>).email as string | undefined,
            firstName: (kc.tokenParsed as Record<string, unknown>).given_name as string | undefined,
            lastName: (kc.tokenParsed as Record<string, unknown>).family_name as string | undefined,
          });
        }

        setInitialized(true);
      })
      .catch((err) => {
        console.error('[Keycloak] Init failed:', err);
        setInitialized(true); // surface the un-authenticated state
      });

    // Listen for token refresh events
    kc.onTokenExpired = () => {
      console.log('[Keycloak] Token expired, refreshing...');
      kc.updateToken(MIN_VALIDITY_SECS)
        .then((refreshed) => {
          if (refreshed) {
            console.log('[Keycloak] Token refreshed');
            setToken(kc.token ?? null);
          }
        })
        .catch(() => {
          console.warn('[Keycloak] Token refresh failed, redirecting to login');
          kc.login();
        });
    };
  }, [url, realm, clientId]);

  // ---- proactive token refresh interval ----
  useEffect(() => {
    if (!authenticated) return;

    const interval = setInterval(() => {
      const kc = keycloakRef.current;
      if (!kc) return;

      kc.updateToken(MIN_VALIDITY_SECS)
        .then((refreshed) => {
          if (refreshed) {
            console.log('[Keycloak] Proactive token refresh succeeded');
            setToken(kc.token ?? null);
          }
        })
        .catch(() => {
          console.warn('[Keycloak] Proactive refresh failed');
        });
    }, REFRESH_INTERVAL_MS);

    return () => clearInterval(interval);
  }, [authenticated]);

  // ---- logout callback ----
  const logout = useCallback(() => {
    keycloakRef.current?.logout({ redirectUri: window.location.origin });
  }, []);

  return (
    <KeycloakContext.Provider
      value={{ initialized, authenticated, token, user, logout }}
    >
      {children}
    </KeycloakContext.Provider>
  );
}
