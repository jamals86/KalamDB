'use client';

/**
 * Client-side providers wrapper
 * 
 * Wraps the app with KalamDBProvider for real-time subscriptions.
 * Separated from layout.tsx because providers require 'use client'.
 */

import { KalamDBProvider } from '@/providers/kalamdb-provider';

const KALAMDB_URL = process.env.NEXT_PUBLIC_KALAMDB_URL || 'http://localhost:8080';
const KALAMDB_USERNAME = process.env.NEXT_PUBLIC_KALAMDB_USERNAME || 'admin';
const KALAMDB_PASSWORD = process.env.NEXT_PUBLIC_KALAMDB_PASSWORD || 'kalamdb123';

export function Providers({ children }: { children: React.ReactNode }) {
  return (
    <KalamDBProvider
      url={KALAMDB_URL}
      username={KALAMDB_USERNAME}
      password={KALAMDB_PASSWORD}
    >
      {children}
    </KalamDBProvider>
  );
}
