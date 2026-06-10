-- Private Mode M2: local home timeline from signed inbox delivery.

CREATE TABLE IF NOT EXISTS timeline_posts (
    id TEXT PRIMARY KEY,
    object_id TEXT NOT NULL UNIQUE,
    actor_id TEXT NOT NULL,
    actor_username TEXT,
    actor_display_name TEXT,
    actor_avatar_url TEXT,
    content TEXT NOT NULL,
    content_html TEXT,
    visibility TEXT NOT NULL DEFAULT 'unknown',
    in_reply_to TEXT,
    published_at TEXT NOT NULL,
    updated_at TEXT,
    deleted_at TEXT,
    raw_object TEXT,
    raw_activity TEXT,
    protocol TEXT NOT NULL DEFAULT 'activitypub'
        CHECK(protocol IN ('activitypub', 'atproto')),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_timeline_posts_actor ON timeline_posts(actor_id);
CREATE INDEX IF NOT EXISTS idx_timeline_posts_published ON timeline_posts(published_at DESC);
CREATE INDEX IF NOT EXISTS idx_timeline_posts_deleted ON timeline_posts(deleted_at);
CREATE INDEX IF NOT EXISTS idx_timeline_posts_protocol ON timeline_posts(protocol);
