'use client';

/**
 * Client-side providers wrapper
 * 
 * Wraps the app with KalamDBProvider for real-time subscriptions.
 * Separated from layout.tsx because providers require 'use client'.
 */

import { KalamDBProvider } from '@/providers/kalamdb-provider';
import { KALAMDB_CONFIG } from '@/lib/config';

export function Providers({ children }: { children: React.ReactNode }) {
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
