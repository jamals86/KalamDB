-- KalamDB Chat with AI Application Schema
-- 
-- This script creates the necessary tables, topics, and users
-- for the chat application example.
--
-- Run with: ./setup.sh

-- ============================================================================
-- Namespace
-- ============================================================================
CREATE NAMESPACE IF NOT EXISTS chat;

-- ============================================================================
-- Tables
-- ============================================================================

-- Conversations table (shared table - all users see all conversations)
CREATE USER TABLE chat.conversations (
    id BIGINT NOT NULL DEFAULT SNOWFLAKE_ID() PRIMARY KEY,
    title TEXT NOT NULL,
    created_by TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Messages table (shared table - all users see all messages including AI replies)
CREATE USER TABLE chat.messages (
    id BIGINT NOT NULL DEFAULT SNOWFLAKE_ID() PRIMARY KEY,
    client_id TEXT,
    conversation_id BIGINT NOT NULL,
    sender TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'user',
    content TEXT NOT NULL,
    file_data FILE,
    status TEXT NOT NULL DEFAULT 'sent',
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Typing indicators table (shared table - all users see typing status)
CREATE STREAM TABLE chat.typing_indicators (
    id BIGINT NOT NULL DEFAULT SNOWFLAKE_ID() PRIMARY KEY,
    conversation_id BIGINT NOT NULL,
    user_name TEXT NOT NULL,
    is_typing BOOLEAN NOT NULL DEFAULT TRUE,
    state TEXT NOT NULL DEFAULT 'typing',
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
) WITH (TTL_SECONDS = 30);

-- ============================================================================
-- Topics (Message processing queue)
-- ============================================================================

-- Topic for AI processing
-- Automatically receives all message INSERTs via CDC
CREATE TOPIC "chat.ai-processing";

-- CDC: Route all INSERT events on chat.messages to the topic
-- The AI service will filter for role='user' messages only
ALTER TOPIC "chat.ai-processing" ADD SOURCE chat.messages ON INSERT WITH (payload = 'full');

-- ============================================================================
-- Sample Conversations
-- ============================================================================

INSERT INTO chat.conversations (title, created_by) VALUES
    ('Welcome to KalamDB Chat', 'demo-user'),
    ('Getting Started with AI', 'demo-user');
