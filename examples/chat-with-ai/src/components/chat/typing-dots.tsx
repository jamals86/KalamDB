'use client';

import { cn } from '@/lib/utils';

interface TypingDotsProps {
  className?: string;
  label?: string;
  showThinkingText?: boolean;
}

export function TypingDots({ className, label, showThinkingText = true }: TypingDotsProps) {
  return (
    <div className={cn('flex items-center gap-2', className)}>
      {label && (
        <span className="text-xs text-muted-foreground mr-1">{label}</span>
      )}
      
      {showThinkingText && (
        <span className="text-sm font-medium text-muted-foreground animate-pulse-text mr-1">
          Thinking
        </span>
      )}
      
      <div className="flex items-center gap-1">
        <span
          className="w-1.5 h-1.5 rounded-full bg-muted-foreground/70 animate-typing-dot"
          style={{ animationDelay: '0ms' }}
        />
        <span
          className="w-1.5 h-1.5 rounded-full bg-muted-foreground/70 animate-typing-dot"
          style={{ animationDelay: '200ms' }}
        />
        <span
          className="w-1.5 h-1.5 rounded-full bg-muted-foreground/70 animate-typing-dot"
          style={{ animationDelay: '400ms' }}
        />
      </div>
    </div>
  );
}
