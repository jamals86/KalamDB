'use client';

import { useState } from 'react';
import { ConversationDrawer } from './conversation-drawer';
import { ChatPanel } from './chat-panel';
import { WelcomeScreen } from './welcome-screen';
import { UserMenu } from '@/components/user-menu';
import { useConversations, useConnectionStatus } from '@/hooks/use-kalamdb';

export default function Design2Page() {
  const [selectedConversationId, setSelectedConversationId] = useState<string | null>(null);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const { conversations, loading: convLoading, createConversation, deleteConversation, refetch } = useConversations();
  const connectionStatus = useConnectionStatus();

  const selectedConversation = conversations.find(c => c.id === selectedConversationId) || null;

  const handleNewConversation = async () => {
    const conv = await createConversation('New conversation');
    if (conv) {
      setSelectedConversationId(conv.id);
      setDrawerOpen(false);
    }
  };

  const handleSelect = (id: string) => {
    setSelectedConversationId(id);
    setDrawerOpen(false);
  };

  const handleDeleteConversation = async (id: string) => {
    const success = await deleteConversation(id);
    if (success && selectedConversationId === id) {
      setSelectedConversationId(null);
    }
  };

  return (
    <div className="min-h-screen bg-gradient-to-br from-violet-50 via-background to-rose-50/30">
      {/* Top bar */}
      <header className="fixed top-0 left-0 right-0 z-50 backdrop-blur-lg bg-background/70 border-b">
        <div className="max-w-4xl mx-auto px-4 h-14 flex items-center justify-between">
          <button
            onClick={() => setDrawerOpen(!drawerOpen)}
            className="text-sm font-medium hover:text-primary transition-colors flex items-center gap-2"
          >
            <svg className="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6h16M4 12h16M4 18h16" />
            </svg>
            Conversations
          </button>
          <h1 className="text-sm font-semibold bg-gradient-to-r from-violet-600 to-rose-500 bg-clip-text text-transparent">
            KalamDB Chat
          </h1>
          <div className="flex items-center gap-3">
            <UserMenu />
            <div className="flex items-center gap-2">
              <div className={`w-2 h-2 rounded-full ${
                connectionStatus === 'connected' ? 'bg-green-500' : 'bg-red-500'
              }`} />
              <span className="text-xs text-muted-foreground capitalize">{connectionStatus}</span>
            </div>
          </div>
        </div>
      </header>

      {/* Drawer overlay */}
      {drawerOpen && (
        <ConversationDrawer
          conversations={conversations}
          selectedId={selectedConversationId}
          onSelect={handleSelect}
          onNewConversation={handleNewConversation}
          onDeleteConversation={handleDeleteConversation}
          onClose={() => setDrawerOpen(false)}
          loading={convLoading}
        />
      )}

      {/* Main content */}
      <main className="pt-14 min-h-screen">
        {selectedConversation ? (
          <ChatPanel
            conversation={selectedConversation}
            onRefreshConversations={refetch}
          />
        ) : (
          <WelcomeScreen onNewConversation={handleNewConversation} />
        )}
      </main>
    </div>
  );
}
