-- Interactions schema for Phase 3
-- Migration: 002_interactions
-- Created: 2026-03-09

-- Replies table: Store replies to your posts from other actors
CREATE TABLE IF NOT EXISTS replies (
    id TEXT PRIMARY KEY,                  -- Reply post URL
    post_id TEXT NOT NULL,                -- Your post being replied to
    actor_id TEXT NOT NULL,               -- Actor URL who replied
    actor_username TEXT,                  -- Username for display (e.g., @user@domain)
    actor_display_name TEXT,              -- Display name for rendering
    actor_avatar_url TEXT,                -- Avatar for display
    content TEXT NOT NULL,                -- Reply content (HTML)
    published_at TEXT NOT NULL,           -- When reply was published
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (post_id) REFERENCES posts(id) ON DELETE CASCADE
);

-- Interactions table: Store likes and boosts
CREATE TABLE IF NOT EXISTS interactions (
    id TEXT PRIMARY KEY,                  -- Activity ID
    type TEXT NOT NULL CHECK(type IN ('like', 'boost')),
    actor_id TEXT NOT NULL,               -- Actor URL who liked/boosted
    actor_username TEXT,                  -- Username for display
    actor_display_name TEXT,              -- Display name for rendering
    actor_avatar_url TEXT,                -- Avatar for display
    post_id TEXT,                         -- Your post being interacted with
    object_url TEXT,                      -- Original object URL (for boosts of other posts)
    created_at TEXT NOT NULL,             -- When interaction happened
    FOREIGN KEY (post_id) REFERENCES posts(id) ON DELETE CASCADE
);

-- Notifications table: Store user notifications
CREATE TABLE IF NOT EXISTS notifications (
    id TEXT PRIMARY KEY,                  -- Notification ID (UUID)
    type TEXT NOT NULL CHECK(type IN ('mention', 'reply', 'like', 'boost', 'follow')),
    actor_id TEXT NOT NULL,               -- Actor who triggered notification
    actor_username TEXT,                  -- Username for display
    actor_display_name TEXT,              -- Display name
    actor_avatar_url TEXT,                -- Avatar
    post_id TEXT,                         -- Related post (if applicable)
    activity_id TEXT,                     -- Related activity ID
    content TEXT,                         -- Notification text/preview
    read BOOLEAN DEFAULT FALSE,           -- Has user seen this?
    created_at TEXT NOT NULL              -- When notification was created
);

-- Add icon and image columns to actors if they don't exist
-- (Profile avatar and header - may already exist from manual additions)
ALTER TABLE actors ADD COLUMN icon TEXT;
ALTER TABLE actors ADD COLUMN image TEXT;

-- Create indexes for efficient queries
CREATE INDEX IF NOT EXISTS idx_replies_post_id ON replies(post_id);
CREATE INDEX IF NOT EXISTS idx_replies_published ON replies(published_at DESC);
CREATE INDEX IF NOT EXISTS idx_interactions_post_id ON interactions(post_id);
CREATE INDEX IF NOT EXISTS idx_interactions_type ON interactions(type);
CREATE INDEX IF NOT EXISTS idx_interactions_created ON interactions(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_notifications_type ON notifications(type);
CREATE INDEX IF NOT EXISTS idx_notifications_read ON notifications(read);
CREATE INDEX IF NOT EXISTS idx_notifications_created ON notifications(created_at DESC);
