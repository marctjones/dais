-- Add protocol support to posts table
ALTER TABLE posts ADD COLUMN protocol TEXT DEFAULT 'activitypub'
    CHECK(protocol IN ('activitypub', 'atproto', 'both'));

-- Add AT Protocol specific fields
ALTER TABLE posts ADD COLUMN atproto_uri TEXT;
ALTER TABLE posts ADD COLUMN atproto_cid TEXT;

-- Delivery tracking table for queue-based delivery
CREATE TABLE IF NOT EXISTS deliveries (
    id TEXT PRIMARY KEY,
    post_id TEXT NOT NULL,
    target_type TEXT NOT NULL CHECK(target_type IN ('inbox', 'did')),
    target_url TEXT NOT NULL,
    protocol TEXT NOT NULL CHECK(protocol IN ('activitypub', 'atproto')),
    status TEXT DEFAULT 'queued' CHECK(status IN ('queued', 'delivered', 'failed', 'retry')),
    retry_count INTEGER DEFAULT 0,
    last_attempt_at TEXT,
    error_message TEXT,
    created_at TEXT NOT NULL,
    delivered_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_deliveries_post_id ON deliveries(post_id);
CREATE INDEX IF NOT EXISTS idx_deliveries_status ON deliveries(status);
CREATE INDEX IF NOT EXISTS idx_deliveries_retry ON deliveries(status, retry_count);

-- AT Protocol configuration table
CREATE TABLE IF NOT EXISTS atproto_config (
    id INTEGER PRIMARY KEY DEFAULT 1,
    handle TEXT NOT NULL,
    did TEXT NOT NULL,
    last_sync_at TEXT,
    CHECK (id = 1)
);
