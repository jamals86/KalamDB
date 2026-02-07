'use client';

import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';
import { Wifi, WifiOff, Loader2 } from 'lucide-react';
import type { ConnectionStatus } from '@/types';

interface ConnectionBadgeProps {
  status: ConnectionStatus;
  className?: string;
}

export function ConnectionBadge({ status, className }: ConnectionBadgeProps) {
  const config = {
    connected: {
      variant: 'default' as const,
      icon: Wifi,
      label: 'Connected',
      color: 'bg-green-500/10 text-green-700 border-green-500/20',
    },
    connecting: {
      variant: 'secondary' as const,
      icon: Loader2,
      label: 'Connecting...',
      color: 'bg-yellow-500/10 text-yellow-700 border-yellow-500/20',
    },
    reconnecting: {
      variant: 'secondary' as const,
      icon: Loader2,
      label: 'Reconnecting...',
      color: 'bg-yellow-500/10 text-yellow-700 border-yellow-500/20',
    },
    disconnected: {
      variant: 'destructive' as const,
      icon: WifiOff,
      label: 'Disconnected',
      color: 'bg-red-500/10 text-red-700 border-red-500/20',
    },
    error: {
      variant: 'destructive' as const,
      icon: WifiOff,
      label: 'Error',
      color: 'bg-red-500/10 text-red-700 border-red-500/20',
    },
  };

  const { icon: Icon, label, color } = config[status];
  const isAnimating = status === 'connecting' || status === 'reconnecting';

  return (
    <Badge
      variant="outline"
      className={cn('gap-1.5 text-xs font-normal', color, className)}
    >
      <Icon className={cn('h-3 w-3', isAnimating && 'animate-spin')} />
      {label}
    </Badge>
  );
}
