-- Add following system to track users we follow.
--
-- 001_initial_schema.sql created an early `following` table using
-- following_actor_id/following_inbox and an `approved` status. The actual
-- follow/timeline code standardized on target_actor_id/target_inbox and
-- `accepted`, so this migration upgrades the early table in place.
ALTER TABLE following RENAME TO following_legacy;

CREATE TABLE following (
    id TEXT PRIMARY KEY,              -- Follow activity ID
    actor_id TEXT NOT NULL,           -- Our actor ID (who is following)
    target_actor_id TEXT NOT NULL,    -- The actor we're following
    target_inbox TEXT NOT NULL,       -- Their inbox URL for sending activities
    status TEXT DEFAULT 'pending' CHECK(status IN ('pending', 'accepted', 'rejected')),
    created_at TEXT NOT NULL,
    accepted_at TEXT                  -- When they accepted our follow
);

INSERT INTO following (
    id, actor_id, target_actor_id, target_inbox, status, created_at, accepted_at
)
SELECT
    id,
    actor_id,
    following_actor_id,
    following_inbox,
    CASE WHEN status = 'approved' THEN 'accepted' ELSE status END,
    COALESCE(created_at, CURRENT_TIMESTAMP),
    CASE WHEN status = 'approved' THEN updated_at ELSE NULL END
FROM following_legacy;

DROP TABLE following_legacy;

-- Unique constraint on actor + target combination
CREATE UNIQUE INDEX IF NOT EXISTS idx_following_unique ON following(actor_id, target_actor_id);

-- Index for quick lookup
CREATE INDEX IF NOT EXISTS idx_following_actor ON following(actor_id);
CREATE INDEX IF NOT EXISTS idx_following_target ON following(target_actor_id);
CREATE INDEX IF NOT EXISTS idx_following_status ON following(status);
