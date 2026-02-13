'use client';

import { useEffect, useRef, useState, useCallback } from 'react';
import { useMessages, useTypingIndicator } from '@/hooks/use-kalamdb';
import { MessageInput, FileDisplay } from '@/components/chat';
import { cn, parseTimestamp } from '@/lib/utils';
import { format } from 'date-fns';
import type { Conversation, Message } from '@/types';

interface TerminalChatProps {
  conversation: Conversation;
  onRefreshConversations: () => void;
}

export function TerminalChat({ conversation, onRefreshConversations }: TerminalChatProps) {
  const { messages, loading, sending, uploadProgress, waitingForAI, sendMessage } = useMessages(conversation.id);
  const { typingUsers, setTyping, aiStatus } = useTypingIndicator(conversation.id);
  const scrollRef = useRef<HTMLDivElement>(null);
  const [newIds, setNewIds] = useState<Set<string>>(new Set());
  const prevCount = useRef(0);

  useEffect(() => {
    if (messages.length > prevCount.current) {
      const assistantNew = messages
        .slice(prevCount.current)
        .filter(m => m.role === 'assistant')
        .map(m => m.id);
      if (assistantNew.length > 0) {
        setNewIds(prev => new Set([...prev, ...assistantNew]));
        setTimeout(() => {
          setNewIds(prev => {
            const next = new Set(prev);
            assistantNew.forEach(id => next.delete(id));
            return next;
          });
        }, 6000);
      }
    }
    prevCount.current = messages.length;
  }, [messages]);

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight, behavior: 'smooth' });
  }, [messages, typingUsers]);

  const handleSend = useCallback(async (content: string, files?: File[]) => {
    await sendMessage(content, files);
    onRefreshConversations();
  }, [sendMessage, onRefreshConversations]);

  const aiTyping = typingUsers.some(u => u.includes('ai') || u.includes('assistant')) || waitingForAI || !!aiStatus?.isTyping;

  return (
    <div className="flex-1 flex flex-col">
      {/* Header */}
      <div className="px-4 py-2 border-b border-emerald-900/30 flex items-center justify-between bg-[#0d0d14]">
        <div className="flex items-center gap-2">
          <span className="text-emerald-600 text-xs">$</span>
          <span className="text-xs text-emerald-300">{conversation.title}</span>
        </div>
        <span className="text-[10px] text-emerald-800">
          {messages.length} msg{messages.length !== 1 ? 's' : ''}
        </span>
      </div>

      {/* Messages */}
      <div ref={scrollRef} className="flex-1 overflow-y-auto bg-[#0a0a0f]">
        <div className="p-4 space-y-3">
          {/* System intro */}
          <div className="text-[10px] text-emerald-800 border-b border-emerald-900/20 pb-2 mb-4">
            <span className="text-emerald-600">[system]</span> Connected to chat session. Type a message below.
          </div>

          {loading ? (
            <div className="space-y-2">
              {[1, 2, 3].map(i => (
                <div key={i} className="h-4 bg-emerald-900/10 rounded w-48 animate-pulse" />
              ))}
            </div>
          ) : (
            messages.map((msg) => (
              <TerminalMessage
                key={msg.id}
                message={msg}
                isNew={newIds.has(msg.id)}
              />
            ))
          )}

          {aiTyping && (
            <div className="flex items-center gap-2 animate-fade-in text-xs">
              <span className="text-cyan-500">[ai]</span>
              <span className="text-cyan-400/90 font-medium animate-pulse-text">
                {aiStatus?.label ?? 'thinking'}
              </span>
              <span className="text-cyan-400/70 flex gap-0.5">
                <span className="animate-typing-dot" style={{ animationDelay: '0ms' }}>.</span>
                <span className="animate-typing-dot" style={{ animationDelay: '200ms' }}>.</span>
                <span className="animate-typing-dot" style={{ animationDelay: '400ms' }}>.</span>
              </span>
              <span className="text-cyan-400/70 inline-block animate-pulse">█</span>
            </div>
          )}
        </div>
      </div>

      {/* Terminal-style input */}
      <div className="border-t border-emerald-900/30 bg-[#0d0d14] px-4 py-3">
        <div className="space-y-2">
          <div className="flex items-center gap-2 text-xs text-emerald-600">
            <span>{sending ? '...' : '>'}</span>
            <span className="text-emerald-800">Message input (supports files & emojis)</span>
          </div>
          <MessageInput
            onSend={handleSend}
            onTypingChange={setTyping}
            sending={sending}
            uploadProgress={uploadProgress}
            placeholder="Type your message..."
            className="[&_textarea]:bg-[#0a0a0f] [&_textarea]:text-emerald-100 [&_textarea]:border-emerald-900/30 [&_textarea]:font-mono [&_textarea]:text-sm [&_button]:bg-emerald-500/10 [&_button]:text-emerald-400 [&_button]:border-emerald-900/30 [&_button:hover]:bg-emerald-500/20"
          />
        </div>
      </div>
    </div>
  );
}

function TerminalMessage({ message }: { message: Message; isNew: boolean }) {
  const isUser = message.role === 'user';

  const createdAt = parseTimestamp(message.created_at);

  return (
    <div className="animate-fade-in group">
      <div className="flex items-start gap-2 text-xs">
        <span className={cn(
          'shrink-0 mt-px',
          isUser ? 'text-amber-500' : 'text-cyan-500'
        )}>
          [{isUser ? 'you' : 'ai'}]
        </span>
        <div className="flex-1 min-w-0">
          <span className={cn(
            'leading-relaxed whitespace-pre-wrap',
            isUser ? 'text-emerald-200' : 'text-cyan-200'
          )}>
            {message.content}
          </span>
          {message.files && message.files.length > 0 && (
            <div className="mt-2 p-2 border border-emerald-900/30 rounded bg-[#0d0d14]">
              <FileDisplay files={message.files} />
            </div>
          )}
        </div>
        <span className="text-emerald-900 text-[10px] shrink-0 opacity-0 group-hover:opacity-100 transition-opacity">
          {createdAt ? format(createdAt, 'HH:mm:ss') : '--:--:--'}
        </span>
      </div>
      {message.status === 'sending' && (
        <span className="text-[10px] text-amber-800 ml-6">sending...</span>
      )}
      {message.status === 'error' && (
        <span className="text-[10px] text-red-500 ml-6">error: message delivery failed</span>
      )}
    </div>
  );
}
