-- Public source subscriptions and normalized reader items.
-- Ingestion is private reader state by default; reposting/federation remains a
-- separate explicit owner action.

CREATE TABLE IF NOT EXISTS source_subscriptions (
    id TEXT PRIMARY KEY,
    source_type TEXT NOT NULL CHECK(source_type IN ('rss', 'atom', 'activitypub', 'api')),
    url TEXT NOT NULL UNIQUE,
    title TEXT,
    homepage_url TEXT,
    status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active', 'paused', 'error')),
    refresh_cadence_minutes INTEGER NOT NULL DEFAULT 60,
    last_fetched_at TEXT,
    next_fetch_at TEXT,
    etag TEXT,
    last_modified TEXT,
    last_error TEXT,
    error_count INTEGER NOT NULL DEFAULT 0,
    policy_json TEXT NOT NULL DEFAULT '{}',
    api_secret_name TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS source_items (
    id TEXT PRIMARY KEY,
    source_id TEXT NOT NULL REFERENCES source_subscriptions(id) ON DELETE CASCADE,
    source_type TEXT NOT NULL,
    title TEXT NOT NULL,
    canonical_url TEXT,
    external_id TEXT,
    author TEXT,
    published_at TEXT,
    fetched_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    excerpt TEXT,
    content_type TEXT,
    hash TEXT NOT NULL,
    thumbnail_url TEXT,
    rights_policy_json TEXT NOT NULL DEFAULT '{}',
    read INTEGER NOT NULL DEFAULT 0 CHECK(read IN (0, 1)),
    summary TEXT,
    raw_metadata_json TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_source_items_source_external
ON source_items(source_id, external_id)
WHERE external_id IS NOT NULL AND external_id != '';

CREATE UNIQUE INDEX IF NOT EXISTS idx_source_items_source_canonical
ON source_items(source_id, canonical_url)
WHERE canonical_url IS NOT NULL AND canonical_url != '';

CREATE INDEX IF NOT EXISTS idx_source_items_published
ON source_items(published_at DESC, fetched_at DESC);

CREATE INDEX IF NOT EXISTS idx_source_subscriptions_next_fetch
ON source_subscriptions(status, next_fetch_at);
