-- Add blocking system for managing abusive users and instances
CREATE TABLE IF NOT EXISTS blocks (
    id TEXT PRIMARY KEY,
    actor_id TEXT NOT NULL,  -- The actor being blocked
    blocked_domain TEXT,     -- If blocking an entire domain (e.g., "spam.social")
    reason TEXT,             -- Optional reason for the block
    created_at TEXT NOT NULL,
    UNIQUE(actor_id)
);

-- Index for quick block lookups
CREATE INDEX IF NOT EXISTS idx_blocks_actor_id ON blocks(actor_id);
CREATE INDEX IF NOT EXISTS idx_blocks_domain ON blocks(blocked_domain);

-- Add block checking helper
-- Note: Actual checking will be done in application code
