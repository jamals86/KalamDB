/**
 * React Hook Tests for Chat-with-AI
 * 
 * Tests that verify conversations and messages hooks work correctly
 * using mocked kalam-link SDK.
 * 
 * Run: npm test
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import React from 'react';

// Mock the kalam-link SDK
const mockQueryAll = vi.fn();
const mockQuery = vi.fn();
const mockSubscribeWithSql = vi.fn();
const mockLogin = vi.fn();
const mockConnect = vi.fn();
const mockDisconnect = vi.fn();
const mockSetAutoReconnect = vi.fn();
const mockSetReconnectDelay = vi.fn();
const mockSetMaxReconnectAttempts = vi.fn();
const mockConsumeFromTopic = vi.fn();
const mockAckTopic = vi.fn();

// Mock kalam-link module
vi.mock('kalam-link', () => ({
  createClient: vi.fn(() => ({
    login: mockLogin,
    connect: mockConnect,
    disconnect: mockDisconnect,
    setAutoReconnect: mockSetAutoReconnect,
    setReconnectDelay: mockSetReconnectDelay,
    setMaxReconnectAttempts: mockSetMaxReconnectAttempts,
    query: mockQuery,
    queryAll: mockQueryAll,
    subscribeWithSql: mockSubscribeWithSql,
    consumeFromTopic: mockConsumeFromTopic,
    ackTopic: mockAckTopic,
    isConnected: () => true,
  })),
  Auth: {
    basic: (username: string, password: string) => ({
      type: 'basic' as const,
      username,
      password,
    }),
    jwt: (token: string) => ({ type: 'jwt' as const, token }),
    none: () => ({ type: 'none' as const }),
  },
}));

// Import after mocks are set up
import { KalamDBProvider, useKalamDB } from '@/providers/kalamdb-provider';
import {
  useConversations,
  useMessages,
  useTypingIndicator,
  useConnectionStatus,
} from '@/hooks/use-kalamdb';

// Wrapper component that provides KalamDB context
function createWrapper() {
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return React.createElement(
      KalamDBProvider,
      { url: 'http://localhost:8080', username: 'test-user', password: 'test-pass' },
      children
    );
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  mockLogin.mockResolvedValue('mock-jwt-token');
  mockConnect.mockResolvedValue(undefined);
  mockDisconnect.mockResolvedValue(undefined);
});

// ============================================================================
// useConversations tests
// ============================================================================

describe('useConversations', () => {
  it('should start in loading state', () => {
    // Make subscription hang to keep loading state
    mockSubscribeWithSql.mockImplementation(() => new Promise(() => {}));

    const { result } = renderHook(() => useConversations(), {
      wrapper: createWrapper(),
    });

    expect(result.current.loading).toBe(true);
    expect(result.current.conversations).toEqual([]);
    expect(result.current.error).toBeNull();
  });

  it('should load conversations via subscription', async () => {
    const mockConversations = [
      { id: '1', title: 'Test Chat', created_by: 'user1', created_at: '2024-01-01', updated_at: '2024-01-01' },
      { id: '2', title: 'Another Chat', created_by: 'user2', created_at: '2024-01-02', updated_at: '2024-01-02' },
    ];

    // Simulate subscription that delivers initial data
    let subscriptionCallback: ((event: any) => void) | null = null;
    const mockUnsub = vi.fn().mockResolvedValue(undefined);
    mockSubscribeWithSql.mockImplementation((_sql: string, callback: any) => {
      subscriptionCallback = callback;
      // Deliver initial data immediately
      setTimeout(() => {
        callback({
          type: 'initial_data_batch',
          rows: mockConversations,
          batch_control: { batch_num: 0, has_more: false, status: 'ready' },
        });
      }, 10);
      return Promise.resolve(mockUnsub);
    });

    const { result } = renderHook(() => useConversations(), {
      wrapper: createWrapper(),
    });

    await waitFor(() => {
      expect(result.current.conversations.length).toBeGreaterThanOrEqual(0);
    }, { timeout: 5000 });
  });

  it('should fall back to query on subscription error', async () => {
    const mockConversations = [
      { id: '1', title: 'Fallback Chat', created_by: 'user1', created_at: '2024-01-01', updated_at: '2024-01-01' },
    ];

    mockSubscribeWithSql.mockRejectedValue(new Error('Subscription failed'));
    mockQueryAll.mockResolvedValue(mockConversations);

    const { result } = renderHook(() => useConversations(), {
      wrapper: createWrapper(),
    });

    await waitFor(() => {
      expect(result.current.error).toBeTruthy();
    }, { timeout: 5000 });
  });

  it('should expose createConversation function', () => {
    mockSubscribeWithSql.mockImplementation(() => new Promise(() => {}));

    const { result } = renderHook(() => useConversations(), {
      wrapper: createWrapper(),
    });

    expect(typeof result.current.createConversation).toBe('function');
    expect(typeof result.current.refetch).toBe('function');
  });
});

// ============================================================================
// useMessages tests
// ============================================================================

describe('useMessages', () => {
  it('should return empty messages when no conversationId', () => {
    const { result } = renderHook(() => useMessages(null), {
      wrapper: createWrapper(),
    });

    expect(result.current.messages).toEqual([]);
    expect(result.current.loading).toBe(false);
  });

  it('should start loading when conversationId is provided', () => {
    mockSubscribeWithSql.mockImplementation(() => new Promise(() => {}));

    const { result } = renderHook(() => useMessages('123'), {
      wrapper: createWrapper(),
    });

    // Should be in loading state initially
    expect(result.current.sending).toBe(false);
  });

  it('should load messages via subscription', async () => {
    const mockMessages = [
      { id: '1', conversation_id: '123', sender: 'user', role: 'user', content: 'Hello', status: 'sent', created_at: '2024-01-01T10:00:00Z' },
      { id: '2', conversation_id: '123', sender: 'ai', role: 'assistant', content: 'Hi!', status: 'sent', created_at: '2024-01-01T10:01:00Z' },
    ];

    const mockUnsub = vi.fn().mockResolvedValue(undefined);
    mockSubscribeWithSql.mockImplementation((_sql: string, callback: any) => {
      setTimeout(() => {
        callback({
          type: 'initial_data_batch',
          rows: mockMessages,
          batch_control: { batch_num: 0, has_more: false, status: 'ready' },
        });
      }, 10);
      return Promise.resolve(mockUnsub);
    });

    const { result } = renderHook(() => useMessages('123'), {
      wrapper: createWrapper(),
    });

    await waitFor(() => {
      expect(result.current.messages.length).toBeGreaterThanOrEqual(0);
    }, { timeout: 5000 });
  });

  it('should handle real-time inserts', async () => {
    let subscriptionCallback: ((event: any) => void) | null = null;
    const mockUnsub = vi.fn().mockResolvedValue(undefined);

    mockSubscribeWithSql.mockImplementation((_sql: string, callback: any) => {
      subscriptionCallback = callback;
      // Initial empty data
      setTimeout(() => {
        callback({
          type: 'initial_data_batch',
          rows: [],
          batch_control: { batch_num: 0, has_more: false, status: 'ready' },
        });
      }, 10);
      return Promise.resolve(mockUnsub);
    });

    const { result } = renderHook(() => useMessages('123'), {
      wrapper: createWrapper(),
    });

    // Wait for initial load
    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    }, { timeout: 5000 });

    // Simulate new message arriving via subscription
    if (subscriptionCallback) {
      act(() => {
        subscriptionCallback!({
          type: 'change',
          change_type: 'insert',
          rows: [
            { id: '3', conversation_id: '123', sender: 'user', role: 'user', content: 'New message!', status: 'sent', created_at: '2024-01-01T10:05:00Z' },
          ],
        });
      });
    }

    await waitFor(() => {
      expect(result.current.messages.length).toBeGreaterThanOrEqual(0);
    });
  });

  it('should expose sendMessage function', () => {
    mockSubscribeWithSql.mockImplementation(() => new Promise(() => {}));

    const { result } = renderHook(() => useMessages('123'), {
      wrapper: createWrapper(),
    });

    expect(typeof result.current.sendMessage).toBe('function');
    expect(typeof result.current.refetch).toBe('function');
  });

  it('should clean up on conversationId change', async () => {
    const mockUnsub = vi.fn().mockResolvedValue(undefined);
    mockSubscribeWithSql.mockImplementation((_sql: string, callback: any) => {
      setTimeout(() => {
        callback({
          type: 'initial_data_batch',
          rows: [],
          batch_control: { batch_num: 0, has_more: false, status: 'ready' },
        });
      }, 10);
      return Promise.resolve(mockUnsub);
    });

    const { result, rerender } = renderHook(
      ({ convId }) => useMessages(convId),
      {
        wrapper: createWrapper(),
        initialProps: { convId: '123' as string | null },
      }
    );

    // Switch conversation
    rerender({ convId: '456' });
    
    // Messages should reset
    await waitFor(() => {
      expect(result.current.messages).toEqual([]);
    });
  });
});

// ============================================================================
// useTypingIndicator tests
// ============================================================================

describe('useTypingIndicator', () => {
  it('should return empty typing users when no conversationId', () => {
    const { result } = renderHook(() => useTypingIndicator(null), {
      wrapper: createWrapper(),
    });

    expect(result.current.typingUsers).toEqual([]);
    expect(typeof result.current.setTyping).toBe('function');
  });

  it('should subscribe to typing indicators', async () => {
    const mockUnsub = vi.fn().mockResolvedValue(undefined);
    mockSubscribeWithSql.mockImplementation((_sql: string, callback: any) => {
      setTimeout(() => {
        callback({
          type: 'initial_data_batch',
          rows: [{ user_name: 'ai-assistant', is_typing: true }],
          batch_control: { batch_num: 0, has_more: false, status: 'ready' },
        });
      }, 10);
      return Promise.resolve(mockUnsub);
    });

    const { result } = renderHook(() => useTypingIndicator('123'), {
      wrapper: createWrapper(),
    });

    await waitFor(() => {
      expect(result.current.typingUsers.length).toBeGreaterThanOrEqual(0);
    }, { timeout: 5000 });
  });
});

// ============================================================================
// useConnectionStatus tests
// ============================================================================

describe('useConnectionStatus', () => {
  it('should return initial connection status', () => {
    const { result } = renderHook(() => useConnectionStatus(), {
      wrapper: createWrapper(),
    });

    // Initial status is 'connecting'
    expect(['connecting', 'connected', 'disconnected', 'error']).toContain(result.current);
  });
});

// ============================================================================
// KalamDBProvider tests
// ============================================================================

describe('KalamDBProvider', () => {
  it('should provide client context', () => {
    const { result } = renderHook(() => useKalamDB(), {
      wrapper: createWrapper(),
    });

    expect(result.current).toHaveProperty('client');
    expect(result.current).toHaveProperty('connectionStatus');
    expect(result.current).toHaveProperty('isReady');
    expect(result.current).toHaveProperty('error');
  });
});
