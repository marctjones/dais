-- Direct Messages schema for Phase 3 (DMs)
-- Migration: 007_direct_messages
-- Created: 2026-03-12

-- Conversations table: Groups messages between the same set of participants
CREATE TABLE IF NOT EXISTS conversations (
    id TEXT PRIMARY KEY,                  -- Hash of sorted participant URLs
    participants TEXT NOT NULL,           -- JSON array of actor URLs
    last_message_at TEXT,                 -- Timestamp of most recent message
    created_at TEXT DEFAULT (datetime('now'))
);

-- Direct messages table: Individual messages within conversations
CREATE TABLE IF NOT EXISTS direct_messages (
    id TEXT PRIMARY KEY,                  -- Message URL/ID
    conversation_id TEXT NOT NULL,        -- Links to conversation
    sender_id TEXT NOT NULL,              -- Actor URL who sent this
    content TEXT NOT NULL,                -- Message content
    published_at TEXT NOT NULL,           -- When message was sent
    created_at TEXT DEFAULT (datetime('now')),
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
);

-- Conversation participants: Track read status per participant
CREATE TABLE IF NOT EXISTS conversation_participants (
    conversation_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,               -- Participant's actor URL
    last_read_at TEXT,                    -- When they last read messages
    PRIMARY KEY (conversation_id, actor_id),
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
);

-- Create indexes for efficient queries
CREATE INDEX IF NOT EXISTS idx_conversations_last_message ON conversations(last_message_at DESC);
CREATE INDEX IF NOT EXISTS idx_dm_conversation ON direct_messages(conversation_id);
CREATE INDEX IF NOT EXISTS idx_dm_published ON direct_messages(published_at DESC);
CREATE INDEX IF NOT EXISTS idx_conv_participants_actor ON conversation_participants(actor_id);
