-- Initial schema for dais.social ActivityPub server
-- Migration: 001_initial_schema
-- Created: 2026-01-04

-- Actors table: Store user/server information
CREATE TABLE IF NOT EXISTS actors (
    id TEXT PRIMARY KEY,
    username TEXT UNIQUE NOT NULL,
    display_name TEXT,
    summary TEXT,
    avatar_url TEXT,
    header_url TEXT,
    public_key TEXT NOT NULL,
    private_key TEXT NOT NULL,  -- Encrypted at rest in production
    inbox_url TEXT NOT NULL,
    outbox_url TEXT NOT NULL,
    followers_url TEXT NOT NULL,
    following_url TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Followers table: Track who follows this actor
CREATE TABLE IF NOT EXISTS followers (
    id TEXT PRIMARY KEY,
    actor_id TEXT NOT NULL,
    follower_actor_id TEXT NOT NULL,  -- Full actor URL (e.g., https://mastodon.social/users/alice)
    follower_inbox TEXT NOT NULL,     -- Inbox URL for delivering activities
    follower_shared_inbox TEXT,       -- Shared inbox (optional, for efficiency)
    status TEXT NOT NULL DEFAULT 'pending',  -- 'pending', 'approved', 'rejected'
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (actor_id) REFERENCES actors(id),
    UNIQUE(actor_id, follower_actor_id)
);

-- Following table: Track who this actor follows (for future use)
CREATE TABLE IF NOT EXISTS following (
    id TEXT PRIMARY KEY,
    actor_id TEXT NOT NULL,
    following_actor_id TEXT NOT NULL,
    following_inbox TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',  -- 'pending', 'approved', 'rejected'
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (actor_id) REFERENCES actors(id),
    UNIQUE(actor_id, following_actor_id)
);

-- Posts table: Store published content
CREATE TABLE IF NOT EXISTS posts (
    id TEXT PRIMARY KEY,
    actor_id TEXT NOT NULL,
    content TEXT NOT NULL,
    content_html TEXT,  -- Rendered HTML version
    visibility TEXT NOT NULL DEFAULT 'public',  -- 'public', 'unlisted', 'followers', 'direct'
    in_reply_to TEXT,   -- URL of post this is replying to
    media_attachments TEXT,  -- JSON array of media attachment URLs
    published_at DATETIME NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (actor_id) REFERENCES actors(id)
);

-- Activities table: Log of all ActivityPub activities (for debugging/audit)
CREATE TABLE IF NOT EXISTS activities (
    id TEXT PRIMARY KEY,
    type TEXT NOT NULL,  -- 'Follow', 'Accept', 'Reject', 'Create', 'Like', 'Announce', etc.
    actor TEXT NOT NULL,  -- Actor URL who performed the activity
    object TEXT,          -- Object of the activity (JSON or URL)
    target TEXT,          -- Target of the activity (optional)
    published DATETIME NOT NULL,
    received_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Create indexes for common queries
CREATE INDEX IF NOT EXISTS idx_followers_actor_status ON followers(actor_id, status);
CREATE INDEX IF NOT EXISTS idx_followers_actor_follower ON followers(actor_id, follower_actor_id);
CREATE INDEX IF NOT EXISTS idx_posts_actor_published ON posts(actor_id, published_at DESC);
CREATE INDEX IF NOT EXISTS idx_posts_visibility ON posts(visibility);
CREATE INDEX IF NOT EXISTS idx_activities_type ON activities(type);
CREATE INDEX IF NOT EXISTS idx_activities_actor ON activities(actor);
CREATE INDEX IF NOT EXISTS idx_activities_published ON activities(published DESC);
