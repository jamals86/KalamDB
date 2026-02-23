'use client';

import { useEffect, useRef, useState } from 'react';
import { ScrollArea } from '@/components/ui/scroll-area';
import { MessageInput, TypingDots, FileDisplay } from '@/components/chat';
import { Avatar, AvatarFallback } from '@/components/ui/avatar';
import { Skeleton } from '@/components/ui/skeleton';
import { useMessages, useTypingIndicator } from '@/hooks/use-kalamdb';
import { cn, parseTimestamp } from '@/lib/utils';
import { formatDistanceToNow } from 'date-fns';
import { Bot, User, Trash2, StopCircle } from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';
import type { Conversation, Message } from '@/types';

interface ChatAreaProps {
  conversation: Conversation;
  onRefreshConversations: () => void;
}

export function ChatArea({ conversation, onRefreshConversations }: ChatAreaProps) {
  const { messages, loading, sending, uploadProgress, waitingForAI, sendMessage, deleteMessage, stopResponse } = useMessages(conversation.id);
  const { typingUsers, setTyping, aiStatus } = useTypingIndicator(conversation.id);
  const scrollRef = useRef<HTMLDivElement>(null);
  const [newMessageIds, setNewMessageIds] = useState<Set<string>>(new Set());
  const prevMessageCountRef = useRef(0);

  // Track new messages for streaming animation
  useEffect(() => {
    if (messages.length > prevMessageCountRef.current) {
      const newMessages = messages.slice(prevMessageCountRef.current);
      const assistantNewIds = newMessages
        .filter(m => m.role === 'assistant')
        .map(m => m.id);
      if (assistantNewIds.length > 0) {
        setNewMessageIds(prev => new Set([...prev, ...assistantNewIds]));
        // Clear after animation
        setTimeout(() => {
          setNewMessageIds(prev => {
            const next = new Set(prev);
            assistantNewIds.forEach(id => next.delete(id));
            return next;
          });
        }, 5000);
      }
    }
    prevMessageCountRef.current = messages.length;
  }, [messages]);

  // Auto-scroll to bottom
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [messages, typingUsers]);

  const handleSend = async (content: string, files?: File[]) => {
    await sendMessage(content, files);
    onRefreshConversations();
  };

  const aiTyping = typingUsers.some(u => u.includes('ai') || u.includes('assistant')) || waitingForAI || !!aiStatus?.isTyping;

  return (
    <div className="flex-1 flex flex-col">
      {/* Header */}
      <div className="border-b px-6 py-3 flex items-center gap-3">
        <div className="flex-1 min-w-0">
          <h1 className="font-semibold truncate">{conversation.title}</h1>
          <p className="text-xs text-muted-foreground">
            {messages.length} messages
          </p>
        </div>
      </div>

      {/* Messages */}
      <div ref={scrollRef} className="flex-1 overflow-y-auto scroll-smooth">
        <div className="max-w-3xl mx-auto px-4 py-6 space-y-6">
          {loading ? (
            <MessageSkeleton />
          ) : messages.length === 0 ? (
            <motion.div 
              initial={{ opacity: 0, y: 20 }}
              animate={{ opacity: 1, y: 0 }}
              className="text-center py-12"
            >
              <Bot className="h-12 w-12 mx-auto text-muted-foreground/50 mb-3" />
              <p className="text-muted-foreground">
                Send a message to start the conversation
              </p>
            </motion.div>
          ) : (
            <AnimatePresence initial={false}>
              {messages.map((message) => (
                <MessageBubble
                  key={message.id}
                  message={message}
                  isNew={newMessageIds.has(message.id)}
                  onDelete={() => deleteMessage?.(message.id)}
                />
              ))}
            </AnimatePresence>
          )}

          {/* Typing indicator */}
          <AnimatePresence>
            {aiTyping && (
              <motion.div 
                initial={{ opacity: 0, y: 10, scale: 0.95 }}
                animate={{ opacity: 1, y: 0, scale: 1 }}
                exit={{ opacity: 0, scale: 0.95, transition: { duration: 0.2 } }}
                className="flex items-start gap-3"
              >
                <Avatar className="h-8 w-8 ring-2 ring-primary/20 animate-pulse-slow">
                  <AvatarFallback className="bg-primary/10 text-primary">
                    <Bot className="h-4 w-4" />
                  </AvatarFallback>
                </Avatar>
                <div className="bg-muted rounded-2xl rounded-tl-sm px-4 py-3 shadow-sm">
                  <TypingDots statusText={aiStatus?.label} showThinkingText={!aiStatus} />
                </div>
              </motion.div>
            )}
          </AnimatePresence>
        </div>
      </div>

      {/* Input */}
      <div className="border-t px-4 py-3 bg-background/80 backdrop-blur-sm sticky bottom-0">
        <div className="max-w-3xl mx-auto relative">
          {waitingForAI && stopResponse && (
            <motion.button
              initial={{ opacity: 0, y: 10 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: 10 }}
              onClick={stopResponse}
              className="absolute -top-12 left-1/2 -translate-x-1/2 flex items-center gap-2 bg-background border shadow-sm rounded-full px-4 py-1.5 text-xs font-medium text-muted-foreground hover:text-foreground transition-colors"
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
            placeholder="Type your message..."
          />
        </div>
      </div>
    </div>
  );
}

function MessageBubble({ message, isNew, onDelete }: { message: Message; isNew: boolean; onDelete?: () => void }) {
  const isUser = message.role === 'user';

  return (
    <motion.div
      layout
      initial={{ opacity: 0, y: 20, scale: 0.95 }}
      animate={{ opacity: 1, y: 0, scale: 1 }}
      exit={{ opacity: 0, scale: 0.95, transition: { duration: 0.2 } }}
      transition={{ type: "spring", stiffness: 400, damping: 30 }}
      className={cn(
        'flex items-start gap-3 group',
        isUser && 'flex-row-reverse'
      )}
    >
      <Avatar className="h-8 w-8 shrink-0 shadow-sm">
        <AvatarFallback
          className={cn(
            isUser
              ? 'bg-primary text-primary-foreground'
              : 'bg-primary/10 text-primary'
          )}
        >
          {isUser ? <User className="h-4 w-4" /> : <Bot className="h-4 w-4" />}
        </AvatarFallback>
      </Avatar>

      <div className={cn("flex flex-col gap-1 max-w-[80%]", isUser && "items-end")}>
        <div
          className={cn(
            'rounded-2xl px-4 py-2.5 shadow-sm relative',
            isUser
              ? 'bg-primary text-primary-foreground rounded-tr-sm'
              : 'bg-muted rounded-tl-sm'
          )}
        >
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
        <div
          className={cn(
            'text-[10px] px-1',
            isUser ? 'text-muted-foreground' : 'text-muted-foreground/60'
          )}
        >
          {(() => {
            const createdAt = parseTimestamp(message.created_at);
            return createdAt
              ? formatDistanceToNow(createdAt, { addSuffix: true })
              : 'just now';
          })()}
          {message.status === 'sending' && ' · Sending...'}
          {message.status === 'error' && ' · Failed'}
        </div>
      </div>
    </motion.div>
  );
}

function MessageSkeleton() {
  return (
    <div className="space-y-6">
      {[1, 2, 3].map((i) => (
        <div key={i} className="flex items-start gap-3">
          <Skeleton className="h-8 w-8 rounded-full" />
          <div className="space-y-2">
            <Skeleton className="h-4 w-48" />
            <Skeleton className="h-4 w-32" />
          </div>
        </div>
      ))}
    </div>
  );
}
