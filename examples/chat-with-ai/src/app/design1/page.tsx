'use client';

import { useState } from 'react';
import { Sidebar } from './sidebar';
import { ChatArea } from './chat-area';
import { EmptyState } from './empty-state';
import { useConversations, useConnectionStatus } from '@/hooks/use-kalamdb';

export default function Design1Page() {
  const [selectedConversationId, setSelectedConversationId] = useState<string | null>(null);
  const { conversations, loading: convLoading, createConversation, deleteConversation, refetch } = useConversations();
  const connectionStatus = useConnectionStatus();

  const selectedConversation = conversations.find(c => c.id === selectedConversationId) || null;

  const handleNewConversation = async () => {
    const conv = await createConversation('New conversation');
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
    <div className="flex h-screen bg-background">
      <Sidebar
        conversations={conversations}
        selectedId={selectedConversationId}
        onSelect={setSelectedConversationId}
        onNewConversation={handleNewConversation}
        onDeleteConversation={handleDeleteConversation}
        loading={convLoading}
        connectionStatus={connectionStatus}
      />
      <main className="flex-1 flex flex-col">
        {selectedConversation ? (
          <ChatArea
            conversation={selectedConversation}
            onRefreshConversations={refetch}
          />
        ) : (
          <EmptyState onNewConversation={handleNewConversation} />
        )}
      </main>
    </div>
  );
}
