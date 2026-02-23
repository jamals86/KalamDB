'use client';

import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Separator } from '@/components/ui/separator';
import { ConnectionBadge } from '@/components/chat';
import { UserMenu } from '@/components/user-menu';
import { Plus, MessageSquare, Loader2, Trash2 } from 'lucide-react';
import { cn, parseTimestamp } from '@/lib/utils';
import { formatDistanceToNow } from 'date-fns';
import { motion, AnimatePresence } from 'framer-motion';
import type { Conversation, ConnectionStatus } from '@/types';

interface SidebarProps {
  conversations: Conversation[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  onNewConversation: () => void;
  onDeleteConversation?: (id: string) => void;
  loading: boolean;
  connectionStatus: ConnectionStatus;
}

export function Sidebar({
  conversations,
  selectedId,
  onSelect,
  onDeleteConversation,
  onNewConversation,
  loading,
  connectionStatus,
}: SidebarProps) {
  return (
    <aside className="w-72 border-r bg-muted/30 flex flex-col">
      {/* Header */}
      <div className="p-4 space-y-3">
        <div className="flex items-center justify-between">
          <h2 className="font-semibold text-lg">Chats</h2>
          <ConnectionBadge status={connectionStatus} />
        </div>
        <Button
          onClick={onNewConversation}
          className="w-full gap-2"
          variant="outline"
        >
          <Plus className="h-4 w-4" />
          New Chat
        </Button>
      </div>

      <Separator />

      {/* Conversation List */}
      <ScrollArea className="flex-1">
        <div className="p-2 space-y-1">
          {loading ? (
            <div className="flex items-center justify-center py-8">
              <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" />
            </div>
          ) : conversations.length === 0 ? (
            <p className="text-sm text-muted-foreground text-center py-8 px-4">
              No conversations yet. Start a new chat!
            </p>
          ) : (
            <AnimatePresence initial={false}>
              {conversations.map((conv) => (
                <motion.div
                  key={conv.id}
                  layout
                  initial={{ opacity: 0, x: -20 }}
                  animate={{ opacity: 1, x: 0 }}
                  exit={{ opacity: 0, x: -20, transition: { duration: 0.2 } }}
                  className={cn(
                    'group relative w-full flex items-start gap-3 rounded-lg px-3 py-2.5 text-left text-sm transition-colors hover:bg-accent',
                    selectedId === conv.id && 'bg-accent'
                  )}
                >
                  <button
                    onClick={() => onSelect(conv.id)}
                    className="flex items-start gap-3 flex-1 min-w-0"
                  >
                    <MessageSquare className="h-4 w-4 mt-0.5 shrink-0 text-muted-foreground" />
                    <div className="min-w-0 flex-1">
                      <p className="font-medium truncate">{conv.title}</p>
                      <p className="text-xs text-muted-foreground">
                        {(() => {
                          const updatedAt = parseTimestamp(conv.updated_at);
                          return updatedAt
                            ? formatDistanceToNow(updatedAt, { addSuffix: true })
                            : 'just now';
                        })()}
                      </p>
                    </div>
                  </button>
                  {onDeleteConversation && (
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        if (confirm('Delete this conversation and all its messages?')) {
                          onDeleteConversation(conv.id);
                        }
                      }}
                      className="opacity-0 group-hover:opacity-100 p-1 hover:bg-destructive/10 rounded text-destructive transition-opacity"
                      title="Delete conversation"
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </button>
                  )}
                </motion.div>
              ))}
            </AnimatePresence>
          )}
        </div>
      </ScrollArea>

      {/* Footer */}
      <Separator />
      <div className="p-3 space-y-2">
        <UserMenu />
        <p className="text-xs text-muted-foreground text-center">
          Powered by KalamDB
        </p>
      </div>
    </aside>
  );
}
