-- KalamDB TODO Application Schema
-- Feature: 006-docker-wasm-examples
-- 
-- This script creates the todos table for the React example application
-- Run with: kalam user create --name "demo" --role "user" to get API key first
-- Then: kalam exec --file todo-app.sql

-- Create namespace if it doesn't exist
CREATE NAMESPACE IF NOT EXISTS app;

-- Create USER TABLE (one table instance per user with isolated storage)
CREATE USER TABLE app.todos (
    id BIGINT NOT NULL DEFAULT SNOWFLAKE_ID(),
    title TEXT NOT NULL,
    completed BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
) STORAGE local FLUSH ROW_THRESHOLD 1000;

-- Insert sample data for testing
-- These will be inserted into the current user's isolated table
INSERT INTO app.todos (title, completed) VALUES 
    ('Learn KalamDB basics', true),
    ('Build a React app with WASM', false),
    ('Deploy to production', false),
    ('Test real-time sync across tabs', false),
    ('Explore offline-first capabilities', false);
