'use client';

import { cn, parseTimestamp } from '@/lib/utils';
import { Terminal, Plus, Wifi, WifiOff, Trash2 } from 'lucide-react';
import { formatDistanceToNow } from 'date-fns';
import type { Conversation, ConnectionStatus } from '@/types';

interface TerminalSidebarProps {
  conversations: Conversation[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  onNewConversation: () => void;
  onDeleteConversation?: (id: string) => void;
  loading: boolean;
  connectionStatus: ConnectionStatus;
}

export function TerminalSidebar({
  conversations,
  selectedId,
  onSelect,
  onNewConversation,
  onDeleteConversation,
  loading,
  connectionStatus,
}: TerminalSidebarProps) {
  return (
    <aside className="w-64 bg-[#0d0d14] flex flex-col">
      {/* Header */}
      <div className="p-3 border-b border-emerald-900/30">
        <div className="flex items-center gap-2 mb-3">
          <Terminal className="h-4 w-4 text-emerald-400" />
          <span className="text-xs text-emerald-400 tracking-wider uppercase">KalamDB Terminal</span>
        </div>
        <div className="flex items-center gap-1.5 text-[10px]">
          {connectionStatus === 'connected' ? (
            <>
              <Wifi className="h-3 w-3 text-emerald-500" />
              <span className="text-emerald-500">ONLINE</span>
            </>
          ) : (
            <>
              <WifiOff className="h-3 w-3 text-red-500" />
              <span className="text-red-500">OFFLINE</span>
            </>
          )}
          <span className="text-emerald-900 ml-auto">v0.1.0</span>
        </div>
      </div>

      {/* New session button */}
      <button
        onClick={onNewConversation}
        className="mx-3 mt-3 px-3 py-2 border border-dashed border-emerald-800/50 rounded text-xs text-emerald-500 hover:border-emerald-400 hover:text-emerald-400 hover:bg-emerald-500/5 transition-all flex items-center gap-2"
      >
        <Plus className="h-3 w-3" />
        NEW SESSION
      </button>

      {/* Sessions list */}
      <div className="flex-1 overflow-y-auto mt-3 px-2 space-y-0.5">
        {loading ? (
          <div className="text-center py-6">
            <span className="text-xs text-emerald-800 animate-pulse">Loading sessions...</span>
          </div>
        ) : conversations.length === 0 ? (
          <div className="text-center py-6">
            <span className="text-xs text-emerald-900">No active sessions</span>
          </div>
        ) : (
          conversations.map((conv) => (
            <div
              key={conv.id}
              className={cn(
                'group relative w-full text-left px-3 py-2 rounded text-xs transition-all',
                selectedId === conv.id
                  ? 'bg-emerald-500/10 text-emerald-300 border-l-2 border-emerald-400'
                  : 'text-emerald-700 hover:text-emerald-400 hover:bg-emerald-500/5'
              )}
            >
              <button
                onClick={() => onSelect(conv.id)}
                className="w-full text-left"
              >
                <div className="flex items-center gap-2">
                  <span className={cn(
                    'w-1.5 h-1.5 rounded-full',
                    selectedId === conv.id ? 'bg-emerald-400 shadow-sm shadow-emerald-400/50' : 'bg-emerald-900'
                  )} />
                  <span className="truncate">{conv.title}</span>
                </div>
                <p className="text-[10px] text-emerald-900 mt-0.5 ml-3.5">
                  {(() => {
                    const updatedAt = parseTimestamp(conv.updated_at);
                    return updatedAt
                      ? formatDistanceToNow(updatedAt, { addSuffix: true })
                      : 'just now';
                  })()}
                </p>
              </button>
              {onDeleteConversation && (
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    if (confirm('Terminate session and delete all messages?')) {
                      onDeleteConversation(conv.id);
                    }
                  }}
                  className="absolute right-2 top-1/2 -translate-y-1/2 opacity-0 group-hover:opacity-100 p-1 hover:bg-red-500/10 rounded text-red-500 transition-opacity"
                  title="Terminate session"
                >
                  <Trash2 className="h-3 w-3" />
                </button>
              )}
            </div>
          ))
        )}
      </div>

      {/* Footer */}
      <div className="p-3 border-t border-emerald-900/30">
        <p className="text-[10px] text-emerald-900 flex items-center gap-1">
          <span className="w-1.5 h-1.5 rounded-full bg-emerald-500 shadow-sm shadow-emerald-500/50 animate-pulse" />
          kalamdb://localhost:8080
        </p>
      </div>
    </aside>
  );
}
