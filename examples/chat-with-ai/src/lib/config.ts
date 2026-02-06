// KalamDB client hook configuration for browser-side real-time subscriptions
// Uses kalam-link SDK for WebSocket subscriptions

export const KALAMDB_CONFIG = {
  url: process.env.NEXT_PUBLIC_KALAMDB_URL || 'http://localhost:8080',
  username: process.env.NEXT_PUBLIC_KALAMDB_USERNAME || 'demo-user',
  password: process.env.NEXT_PUBLIC_KALAMDB_PASSWORD || 'demo-password-123',
} as const;
