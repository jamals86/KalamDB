// Shared types for the Chat with AI application

export interface Conversation {
  id: string;
  title: string;
  created_by: string;
  created_at: string;
  updated_at: string;
}

export interface FileAttachment {
  id: string;
  name: string;
  size: number;
  mime_type: string;
  url: string;
}

export interface FileRef {
  id: string;
  sub: string;
  name: string;
  size: number;
  mime: string;
  sha256?: string;
  shard?: number;
}

export interface Message {
  id: string;
  client_id?: string;
  conversation_id: string;
  sender: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  status: 'sending' | 'sent' | 'delivered' | 'error';
  created_at: string;
  files?: FileAttachment[];
}

export interface TypingIndicator {
  id: string;
  conversation_id: string;
  user_name: string;
  is_typing: boolean;
  state?: 'typing' | 'thinking' | 'finished';
  updated_at: string;
}

export interface CreateConversationInput {
  title: string;
}

export interface SendMessageInput {
  conversation_id: string;
  content: string;
  files?: File[];
}

// Token streaming types for animation
export interface StreamToken {
  token: string;
  index: number;
  done: boolean;
}

export interface StreamingMessage {
  messageId: string;
  conversationId: string;
  tokens: string[];
  isComplete: boolean;
}

// Connection status
export type ConnectionStatus = 'connecting' | 'connected' | 'disconnected' | 'reconnecting' | 'error';

// Subscription event types (from kalam-link SDK)
export interface SubscriptionEvent<T> {
  type: 'initial_data_batch' | 'change' | 'subscription_ack' | 'error';
  rows?: T[];
  change_type?: 'insert' | 'update' | 'delete';
  old_values?: T[];
  batch_control?: {
    batch_num: number;
    has_more: boolean;
    status: string;
  };
}
