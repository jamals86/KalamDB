/**
 * ConnectionStatus Component
 * Shows connection status badge with color indicator
 */

import React from 'react';

interface ConnectionStatusProps {
  isConnected: boolean;
}

export function ConnectionStatus({ isConnected }: ConnectionStatusProps) {
  return (
    <div className="connection-status">
      <span className={`status-badge ${isConnected ? 'connected' : 'disconnected'}`}>
        {isConnected ? '🟢 Connected' : '🔴 Disconnected'}
      </span>
    </div>
  );
}
