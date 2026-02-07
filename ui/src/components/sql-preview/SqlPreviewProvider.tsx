/**
 * SqlPreviewProvider - Global React context that allows any component in the
 * admin UI to open the SQL Preview Dialog.
 *
 * Wrap your app with <SqlPreviewProvider> and use the useSqlPreview() hook
 * to trigger the dialog from anywhere.
 *
 * Usage:
 *   // In App.tsx (or root layout)
 *   <SqlPreviewProvider>
 *     <App />
 *   </SqlPreviewProvider>
 *
 *   // In any child component
 *   const { openSqlPreview } = useSqlPreview();
 *   openSqlPreview({
 *     sql: 'DELETE FROM default.users WHERE id = 1;',
 *     title: 'Confirm Delete',
 *     onExecute: async (sql) => { await executeSql(sql); },
 *     onDiscard: () => { changes.discardAll(); },
 *   });
 */

import { createContext, useContext, useState, useCallback, type ReactNode } from 'react';
import { SqlPreviewDialog, type SqlPreviewOptions } from './SqlPreviewDialog';

// ─── Context ─────────────────────────────────────────────────────────────────

interface SqlPreviewContextValue {
  /** Open the SQL preview dialog with the given options. */
  openSqlPreview: (options: SqlPreviewOptions) => void;
  /** Close the dialog programmatically. */
  closeSqlPreview: () => void;
  /** Whether the dialog is currently open. */
  isOpen: boolean;
}

const SqlPreviewContext = createContext<SqlPreviewContextValue | null>(null);

// ─── Provider ────────────────────────────────────────────────────────────────

interface SqlPreviewProviderProps {
  children: ReactNode;
}

export function SqlPreviewProvider({ children }: SqlPreviewProviderProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [options, setOptions] = useState<SqlPreviewOptions | null>(null);

  const openSqlPreview = useCallback((opts: SqlPreviewOptions) => {
    console.log('[SqlPreviewProvider] Opening dialog with options:', {
      title: opts.title,
      sqlLength: opts.sql?.length || 0,
      sqlPreview: opts.sql?.substring(0, 100),
    });
    setOptions(opts);
    setIsOpen(true);
  }, []);

  const closeSqlPreview = useCallback(() => {
    setIsOpen(false);
    // Delay clearing options so the close animation can finish
    setTimeout(() => setOptions(null), 300);
  }, []);

  return (
    <SqlPreviewContext.Provider value={{ openSqlPreview, closeSqlPreview, isOpen }}>
      {children}
      <SqlPreviewDialog
        open={isOpen}
        options={options}
        onClose={closeSqlPreview}
      />
    </SqlPreviewContext.Provider>
  );
}

// ─── Hook ────────────────────────────────────────────────────────────────────

/**
 * Hook to access the global SQL preview dialog.
 * Must be used within a <SqlPreviewProvider>.
 */
export function useSqlPreview(): SqlPreviewContextValue {
  const ctx = useContext(SqlPreviewContext);
  if (!ctx) {
    throw new Error('useSqlPreview must be used within a <SqlPreviewProvider>');
  }
  return ctx;
}
