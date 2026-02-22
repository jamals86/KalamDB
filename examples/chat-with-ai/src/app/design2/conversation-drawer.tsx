'use client';

import { Button } from '@/components/ui/button';
import { Plus, X, MessageCircle, Trash2 } from 'lucide-react';
import { formatDistanceToNow } from 'date-fns';
import { cn, parseTimestamp } from '@/lib/utils';
import { motion, AnimatePresence } from 'framer-motion';
import type { Conversation } from '@/types';

interface ConversationDrawerProps {
  conversations: Conversation[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  onNewConversation: () => void;
  onDeleteConversation?: (id: string) => void;
  onClose: () => void;
  loading: boolean;
}

export function ConversationDrawer({
  conversations,
  selectedId,
  onSelect,
  onNewConversation,
  onDeleteConversation,
  onClose,
  loading,
}: ConversationDrawerProps) {
  return (
    <>
      {/* Backdrop */}
      <div
        className="fixed inset-0 z-40 bg-black/20 backdrop-blur-sm"
        onClick={onClose}
      />

      {/* Drawer */}
      <div className="fixed top-14 left-0 bottom-0 z-50 w-80 bg-background/95 backdrop-blur-xl border-r shadow-xl animate-slide-in">
        <div className="p-4 flex items-center justify-between">
          <h2 className="font-semibold">Conversations</h2>
          <Button variant="ghost" size="icon" onClick={onClose}>
            <X className="h-4 w-4" />
          </Button>
        </div>

        <div className="px-4 mb-4">
          <Button
            onClick={onNewConversation}
            variant="outline"
            className="w-full gap-2 border-dashed"
          >
            <Plus className="h-4 w-4" />
            New Conversation
          </Button>
        </div>

        <div className="overflow-y-auto px-2 space-y-1" style={{ maxHeight: 'calc(100vh - 180px)' }}>
          {loading ? (
            <div className="flex items-center justify-center py-8 text-muted-foreground text-sm">
              Loading...
            </div>
          ) : conversations.length === 0 ? (
            <div className="text-center py-8 text-muted-foreground text-sm">
              No conversations yet
            </div>
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
                    'group relative w-full text-left rounded-xl transition-all hover:bg-violet-50',
                    selectedId === conv.id && 'bg-violet-100 shadow-sm'
                  )}
                >
                  <button
                    onClick={() => onSelect(conv.id)}
                    className="w-full px-4 py-3"
                  >
                    <div className="flex items-start gap-3">
                      <div className={cn(
                        'w-8 h-8 rounded-full flex items-center justify-center shrink-0 mt-0.5',
                        selectedId === conv.id
                          ? 'bg-violet-500 text-white'
                          : 'bg-violet-100 text-violet-600'
                      )}>
                        <MessageCircle className="h-4 w-4" />
                      </div>
                      <div className="min-w-0 flex-1">
                        <p className="text-sm font-medium truncate">{conv.title}</p>
                        <p className="text-xs text-muted-foreground mt-0.5">
                          {(() => {
                            const updatedAt = parseTimestamp(conv.updated_at);
                            return updatedAt
                              ? formatDistanceToNow(updatedAt, { addSuffix: true })
                              : 'just now';
                          })()}
                        </p>
                      </div>
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
                      className="absolute right-2 top-1/2 -translate-y-1/2 opacity-0 group-hover:opacity-100 p-1.5 hover:bg-destructive/10 rounded text-destructive transition-opacity"
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
      </div>
    </>
  );
}
