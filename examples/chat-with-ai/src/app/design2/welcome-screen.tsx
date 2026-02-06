'use client';

import { Button } from '@/components/ui/button';
import { Sparkles, ArrowRight } from 'lucide-react';

interface WelcomeScreenProps {
  onNewConversation: () => void;
}

export function WelcomeScreen({ onNewConversation }: WelcomeScreenProps) {
  return (
    <div className="flex items-center justify-center min-h-[calc(100vh-3.5rem)] px-4">
      <div className="text-center max-w-lg space-y-8">
        <div className="relative">
          <div className="w-24 h-24 rounded-3xl bg-gradient-to-br from-violet-400 to-rose-400 flex items-center justify-center mx-auto shadow-xl shadow-violet-500/20">
            <Sparkles className="h-12 w-12 text-white" />
          </div>
          <div className="absolute -bottom-2 left-1/2 -translate-x-1/2 w-16 h-3 bg-violet-500/10 rounded-full blur-lg" />
        </div>

        <div className="space-y-3">
          <h2 className="text-3xl font-bold tracking-tight">
            <span className="bg-gradient-to-r from-violet-600 to-rose-500 bg-clip-text text-transparent">
              Hello there
            </span>
          </h2>
          <p className="text-muted-foreground text-lg leading-relaxed">
            Experience real-time messaging with streaming responses.
            Every message flows through KalamDB topics.
          </p>
        </div>

        <Button
          onClick={onNewConversation}
          size="lg"
          className="gap-2 bg-gradient-to-r from-violet-500 to-rose-500 hover:from-violet-600 hover:to-rose-600 text-white shadow-lg shadow-violet-500/25"
        >
          Start a conversation
          <ArrowRight className="h-4 w-4" />
        </Button>

        <div className="grid grid-cols-3 gap-4 text-center pt-4">
          {[
            { label: 'Real-time', desc: 'Live updates' },
            { label: 'Streaming', desc: 'Token animation' },
            { label: 'Topics', desc: 'CDC pipeline' },
          ].map((item) => (
            <div key={item.label} className="space-y-1">
              <p className="text-sm font-medium">{item.label}</p>
              <p className="text-xs text-muted-foreground">{item.desc}</p>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
