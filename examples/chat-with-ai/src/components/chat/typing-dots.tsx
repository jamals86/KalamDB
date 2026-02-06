'use client';

import { cn } from '@/lib/utils';

interface TypingDotsProps {
  className?: string;
  label?: string;
}

export function TypingDots({ className, label }: TypingDotsProps) {
  return (
    <div className={cn('flex items-center gap-1', className)}>
      {label && (
        <span className="text-xs text-muted-foreground mr-2">{label}</span>
      )}
      <span
        className="w-2 h-2 rounded-full bg-muted-foreground/60 animate-typing-dot"
        style={{ animationDelay: '0ms' }}
      />
      <span
        className="w-2 h-2 rounded-full bg-muted-foreground/60 animate-typing-dot"
        style={{ animationDelay: '200ms' }}
      />
      <span
        className="w-2 h-2 rounded-full bg-muted-foreground/60 animate-typing-dot"
        style={{ animationDelay: '400ms' }}
      />
    </div>
  );
}
