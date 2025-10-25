-- TODO App Database Schema
-- Creates a simple todos table with auto-incrementing ID in the app namespace
-- Using USER table type for per-user data isolation
-- Note: Requires X-USER-ID header when executing (e.g., X-USER-ID: 1)

CREATE USER TABLE IF NOT EXISTS app.todos (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    title TEXT NOT NULL,
    completed BOOLEAN NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL
);
