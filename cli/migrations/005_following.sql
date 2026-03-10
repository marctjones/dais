-- Add following system to track users we follow
CREATE TABLE IF NOT EXISTS following (
    id TEXT PRIMARY KEY,              -- Follow activity ID
    actor_id TEXT NOT NULL,           -- Our actor ID (who is following)
    target_actor_id TEXT NOT NULL,    -- The actor we're following
    target_inbox TEXT NOT NULL,       -- Their inbox URL for sending activities
    status TEXT DEFAULT 'pending' CHECK(status IN ('pending', 'accepted', 'rejected')),
    created_at TEXT NOT NULL,
    accepted_at TEXT                  -- When they accepted our follow
);

-- Unique constraint on actor + target combination
CREATE UNIQUE INDEX IF NOT EXISTS idx_following_unique ON following(actor_id, target_actor_id);

-- Index for quick lookup
CREATE INDEX IF NOT EXISTS idx_following_actor ON following(actor_id);
CREATE INDEX IF NOT EXISTS idx_following_target ON following(target_actor_id);
CREATE INDEX IF NOT EXISTS idx_following_status ON following(status);
