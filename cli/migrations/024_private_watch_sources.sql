-- Private Watch sources are public-post monitors that do not create remote
-- follows, Bluesky graph records, notification subscriptions, or approval
-- requests. They reuse source_subscriptions/source_items as private reader
-- state while keeping watch source_type values explicit for clients.

PRAGMA foreign_keys = OFF;

ALTER TABLE source_items RENAME TO source_items_legacy_watch_migration;
ALTER TABLE source_subscriptions RENAME TO source_subscriptions_legacy_watch_migration;

CREATE TABLE source_subscriptions (
    id TEXT PRIMARY KEY,
    source_type TEXT NOT NULL CHECK(source_type IN (
        'rss',
        'atom',
        'activitypub',
        'api',
        'watch_rss',
        'watch_atom',
        'watch_activitypub_actor',
        'watch_activitypub_object',
        'watch_bluesky_actor',
        'watch_bluesky_post'
    )),
    url TEXT NOT NULL,
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

INSERT INTO source_subscriptions (
    id, source_type, url, title, homepage_url, status, refresh_cadence_minutes,
    last_fetched_at, next_fetch_at, etag, last_modified, last_error, error_count,
    policy_json, api_secret_name, created_at, updated_at
)
SELECT
    id, source_type, url, title, homepage_url, status, refresh_cadence_minutes,
    last_fetched_at, next_fetch_at, etag, last_modified, last_error, error_count,
    policy_json, api_secret_name, created_at, updated_at
FROM source_subscriptions_legacy_watch_migration;

CREATE TABLE source_items (
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

INSERT INTO source_items (
    id, source_id, source_type, title, canonical_url, external_id, author,
    published_at, fetched_at, excerpt, content_type, hash, thumbnail_url,
    rights_policy_json, read, summary, raw_metadata_json, created_at, updated_at
)
SELECT
    id, source_id, source_type, title, canonical_url, external_id, author,
    published_at, fetched_at, excerpt, content_type, hash, thumbnail_url,
    rights_policy_json, read, summary, raw_metadata_json, created_at, updated_at
FROM source_items_legacy_watch_migration;

DROP TABLE source_items_legacy_watch_migration;
DROP TABLE source_subscriptions_legacy_watch_migration;

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

CREATE UNIQUE INDEX IF NOT EXISTS idx_source_subscriptions_type_url
ON source_subscriptions(source_type, url);

CREATE INDEX IF NOT EXISTS idx_source_subscriptions_type
ON source_subscriptions(source_type, status, updated_at);

PRAGMA foreign_keys = ON;
