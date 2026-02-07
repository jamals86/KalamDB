'use client';

import { useState } from 'react';
import { TerminalSidebar } from './terminal-sidebar';
import { TerminalChat } from './terminal-chat';
import { TerminalWelcome } from './terminal-welcome';
import { useConversations, useConnectionStatus } from '@/hooks/use-kalamdb';

export default function Design3Page() {
  const [selectedConversationId, setSelectedConversationId] = useState<string | null>(null);
  const { conversations, loading: convLoading, createConversation, deleteConversation, refetch } = useConversations();
  const connectionStatus = useConnectionStatus();

  const selectedConversation = conversations.find(c => c.id === selectedConversationId) || null;

  const handleNewConversation = async () => {
    const conv = await createConversation('Session ' + new Date().toISOString().split('T')[0]);
    if (conv) {
      setSelectedConversationId(conv.id);
    }
  };

  const handleDeleteConversation = async (id: string) => {
    const success = await deleteConversation(id);
    if (success && selectedConversationId === id) {
      setSelectedConversationId(null);
    }
  };

  return (
    <div className="dark">
      <div className="flex h-screen bg-[#0a0a0f] text-emerald-50 font-mono">
        <TerminalSidebar
          conversations={conversations}
          selectedId={selectedConversationId}
          onSelect={setSelectedConversationId}
          onNewConversation={handleNewConversation}
          onDeleteConversation={handleDeleteConversation}
          loading={convLoading}
          connectionStatus={connectionStatus}
        />
        <main className="flex-1 flex flex-col border-l border-emerald-900/30">
          {selectedConversation ? (
            <TerminalChat
              conversation={selectedConversation}
              onRefreshConversations={refetch}
            />
          ) : (
            <TerminalWelcome onNewConversation={handleNewConversation} />
          )}
        </main>
      </div>
    </div>
  );
}
