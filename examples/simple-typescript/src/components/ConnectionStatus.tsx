/**
 * ConnectionStatus Component
 * Feature: 006-docker-wasm-examples
 * 
 * Displays the current WebSocket connection status with visual indicator
 */

import type { ConnectionStatus as Status } from '../types/todo';

interface ConnectionStatusProps {
  status: Status;
}

export function ConnectionStatus({ status }: ConnectionStatusProps) {
  const getStatusConfig = (status: Status) => {
    switch (status) {
      case 'connected':
        return {
          label: 'Connected',
          color: 'bg-green-500',
          icon: '●',
          description: 'Real-time sync active'
        };
      case 'connecting':
        return {
          label: 'Connecting...',
          color: 'bg-yellow-500',
          icon: '◐',
          description: 'Establishing connection'
        };
      case 'disconnected':
        return {
          label: 'Disconnected',
          color: 'bg-gray-400',
          icon: '○',
          description: 'Read-only mode'
        };
      case 'error':
        return {
          label: 'Error',
          color: 'bg-red-500',
          icon: '✕',
          description: 'Connection failed'
        };
    }
  };

  const config = getStatusConfig(status);

  return (
    <div className="connection-status" title={config.description}>
      <span className={`status-indicator ${config.color}`}>
        {config.icon}
      </span>
      <span className="status-label">{config.label}</span>
    </div>
  );
}
