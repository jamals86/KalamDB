'use client';

/**
 * Client-side providers wrapper
 *
 * When NEXT_PUBLIC_AUTH_MODE=keycloak (default), the app is wrapped with
 * KeycloakProvider → KalamDBProvider (token auth).
 * When NEXT_PUBLIC_AUTH_MODE=basic, the legacy username/password flow is used.
 */

import { KalamDBProvider } from '@/providers/kalamdb-provider';
import { KeycloakProvider, useKeycloak } from '@/providers/keycloak-provider';
import { KALAMDB_CONFIG, KEYCLOAK_CONFIG } from '@/lib/config';

// ---------------------------------------------------------------------------
// Inner bridge: reads the Keycloak token and passes it to KalamDB
// ---------------------------------------------------------------------------

function KeycloakKalamDBBridge({ children }: { children: React.ReactNode }) {
  const { initialized, authenticated, token, user, logout } = useKeycloak();

  if (!initialized) {
    return (
      <div className="flex items-center justify-center min-h-screen">
        <p className="text-muted-foreground animate-pulse">Authenticating with Keycloak…</p>
      </div>
    );
  }

  if (!authenticated || !token) {
    return (
      <div className="flex items-center justify-center min-h-screen">
        <p className="text-muted-foreground">Redirecting to login…</p>
      </div>
    );
  }

  return (
    <KalamDBProvider url={KALAMDB_CONFIG.url} token={token}>
      {children}
    </KalamDBProvider>
  );
}

// ---------------------------------------------------------------------------
// Exported provider tree
// ---------------------------------------------------------------------------

export function Providers({ children }: { children: React.ReactNode }) {
  if (KEYCLOAK_CONFIG.enabled) {
    return (
      <KeycloakProvider
        url={KEYCLOAK_CONFIG.url}
        realm={KEYCLOAK_CONFIG.realm}
        clientId={KEYCLOAK_CONFIG.clientId}
      >
        <KeycloakKalamDBBridge>{children}</KeycloakKalamDBBridge>
      </KeycloakProvider>
    );
  }

  // Fallback: Basic Auth (legacy)
  return (
    <KalamDBProvider
      url={KALAMDB_CONFIG.url}
      username={KALAMDB_CONFIG.username}
      password={KALAMDB_CONFIG.password}
    >
      {children}
    </KalamDBProvider>
  );
}
