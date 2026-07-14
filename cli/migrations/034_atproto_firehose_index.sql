-- Personal Bluesky AppView (issue #50, Track B): index of firehose-observed
-- activity for accounts the owner follows. `timeline_posts` (010) already has
-- a `protocol` column that accepts 'atproto' and a cross-protocol home
-- timeline query -- posts and replies from followed accounts are written
-- there as protocol='atproto' rows rather than into a separate table.
-- Likes and follows have no owner-authored analog to extend, so they get
-- their own tables here.

CREATE TABLE IF NOT EXISTS atproto_likes (
    uri TEXT PRIMARY KEY,
    actor_did TEXT NOT NULL,
    subject_uri TEXT NOT NULL,
    subject_cid TEXT,
    created_at TEXT NOT NULL,
    indexed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_atproto_likes_subject ON atproto_likes(subject_uri);
CREATE INDEX IF NOT EXISTS idx_atproto_likes_actor ON atproto_likes(actor_did, created_at DESC);

CREATE TABLE IF NOT EXISTS atproto_follows (
    uri TEXT PRIMARY KEY,
    actor_did TEXT NOT NULL,
    subject_did TEXT NOT NULL,
    created_at TEXT NOT NULL,
    indexed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_atproto_follows_subject ON atproto_follows(subject_did);
CREATE INDEX IF NOT EXISTS idx_atproto_follows_actor ON atproto_follows(actor_did);

-- Singleton row tracking the firehose consumer's cursor and a daily
-- active-time budget as a runaway-loop backstop, not a routine duty cycle:
-- the consumer is designed to hold one always-on relay connection since
-- `subscribeRepos` has no server-side DID filter, so a bounded duty cycle
-- would have to decode the entire network's commit volume to catch up on
-- every reconnect rather than just the owner's follows.
CREATE TABLE IF NOT EXISTS atproto_firehose_checkpoint (
    id TEXT PRIMARY KEY DEFAULT 'default',
    last_seq INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'stopped'
        CHECK(status IN ('running', 'stopped', 'error', 'budget_exceeded')),
    reconnect_count INTEGER NOT NULL DEFAULT 0,
    budget_date TEXT NOT NULL DEFAULT (DATE('now')),
    active_seconds_today INTEGER NOT NULL DEFAULT 0,
    daily_budget_seconds INTEGER NOT NULL DEFAULT 79200,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
