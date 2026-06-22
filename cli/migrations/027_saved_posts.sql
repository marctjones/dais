-- Owner-only saved posts and bookmarks.

CREATE TABLE IF NOT EXISTS saved_posts (
    id TEXT PRIMARY KEY,
    post_id TEXT,
    object_id TEXT,
    canonical_url TEXT,
    title TEXT,
    excerpt TEXT,
    source TEXT NOT NULL DEFAULT 'owner',
    saved_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    raw_item TEXT
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_saved_posts_post_id
    ON saved_posts(post_id)
    WHERE post_id IS NOT NULL AND post_id != '';

CREATE UNIQUE INDEX IF NOT EXISTS idx_saved_posts_object_id
    ON saved_posts(object_id)
    WHERE object_id IS NOT NULL AND object_id != '';

CREATE UNIQUE INDEX IF NOT EXISTS idx_saved_posts_canonical_url
    ON saved_posts(canonical_url)
    WHERE canonical_url IS NOT NULL AND canonical_url != '';

CREATE INDEX IF NOT EXISTS idx_saved_posts_saved_at ON saved_posts(saved_at DESC);
