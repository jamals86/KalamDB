'use client';

import { useEffect, useRef, useState, useCallback } from 'react';
import { useMessages, useTypingIndicator } from '@/hooks/use-kalamdb';
import { MessageInput, FileDisplay } from '@/components/chat';
import { cn, parseTimestamp } from '@/lib/utils';
import { format } from 'date-fns';
import { Trash2, StopCircle } from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';
import type { Conversation, Message } from '@/types';

interface TerminalChatProps {
  conversation: Conversation;
  onRefreshConversations: () => void;
}

export function TerminalChat({ conversation, onRefreshConversations }: TerminalChatProps) {
  const { messages, loading, sending, uploadProgress, waitingForAI, sendMessage, deleteMessage, stopResponse } = useMessages(conversation.id);
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
      <div ref={scrollRef} className="flex-1 overflow-y-auto bg-[#0a0a0f] scroll-smooth">
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
            <AnimatePresence initial={false}>
              {messages.map((msg) => (
                <TerminalMessage
                  key={msg.id}
                  message={msg}
                  isNew={newIds.has(msg.id)}
                  onDelete={() => deleteMessage?.(msg.id)}
                />
              ))}
            </AnimatePresence>
          )}

          <AnimatePresence>
            {aiTyping && (
              <motion.div 
                initial={{ opacity: 0, x: -10 }}
                animate={{ opacity: 1, x: 0 }}
                exit={{ opacity: 0, transition: { duration: 0.2 } }}
                className="flex items-center gap-2 text-xs"
              >
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
              </motion.div>
            )}
          </AnimatePresence>
        </div>
      </div>

      {/* Terminal-style input */}
      <div className="border-t border-emerald-900/30 bg-[#0d0d14] px-4 py-3 relative">
        {waitingForAI && stopResponse && (
          <motion.button
            initial={{ opacity: 0, y: 10 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: 10 }}
            onClick={stopResponse}
            className="absolute -top-10 right-4 flex items-center gap-2 bg-[#0a0a0f] border border-emerald-900/50 shadow-sm rounded px-3 py-1 text-[10px] font-mono text-emerald-600 hover:text-emerald-400 hover:border-emerald-500/50 transition-colors"
          >
            <StopCircle className="h-3 w-3" />
            ^C (SIGINT)
          </motion.button>
        )}
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

function TerminalMessage({ message, isNew, onDelete }: { message: Message; isNew: boolean; onDelete?: () => void }) {
  const isUser = message.role === 'user';

  const createdAt = parseTimestamp(message.created_at);

  return (
    <motion.div 
      layout
      initial={{ opacity: 0, x: -10 }}
      animate={{ opacity: 1, x: 0 }}
      exit={{ opacity: 0, x: -10, transition: { duration: 0.2 } }}
      className="group relative"
    >
      <div className="flex items-start gap-2 text-xs">
        <span className={cn(
          'shrink-0 mt-px',
          isUser ? 'text-amber-500' : 'text-cyan-500'
        )}>
          [{isUser ? 'you' : 'ai'}]
        </span>
        <div className="flex-1 min-w-0">
          <span className={cn(
            'leading-relaxed whitespace-pre-wrap break-words',
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
        <div className="flex items-center gap-2 shrink-0 opacity-0 group-hover:opacity-100 transition-opacity">
          {onDelete && (
            <button
              onClick={onDelete}
              className="text-emerald-900 hover:text-red-500 transition-colors"
              title="Delete message"
            >
              <Trash2 className="h-3.5 w-3.5" />
            </button>
          )}
          <span className="text-emerald-900 text-[10px]">
            {createdAt ? format(createdAt, 'HH:mm:ss') : '--:--:--'}
          </span>
        </div>
      </div>
      {message.status === 'sending' && (
        <span className="text-[10px] text-amber-800 ml-6">sending...</span>
      )}
      {message.status === 'error' && (
        <span className="text-[10px] text-red-500 ml-6">error: message delivery failed</span>
      )}
    </motion.div>
  );
}
