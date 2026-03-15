-- Bluesky Chat Support (separate from ActivityPub DMs)
-- Migration: 008_bluesky_chat
-- Created: 2026-03-12

-- Bluesky conversations (chat.bsky.convo.*)
CREATE TABLE IF NOT EXISTS bluesky_conversations (
    id TEXT PRIMARY KEY,                  -- Conversation ID from Bluesky API
    participants TEXT NOT NULL,           -- JSON array of DIDs
    created_at TEXT NOT NULL,
    muted BOOLEAN DEFAULT FALSE,
    unread_count INTEGER DEFAULT 0,
    last_message_at TEXT,
    last_message_text TEXT                -- Preview for list view
);

-- Bluesky messages within conversations
CREATE TABLE IF NOT EXISTS bluesky_messages (
    id TEXT PRIMARY KEY,                  -- Message ID from Bluesky API
    conversation_id TEXT NOT NULL,
    sender_did TEXT NOT NULL,             -- DID of sender
    sender_handle TEXT,                   -- Handle for display (e.g., alice.bsky.social)
    text TEXT NOT NULL,
    sent_at TEXT NOT NULL,
    read BOOLEAN DEFAULT FALSE,
    FOREIGN KEY (conversation_id) REFERENCES bluesky_conversations(id) ON DELETE CASCADE
);

-- Create indexes for efficient queries
CREATE INDEX IF NOT EXISTS idx_bluesky_conversations_last_message ON bluesky_conversations(last_message_at DESC);
CREATE INDEX IF NOT EXISTS idx_bluesky_messages_conversation ON bluesky_messages(conversation_id);
CREATE INDEX IF NOT EXISTS idx_bluesky_messages_sent ON bluesky_messages(sent_at DESC);
CREATE INDEX IF NOT EXISTS idx_bluesky_messages_read ON bluesky_messages(read);

-- Note: This is separate from the ActivityPub direct_messages table
-- ActivityPub DMs use: conversations + direct_messages (migration 007)
-- Bluesky Chats use: bluesky_conversations + bluesky_messages (this migration)
