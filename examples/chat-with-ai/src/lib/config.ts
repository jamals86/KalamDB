// KalamDB client hook configuration for browser-side real-time subscriptions
// Uses kalam-link SDK for WebSocket subscriptions

function requiredPublicEnv(value: string | undefined, name: string, testFallback: string): string {
  if (value && value.trim().length > 0) {
    return value;
  }

  if (process.env.NODE_ENV === 'test') {
    return testFallback;
  }

  throw new Error(`Missing required env var: ${name}. Add it to .env.local`);
}

function optionalPublicEnv(value: string | undefined, fallback: string): string {
  if (value && value.trim().length > 0) {
    return value;
  }
  return fallback;
}

export const KALAMDB_CONFIG = {
  url: requiredPublicEnv(
    process.env.NEXT_PUBLIC_KALAMDB_URL,
    'NEXT_PUBLIC_KALAMDB_URL',
    'http://localhost:8080',
  ),
  username: optionalPublicEnv(
    process.env.NEXT_PUBLIC_KALAMDB_USERNAME,
    'admin',
  ),
  password: optionalPublicEnv(
    process.env.NEXT_PUBLIC_KALAMDB_PASSWORD,
    'kalamdb123',
  ),
} as const;

export const KEYCLOAK_CONFIG = {
  url: optionalPublicEnv(
    process.env.NEXT_PUBLIC_KEYCLOAK_URL,
    'http://localhost:8081',
  ),
  realm: optionalPublicEnv(
    process.env.NEXT_PUBLIC_KEYCLOAK_REALM,
    'kalamdb',
  ),
  clientId: optionalPublicEnv(
    process.env.NEXT_PUBLIC_KEYCLOAK_CLIENT_ID,
    'kalamdb-chat',
  ),
  /** When true the app uses Keycloak for authentication instead of Basic Auth */
  enabled:
    optionalPublicEnv(process.env.NEXT_PUBLIC_AUTH_MODE, 'keycloak') === 'keycloak',
} as const;
