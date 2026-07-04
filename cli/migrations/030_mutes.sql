-- Persist Mastodon-compatible account mute state
CREATE TABLE IF NOT EXISTS mutes (
    id TEXT PRIMARY KEY,
    actor_id TEXT NOT NULL,
    reason TEXT,
    created_at TEXT NOT NULL,
    UNIQUE(actor_id)
);

CREATE INDEX IF NOT EXISTS idx_mutes_actor_id ON mutes(actor_id);
