'use client';

import { useEffect, useRef, useState } from 'react';
import { MessageInput, TypingDots, StreamingText, FileDisplay } from '@/components/chat';
import { Skeleton } from '@/components/ui/skeleton';
import { useMessages, useTypingIndicator } from '@/hooks/use-kalamdb';
import { cn, parseTimestamp } from '@/lib/utils';
import { format } from 'date-fns';
import { Sparkles, User } from 'lucide-react';
import type { Conversation, Message } from '@/types';

interface ChatPanelProps {
  conversation: Conversation;
  onRefreshConversations: () => void;
}

export function ChatPanel({ conversation, onRefreshConversations }: ChatPanelProps) {
  const { messages, loading, sending, uploadProgress, waitingForAI, sendMessage } = useMessages(conversation.id);
  const { typingUsers, setTyping } = useTypingIndicator(conversation.id);
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

  const aiTyping = typingUsers.some(u => u.includes('ai') || u.includes('assistant')) || waitingForAI;

  return (
    <div className="flex flex-col h-[calc(100vh-3.5rem)]">
      {/* Messages */}
      <div ref={scrollRef} className="flex-1 overflow-y-auto">
        <div className="max-w-2xl mx-auto px-4 py-8 space-y-4">
          {loading ? (
            <LoadingSkeleton />
          ) : messages.length === 0 ? (
            <div className="text-center py-20">
              <div className="w-16 h-16 rounded-full bg-gradient-to-br from-violet-400 to-rose-400 flex items-center justify-center mx-auto mb-4">
                <Sparkles className="h-8 w-8 text-white" />
              </div>
              <p className="text-muted-foreground">
                Send a message to begin
              </p>
            </div>
          ) : (
            messages.map((msg) => (
              <MinimalMessage
                key={msg.id}
                message={msg}
                isNew={newMessageIds.has(msg.id)}
              />
            ))
          )}

          {aiTyping && (
            <div className="flex items-center gap-3 animate-fade-in">
              <div className="w-7 h-7 rounded-full bg-gradient-to-br from-violet-400 to-rose-400 flex items-center justify-center animate-pulse-slow shadow-lg shadow-violet-400/30">
                <Sparkles className="h-3.5 w-3.5 text-white" />
              </div>
              <div className="bg-muted/60 backdrop-blur-sm rounded-2xl px-4 py-2.5 border border-border/50 shadow-sm">
                <TypingDots />
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Input */}
      <div className="border-t bg-background/50 backdrop-blur-sm px-4 py-3">
        <div className="max-w-2xl mx-auto">
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

function MinimalMessage({ message, isNew }: { message: Message; isNew: boolean }) {
  const isUser = message.role === 'user';

  return (
    <div className={cn('flex items-start gap-3 animate-fade-in', isUser && 'flex-row-reverse')}>
      <div className={cn(
        'w-7 h-7 rounded-full flex items-center justify-center shrink-0',
        isUser
          ? 'bg-foreground text-background'
          : 'bg-gradient-to-br from-violet-400 to-rose-400 text-white'
      )}>
        {isUser ? <User className="h-3.5 w-3.5" /> : <Sparkles className="h-3.5 w-3.5" />}
      </div>

      <div className={cn(
        'max-w-[75%] rounded-2xl px-4 py-2.5 border border-border/30',
        isUser
          ? 'bg-foreground text-background rounded-tr-sm'
          : 'bg-muted/40 backdrop-blur-sm rounded-tl-sm'
      )}>
        <p className="text-sm leading-relaxed whitespace-pre-wrap">
          {isUser ? (
            <>
              {message.content}
              {message.files && <FileDisplay files={message.files} className="mt-2" />}
            </>
          ) : (
            <>
              <StreamingText text={message.content} isNew={isNew} speed={22} />
              {message.files && <FileDisplay files={message.files} className="mt-2" />}
            </>
          )}
        </p>
        <p className={cn(
          'text-[10px] mt-1.5',
          isUser ? 'text-background/50' : 'text-muted-foreground/50'
        )}>
          {(() => {
            const createdAt = parseTimestamp(message.created_at);
            return createdAt ? format(createdAt, 'h:mm a') : '...';
          })()}
          {message.status === 'sending' && ' · Sending'}
        </p>
      </div>
    </div>
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
