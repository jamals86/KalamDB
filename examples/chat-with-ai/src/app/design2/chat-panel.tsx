'use client';

import { useEffect, useRef, useState } from 'react';
import { MessageInput, TypingDots, FileDisplay } from '@/components/chat';
import { Skeleton } from '@/components/ui/skeleton';
import { useMessages, useTypingIndicator } from '@/hooks/use-kalamdb';
import { cn, parseTimestamp } from '@/lib/utils';
import { format } from 'date-fns';
import { Sparkles, User, Trash2, StopCircle } from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';
import type { Conversation, Message } from '@/types';

interface ChatPanelProps {
  conversation: Conversation;
  onRefreshConversations: () => void;
}

export function ChatPanel({ conversation, onRefreshConversations }: ChatPanelProps) {
  const { messages, loading, sending, uploadProgress, waitingForAI, sendMessage, deleteMessage, stopResponse } = useMessages(conversation.id);
  const { typingUsers, setTyping, aiStatus } = useTypingIndicator(conversation.id);
  const scrollRef = useRef<HTMLDivElement>(null);
  const [newMessageIds, setNewMessageIds] = useState<Set<string>>(new Set());
  const prevCountRef = useRef(0);

  useEffect(() => {
    if (messages.length > prevCountRef.current) {
      const assistantNew = messages
        .slice(prevCountRef.current)
        .filter(m => m.role === 'assistant')
        .map(m => m.id);
      if (assistantNew.length > 0) {
        setNewMessageIds(prev => new Set([...prev, ...assistantNew]));
        setTimeout(() => {
          setNewMessageIds(prev => {
            const next = new Set(prev);
            assistantNew.forEach(id => next.delete(id));
            return next;
          });
        }, 5000);
      }
    }
    prevCountRef.current = messages.length;
  }, [messages]);

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight, behavior: 'smooth' });
  }, [messages, typingUsers]);

  const handleSend = async (content: string, files?: File[]) => {
    await sendMessage(content, files);
    onRefreshConversations();
  };

  const aiTyping = typingUsers.some(u => u.includes('ai') || u.includes('assistant')) || waitingForAI || !!aiStatus?.isTyping;

  return (
    <div className="flex flex-col h-[calc(100vh-3.5rem)]">
      {/* Messages */}
      <div ref={scrollRef} className="flex-1 overflow-y-auto scroll-smooth">
        <div className="max-w-2xl mx-auto px-4 py-8 space-y-4">
          {loading ? (
            <LoadingSkeleton />
          ) : messages.length === 0 ? (
            <motion.div 
              initial={{ opacity: 0, scale: 0.9 }}
              animate={{ opacity: 1, scale: 1 }}
              className="text-center py-20"
            >
              <div className="w-16 h-16 rounded-full bg-gradient-to-br from-violet-400 to-rose-400 flex items-center justify-center mx-auto mb-4 shadow-lg shadow-violet-400/20">
                <Sparkles className="h-8 w-8 text-white" />
              </div>
              <p className="text-muted-foreground">
                Send a message to begin
              </p>
            </motion.div>
          ) : (
            <AnimatePresence initial={false}>
              {messages.map((msg) => (
                <MinimalMessage
                  key={msg.id}
                  message={msg}
                  isNew={newMessageIds.has(msg.id)}
                  onDelete={() => deleteMessage?.(msg.id)}
                />
              ))}
            </AnimatePresence>
          )}

          <AnimatePresence>
            {aiTyping && (
              <motion.div 
                initial={{ opacity: 0, y: 10 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, transition: { duration: 0.2 } }}
                className="flex items-center gap-3"
              >
                <div className="w-7 h-7 rounded-full bg-gradient-to-br from-violet-400 to-rose-400 flex items-center justify-center animate-pulse-slow shadow-lg shadow-violet-400/30">
                  <Sparkles className="h-3.5 w-3.5 text-white" />
                </div>
                <div className="bg-muted/60 backdrop-blur-sm rounded-2xl px-4 py-2.5 border border-border/50 shadow-sm">
                  <TypingDots statusText={aiStatus?.label} showThinkingText={!aiStatus} />
                </div>
              </motion.div>
            )}
          </AnimatePresence>
        </div>
      </div>

      {/* Input */}
      <div className="border-t bg-background/80 backdrop-blur-md px-4 py-3 sticky bottom-0">
        <div className="max-w-2xl mx-auto relative">
          {waitingForAI && stopResponse && (
            <motion.button
              initial={{ opacity: 0, y: 10 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: 10 }}
              onClick={stopResponse}
              className="absolute -top-12 left-1/2 -translate-x-1/2 flex items-center gap-2 bg-background/90 backdrop-blur-sm border shadow-sm rounded-full px-4 py-1.5 text-xs font-medium text-muted-foreground hover:text-foreground transition-colors"
            >
              <StopCircle className="h-3.5 w-3.5" />
              Stop generating
            </motion.button>
          )}
          <MessageInput
            onSend={handleSend}
            onTypingChange={setTyping}
            sending={sending}
            uploadProgress={uploadProgress}
            placeholder="Write something..."
          />
        </div>
      </div>
    </div>
  );
}

function MinimalMessage({ message, isNew, onDelete }: { message: Message; isNew: boolean; onDelete?: () => void }) {
  const isUser = message.role === 'user';

  return (
    <motion.div 
      layout
      initial={{ opacity: 0, y: 20, scale: 0.95 }}
      animate={{ opacity: 1, y: 0, scale: 1 }}
      exit={{ opacity: 0, scale: 0.95, transition: { duration: 0.2 } }}
      transition={{ type: "spring", stiffness: 400, damping: 30 }}
      className={cn('flex items-start gap-3 group', isUser && 'flex-row-reverse')}
    >
      <div className={cn(
        'w-7 h-7 rounded-full flex items-center justify-center shrink-0 shadow-sm',
        isUser
          ? 'bg-foreground text-background'
          : 'bg-gradient-to-br from-violet-400 to-rose-400 text-white'
      )}>
        {isUser ? <User className="h-3.5 w-3.5" /> : <Sparkles className="h-3.5 w-3.5" />}
      </div>

      <div className={cn("flex flex-col gap-1 max-w-[75%]", isUser && "items-end")}>
        <div className={cn(
          'rounded-2xl px-4 py-2.5 border border-border/30 shadow-sm relative',
          isUser
            ? 'bg-foreground text-background rounded-tr-sm'
            : 'bg-muted/40 backdrop-blur-sm rounded-tl-sm'
        )}>
          <div className="text-sm leading-relaxed whitespace-pre-wrap break-words">
            {message.content}
            {message.files && <FileDisplay files={message.files} className="mt-2" />}
          </div>
          
          {onDelete && (
            <button
              onClick={onDelete}
              className={cn(
                "absolute top-2 opacity-0 group-hover:opacity-100 transition-opacity p-1.5 rounded-md hover:bg-black/10",
                isUser ? "-left-10 text-muted-foreground" : "-right-10 text-muted-foreground"
              )}
              title="Delete message"
            >
              <Trash2 className="h-4 w-4" />
            </button>
          )}
        </div>
        <p className={cn(
          'text-[10px] px-1',
          isUser ? 'text-muted-foreground' : 'text-muted-foreground/50'
        )}>
          {(() => {
            const createdAt = parseTimestamp(message.created_at);
            return createdAt ? format(createdAt, 'h:mm a') : '...';
          })()}
          {message.status === 'sending' && ' · Sending'}
        </p>
      </div>
    </motion.div>
  );
}

function LoadingSkeleton() {
  return (
    <div className="space-y-4 py-8">
      {[1, 2, 3].map(i => (
        <div key={i} className="flex items-start gap-3">
          <Skeleton className="h-7 w-7 rounded-full" />
          <Skeleton className="h-16 w-48 rounded-2xl" />
        </div>
      ))}
    </div>
  );
}
