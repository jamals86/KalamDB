'use client';

import { Button } from '@/components/ui/button';
import { MessageSquare, ArrowRight } from 'lucide-react';

interface EmptyStateProps {
  onNewConversation: () => void;
}

export function EmptyState({ onNewConversation }: EmptyStateProps) {
  return (
    <div className="flex-1 flex items-center justify-center p-8">
      <div className="text-center max-w-md space-y-6">
        <div className="w-20 h-20 rounded-2xl bg-primary/10 flex items-center justify-center mx-auto">
          <MessageSquare className="h-10 w-10 text-primary" />
        </div>
        <div className="space-y-2">
          <h2 className="text-2xl font-semibold">Welcome to KalamDB Chat</h2>
          <p className="text-muted-foreground">
            Start a conversation and experience real-time messaging powered by
            KalamDB topics and live queries.
          </p>
        </div>
        <div className="space-y-3">
          <Button onClick={onNewConversation} size="lg" className="gap-2">
            Start chatting
            <ArrowRight className="h-4 w-4" />
          </Button>
          <div className="flex items-center gap-4 justify-center text-xs text-muted-foreground">
            <span>Real-time sync</span>
            <span>·</span>
            <span>Topic-based events</span>
            <span>·</span>
            <span>Streaming responses</span>
          </div>
        </div>
      </div>
    </div>
  );
}
