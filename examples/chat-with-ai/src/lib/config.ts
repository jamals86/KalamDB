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

export const KALAMDB_CONFIG = {
  url: requiredPublicEnv(
    process.env.NEXT_PUBLIC_KALAMDB_URL,
    'NEXT_PUBLIC_KALAMDB_URL',
    'http://localhost:8080',
  ),
  username: requiredPublicEnv(
    process.env.NEXT_PUBLIC_KALAMDB_USERNAME,
    'NEXT_PUBLIC_KALAMDB_USERNAME',
    'test-user',
  ),
  password: requiredPublicEnv(
    process.env.NEXT_PUBLIC_KALAMDB_PASSWORD,
    'NEXT_PUBLIC_KALAMDB_PASSWORD',
    'test-pass',
  ),
} as const;
